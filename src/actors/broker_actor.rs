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
use log::{debug, info, warn};
use serde_json::json;

use crate::actors::messages::broker_messages::{
    BrokerChannel, ClaimBrokerCase, DecideBrokerCase, GetBrokerCase, ListBrokerInbox,
    SubmitBrokerCase, SubscribeBrokerChannel, UnsubscribeBrokerChannel,
};
use crate::actors::messages::BroadcastMessage;
use crate::actors::ClientCoordinatorActor;
use crate::domain::broker::{BrokerCase, CaseInvariantError, DecisionOrchestrator};
use crate::ports::broker_repository::BrokerRepository;

// ---------------------------------------------------------------------------
// Actor
// ---------------------------------------------------------------------------

pub struct BrokerActor {
    inbox: HashMap<String, BrokerCase>,
    orchestrator: DecisionOrchestrator,
    repository: Option<Arc<dyn BrokerRepository>>,
    client_coordinator: Option<Addr<ClientCoordinatorActor>>,
    subscribers: HashMap<BrokerChannel, HashSet<String>>,
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
            subscribers: HashMap::new(),
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
                msg.reasoning,
            )
            .map_err(|e| e.to_string())?;

        let payload = json!({
            "caseId": msg.case_id,
            "decisionId": report.entry.decision_id,
            "action": msg.outcome.action_str(),
            "sharePlan": report.share_plan,
        });
        self.broadcast(&BrokerChannel::Case(msg.case_id.clone()), "case_decided", payload.clone());
        self.broadcast(&BrokerChannel::Inbox, "case_updated", payload);
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
        all.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.created_at.cmp(&a.created_at)));
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
