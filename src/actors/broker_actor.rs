//! `BrokerActor` — supervised owner of the broker inbox + subscriptions (ADR-041).
//!
//! Responsibilities:
//! - Cache the hot inbox (cases the broker is actively looking at).
//! - Apply domain-level operations (`claim`, `decide`, `submit`) via the
//!   `DecisionOrchestrator` so invariants are enforced consistently.
//! - Broadcast `broker:*` WebSocket events to subscribed clients through the
//!   `ClientCoordinatorActor`.
//!
//! The actor is stateless with respect to persistence: it delegates reads /
//! writes to the `BrokerRepository` port (Neo4j in production, in-memory in
//! tests). Supervision metadata is exposed via `SupervisedActorInfo` so the
//! system supervisor can restart it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use actix::prelude::*;
use log::{debug, error, info, warn};
use serde_json::json;

use crate::actors::messages::broker_messages::{
    BrokerChannel, ClaimBrokerCase, DecideBrokerCase, GetBrokerCase, ListBrokerInbox,
    SubmitBrokerCase, SubscribeBrokerChannel, UnsubscribeBrokerChannel,
};
use crate::actors::messages::BroadcastMessage;
use crate::actors::server_nostr_actor::SignBrokerDecision;
use crate::actors::ClientCoordinatorActor;
use crate::actors::ServerNostrActor;
use crate::domain::broker::{
    BrokerCase, CaseCategory, CaseInvariantError, DecisionOrchestrator, DecisionOutcome,
};
use crate::ports::broker_repository::BrokerRepository;
use crate::services::git_ingest::writeback_saga::{
    DecisionReport, EnrichmentPayload, EnrichmentType, WriteBackSaga,
};

// ---------------------------------------------------------------------------
// Actor
// ---------------------------------------------------------------------------

const INBOX_MAX_CAPACITY: usize = 10_000;
const INBOX_PRUNE_AGE_SECS: i64 = 86_400; // 24 hours

pub struct BrokerActor {
    inbox: HashMap<String, BrokerCase>,
    orchestrator: DecisionOrchestrator,
    repository: Option<Arc<dyn BrokerRepository>>,
    client_coordinator: Option<Addr<ClientCoordinatorActor>>,
    nostr_actor: Option<Addr<ServerNostrActor>>,
    subscribers: HashMap<BrokerChannel, HashSet<String>>,
    writeback_saga: Option<Arc<WriteBackSaga>>,
}

impl Default for BrokerActor {
    fn default() -> Self {
        Self::new()
    }
}

impl BrokerActor {
    pub fn new() -> Self {
        Self {
            inbox: HashMap::new(),
            orchestrator: DecisionOrchestrator::new(),
            repository: None,
            client_coordinator: None,
            nostr_actor: None,
            subscribers: HashMap::new(),
            writeback_saga: None,
        }
    }

    pub fn with_repository(mut self, repo: Arc<dyn BrokerRepository>) -> Self {
        self.repository = Some(repo);
        self
    }

    pub fn with_client_coordinator(mut self, addr: Addr<ClientCoordinatorActor>) -> Self {
        self.client_coordinator = Some(addr);
        self
    }

    /// Wire a `ServerNostrActor` address so the broker can emit kind 30300
    /// events for `KnowledgeEnrichment` decisions (PRD-013 G7).
    pub fn with_nostr_actor(mut self, addr: Addr<ServerNostrActor>) -> Self {
        self.nostr_actor = Some(addr);
        self
    }

    pub fn with_writeback_saga(mut self, saga: Arc<WriteBackSaga>) -> Self {
        self.writeback_saga = Some(saga);
        self
    }

    /// Broadcast a broker channel event to subscribed clients.
    fn broadcast(&self, channel: &BrokerChannel, event_type: &str, payload: serde_json::Value) {
        let Some(ref coordinator) = self.client_coordinator else {
            debug!("broker broadcast dropped: no ClientCoordinatorActor wired");
            return;
        };
        let channel_name = match channel {
            BrokerChannel::Inbox => "inbox".to_string(),
            BrokerChannel::Case(id) => format!("case:{id}"),
        };
        let envelope = json!({
            "type": format!("broker:{event_type}"),
            "channel": channel_name,
            "payload": payload,
        });
        // NOTE: we fan out to everybody by default; per-client filtering is
        // a future optimisation — the subscribers map is kept so we can
        // switch to targeted delivery without changing the message contract.
        if let Ok(message) = serde_json::to_string(&envelope) {
            coordinator.do_send(BroadcastMessage { message });
        }
    }
}

impl Actor for BrokerActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("[BrokerActor] started (inbox cache capacity=unbounded)");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("[BrokerActor] stopped (cached {} cases)", self.inbox.len());
    }
}

// ---------------------------------------------------------------------------
// Handlers — commands
// ---------------------------------------------------------------------------

impl Handler<SubmitBrokerCase> for BrokerActor {
    type Result = ResponseFuture<Result<BrokerCase, String>>;

    fn handle(&mut self, msg: SubmitBrokerCase, _ctx: &mut Self::Context) -> Self::Result {
        // M6: prune stale cases when inbox exceeds capacity.
        if self.inbox.len() >= INBOX_MAX_CAPACITY {
            let cutoff = chrono::Utc::now() - chrono::Duration::seconds(INBOX_PRUNE_AGE_SECS);
            self.inbox.retain(|_, c| c.created_at > cutoff);
        }
        let case = msg.case.clone();
        self.inbox.insert(case.id.clone(), case.clone());
        self.broadcast(
            &BrokerChannel::Inbox,
            "new_case",
            json!({ "caseId": case.id, "title": case.title, "category": case.category }),
        );
        let repo = self.repository.clone();
        Box::pin(async move {
            if let Some(repo) = repo {
                // Repository uses the legacy `models::enterprise::BrokerCase`;
                // for now we only mirror a best-effort persistence hook. The
                // legacy projection is created via the existing REST handler
                // path; the actor's cache is authoritative for the session.
                let _ = repo; // placeholder — wiring to legacy model deferred.
            }
            Ok(case)
        })
    }
}

impl Handler<ClaimBrokerCase> for BrokerActor {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: ClaimBrokerCase, _ctx: &mut Self::Context) -> Self::Result {
        let case = self
            .inbox
            .get_mut(&msg.case_id)
            .ok_or_else(|| format!("case {} not cached", msg.case_id))?;
        case.claim(&msg.broker_pubkey).map_err(stringify_error)?;
        self.broadcast(
            &BrokerChannel::Case(msg.case_id.clone()),
            "case_claimed",
            json!({ "caseId": msg.case_id, "broker": msg.broker_pubkey }),
        );
        Ok(())
    }
}

impl Handler<DecideBrokerCase> for BrokerActor {
    type Result = Result<String, String>;

    fn handle(&mut self, msg: DecideBrokerCase, _ctx: &mut Self::Context) -> Self::Result {
        let case = self
            .inbox
            .get_mut(&msg.case_id)
            .ok_or_else(|| format!("case {} not cached", msg.case_id))?;

        let report = self
            .orchestrator
            .decide(
                case,
                msg.decision_id.clone(),
                msg.outcome.clone(),
                msg.broker_pubkey.clone(),
                msg.reasoning.clone(),
            )
            .map_err(|e| e.to_string())?;

        let payload = json!({
            "caseId": msg.case_id,
            "decisionId": report.entry.decision_id,
            "action": msg.outcome.action_str(),
            "sharePlan": report.share_plan,
        });
        self.broadcast(
            &BrokerChannel::Case(msg.case_id.clone()),
            "case_decided",
            payload.clone(),
        );
        self.broadcast(&BrokerChannel::Inbox, "case_updated", payload);

        // PRD-013 G4: trigger WriteBackSaga for approved KnowledgeEnrichment cases.
        let case_snapshot = self.inbox.get(&msg.case_id).cloned();
        if let Some(ref case) = case_snapshot {
            let should_writeback = case.category == CaseCategory::KnowledgeEnrichment
                && matches!(
                    msg.outcome,
                    DecisionOutcome::Approve | DecisionOutcome::Promote { .. }
                );

            if should_writeback {
                if let Some(saga) = self.writeback_saga.clone() {
                    match build_writeback_params(case, &msg.broker_pubkey, &msg.reasoning) {
                        Ok((remote_id, enrichment, decision_report)) => {
                            let case_id = case.id.clone();
                            actix::spawn(async move {
                                info!(
                                    "[BrokerActor] write-back: spawning saga for case {}",
                                    case_id
                                );
                                match saga
                                    .execute(&remote_id, &enrichment, &decision_report)
                                    .await
                                {
                                    Ok(result) => {
                                        info!(
                                            "[BrokerActor] write-back: saga complete for case {} \
                                             → commit {} on remote {}",
                                            case_id, result.commit_sha, result.remote_id
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            "[BrokerActor] write-back: saga failed for case {}: {}",
                                            case_id, e
                                        );
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            warn!(
                                "[BrokerActor] write-back: skipping case {} — \
                                 missing metadata: {}",
                                case.id, e
                            );
                        }
                    }
                } else {
                    debug!(
                        "[BrokerActor] write-back: no saga wired; skipping case {}",
                        case.id
                    );
                }
            }

            // PRD-013 G7: emit a kind 30300 Nostr event for the broker decision
            // so relay watchers can correlate enrichment proposals (30301) with
            // their outcomes.
            if case.category == CaseCategory::KnowledgeEnrichment {
                if let Some(ref nostr) = self.nostr_actor {
                    let entity_urn = case.metadata.get("entity_urn").cloned().unwrap_or_default();
                    nostr.do_send(SignBrokerDecision {
                        case_id: case.id.clone(),
                        decision_id: report.entry.decision_id.clone(),
                        outcome_action: msg.outcome.action_str().to_string(),
                        broker_pubkey: msg.broker_pubkey.clone(),
                        entity_urn,
                        reasoning: msg.reasoning.clone(),
                    });
                }
            }
        }

        Ok(report.entry.decision_id)
    }
}

// ---------------------------------------------------------------------------
// Handlers — queries
// ---------------------------------------------------------------------------

impl Handler<ListBrokerInbox> for BrokerActor {
    type Result = MessageResult<ListBrokerInbox>;

    fn handle(&mut self, msg: ListBrokerInbox, _ctx: &mut Self::Context) -> Self::Result {
        let mut all: Vec<BrokerCase> = self.inbox.values().cloned().collect();
        all.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(b.created_at.cmp(&a.created_at))
        });
        all.truncate(msg.limit);
        MessageResult(all)
    }
}

impl Handler<GetBrokerCase> for BrokerActor {
    type Result = MessageResult<GetBrokerCase>;

    fn handle(&mut self, msg: GetBrokerCase, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(self.inbox.get(&msg.case_id).cloned())
    }
}

// ---------------------------------------------------------------------------
// Handlers — subscriptions
// ---------------------------------------------------------------------------

impl Handler<SubscribeBrokerChannel> for BrokerActor {
    type Result = ();

    fn handle(&mut self, msg: SubscribeBrokerChannel, _ctx: &mut Self::Context) {
        self.subscribers
            .entry(msg.channel.clone())
            .or_insert_with(HashSet::new)
            .insert(msg.client_id.clone());
        debug!(
            "BrokerActor: client {} subscribed to {:?}",
            msg.client_id, msg.channel
        );
    }
}

impl Handler<UnsubscribeBrokerChannel> for BrokerActor {
    type Result = ();

    fn handle(&mut self, msg: UnsubscribeBrokerChannel, _ctx: &mut Self::Context) {
        if let Some(set) = self.subscribers.get_mut(&msg.channel) {
            set.remove(&msg.client_id);
            if set.is_empty() {
                self.subscribers.remove(&msg.channel);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stringify_error(err: CaseInvariantError) -> String {
    warn!("broker invariant violation: {}", err);
    err.to_string()
}

/// Extract enrichment metadata from a `KnowledgeEnrichment` case and build the
/// types the `WriteBackSaga` expects. Returns an error string if any required
/// metadata key is missing.
fn build_writeback_params(
    case: &BrokerCase,
    broker_pubkey: &str,
    reasoning: &str,
) -> Result<(String, EnrichmentPayload, DecisionReport), String> {
    let meta = &case.metadata;

    let get = |key: &str| -> Result<String, String> {
        meta.get(key)
            .filter(|v| !v.is_empty())
            .cloned()
            .ok_or_else(|| format!("metadata key '{}' missing or empty", key))
    };

    let remote_id = get("remote_id")?;

    let enrichment_type = match get("enrichment_type")?.as_str() {
        "ontology_promotion" => EnrichmentType::OntologyPromotion,
        "embedding_update" => EnrichmentType::EmbeddingUpdate,
        "gap_detection" => EnrichmentType::GapDetection,
        "agent_annotation" => EnrichmentType::AgentAnnotation,
        other => return Err(format!("unknown enrichment_type '{}'", other)),
    };

    let enrichment = EnrichmentPayload {
        enrichment_type,
        target_path: get("target_path")?,
        content: get("content")?,
        commit_subject: get("commit_subject")?,
        commit_body: meta.get("commit_body").cloned().unwrap_or_default(),
    };

    let server_did = std::env::var("SERVER_NOSTR_PUBKEY")
        .map(|hex| format!("did:nostr:{}", hex))
        .unwrap_or_else(|_| "did:nostr:unknown".to_string());

    let decision_report = DecisionReport {
        case_id: case.id.clone(),
        decision: "approve".to_string(),
        proposed_by: meta.get("proposed_by").cloned().unwrap_or_default(),
        approved_by: format!("did:nostr:{}", broker_pubkey),
        reasoning: reasoning.to_string(),
        server_did,
        entity_urn: meta.get("entity_urn").cloned().unwrap_or_default(),
    };

    Ok((remote_id, enrichment, decision_report))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::broker::{
        BrokerCase, CaseCategory, DecisionOutcome, ShareState, SubjectKind, SubjectRef,
    };

    fn seed_case(author: &str) -> BrokerCase {
        BrokerCase::new(
            "case-a",
            CaseCategory::ContributorMeshShare,
            SubjectRef {
                kind: SubjectKind::WorkArtifact,
                id: "art-1".into(),
                from_state: Some(ShareState::Private),
                to_state: Some(ShareState::Team),
            },
            "Promote",
            "Summary",
            author,
            60,
        )
    }

    #[actix::test]
    async fn submit_and_list_and_decide() {
        let addr = BrokerActor::new().start();
        let case = seed_case("alice");
        let _ = addr
            .send(SubmitBrokerCase { case: case.clone() })
            .await
            .unwrap()
            .unwrap();

        let inbox = addr.send(ListBrokerInbox { limit: 50 }).await.unwrap();
        assert_eq!(inbox.len(), 1);

        addr.send(ClaimBrokerCase {
            case_id: "case-a".into(),
            broker_pubkey: "bob".into(),
        })
        .await
        .unwrap()
        .unwrap();

        let dec_id = addr
            .send(DecideBrokerCase {
                case_id: "case-a".into(),
                decision_id: "dec-1".into(),
                outcome: DecisionOutcome::Approve,
                broker_pubkey: "bob".into(),
                reasoning: "looks good".into(),
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dec_id, "dec-1");
    }

    #[actix::test]
    async fn self_review_rejected_through_actor() {
        let addr = BrokerActor::new().start();
        let case = seed_case("alice");
        addr.send(SubmitBrokerCase { case }).await.unwrap().unwrap();
        // Force underlying state by claiming with alice — should fail.
        let err = addr
            .send(ClaimBrokerCase {
                case_id: "case-a".into(),
                broker_pubkey: "alice".into(),
            })
            .await
            .unwrap()
            .unwrap_err();
        assert!(err.contains("self-review"));
    }
}
