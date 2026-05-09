//! Enrichment proposal REST handler (PRD-013 G7).
//!
//! Exposes `POST /api/enrichment-proposals` as the inbound surface for
//! agentbox's git-bridge. When an agent submits a knowledge-graph enrichment
//! (property addition, ontology promotion, embedding update, etc.), this
//! handler:
//!
//! 1. Creates a `BrokerCase` with `CaseCategory::KnowledgeEnrichment`.
//! 2. Submits it to the `BrokerActor` for broker gating.
//! 3. (Optionally) signs a kind 30301 Nostr event via `ServerNostrActor`.
//! 4. Returns the case id so the caller can poll for the decision.

use actix::prelude::*;
use actix_web::{web, HttpResponse};
use log::{error, info};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::actors::broker_actor::BrokerActor;
use crate::actors::messages::broker_messages::SubmitBrokerCase;
use crate::actors::server_nostr_actor::{ServerNostrActor, SignEnrichmentProposal};
use crate::domain::broker::{BrokerCase, CaseCategory, SubjectKind, SubjectRef};
use crate::settings::auth_extractor::AuthenticatedUser;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// JSON body accepted by `POST /api/enrichment-proposals`.
#[derive(Debug, Clone, Deserialize)]
pub struct EnrichmentProposalRequest {
    /// DID of the agent submitting the proposal (e.g. `did:nostr:<hex>`).
    pub agent_did: String,
    /// URN of the knowledge-graph entity being enriched.
    pub entity_urn: String,
    /// Free-form enrichment type label (e.g. `property_addition`,
    /// `ontology_promotion`, `embedding_update`, `gap_detection`,
    /// `agent_annotation`).
    pub enrichment_type: String,
    /// Relative file path within the source repository that the enrichment
    /// targets (e.g. `pages/Quantum_Computing.md`).
    pub target_path: String,
    /// Blake3 (or similar) hash of the agent's reasoning trace, for
    /// auditability without storing the full trace on-chain.
    pub reasoning_hash: String,
    /// Human-readable title for the broker case.
    #[serde(default)]
    pub title: Option<String>,
    /// Human-readable summary / justification.
    #[serde(default)]
    pub summary: Option<String>,
    /// Priority hint (0-100). Defaults to 50.
    #[serde(default = "default_priority")]
    pub priority: u8,
    /// The proposed content to write back (Markdown, YAML front-matter, etc.).
    /// Stored in case metadata so the write-back saga can access it after
    /// broker approval.
    #[serde(default)]
    pub content: Option<String>,
    /// Git commit subject line for write-back.
    #[serde(default)]
    pub commit_subject: Option<String>,
    /// Git commit body for write-back.
    #[serde(default)]
    pub commit_body: Option<String>,
    /// Remote id (from git-ingest registry) that the write-back targets.
    #[serde(default)]
    pub remote_id: Option<String>,
}

fn default_priority() -> u8 {
    50
}

/// JSON response returned on success.
#[derive(Debug, Serialize)]
pub struct EnrichmentProposalResponse {
    pub case_id: String,
    pub status: &'static str,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `POST /api/enrichment-proposals`
///
/// Creates a `KnowledgeEnrichment` broker case and optionally signs a kind
/// 30301 Nostr event. Returns 201 with the case id on success.
pub async fn submit_enrichment_proposal(
    _user: AuthenticatedUser,
    broker: web::Data<Addr<BrokerActor>>,
    nostr: Option<web::Data<Addr<ServerNostrActor>>>,
    body: web::Json<EnrichmentProposalRequest>,
) -> HttpResponse {
    let req = body.into_inner();
    let case_id = format!("enrich-{}", Uuid::new_v4());

    info!(
        "[enrichment_proposal_handler] POST /api/enrichment-proposals \
         agent={} urn={} type={}",
        req.agent_did, req.entity_urn, req.enrichment_type
    );

    let title = req
        .title
        .clone()
        .unwrap_or_else(|| format!("{} on {}", req.enrichment_type, req.entity_urn));
    let summary = req
        .summary
        .clone()
        .unwrap_or_else(|| format!("Agent {} proposes {}", req.agent_did, req.enrichment_type));

    let mut case = BrokerCase::new(
        &case_id,
        CaseCategory::KnowledgeEnrichment,
        SubjectRef {
            kind: SubjectKind::Opaque,
            id: req.entity_urn.clone(),
            from_state: None,
            to_state: None,
        },
        &title,
        &summary,
        &req.agent_did,
        req.priority,
    );

    // Stash enrichment metadata so the write-back saga can extract it later.
    case.metadata
        .insert("entity_urn".to_string(), req.entity_urn.clone());
    case.metadata
        .insert("enrichment_type".to_string(), req.enrichment_type.clone());
    case.metadata
        .insert("target_path".to_string(), req.target_path.clone());
    case.metadata
        .insert("reasoning_hash".to_string(), req.reasoning_hash.clone());
    case.metadata
        .insert("proposed_by".to_string(), req.agent_did.clone());
    if let Some(ref content) = req.content {
        case.metadata.insert("content".to_string(), content.clone());
    }
    if let Some(ref subject) = req.commit_subject {
        case.metadata
            .insert("commit_subject".to_string(), subject.clone());
    }
    if let Some(ref body_text) = req.commit_body {
        case.metadata
            .insert("commit_body".to_string(), body_text.clone());
    }
    if let Some(ref remote_id) = req.remote_id {
        case.metadata
            .insert("remote_id".to_string(), remote_id.clone());
    }

    // Submit to broker actor.
    match broker.send(SubmitBrokerCase { case }).await {
        Ok(Ok(_submitted)) => {
            info!(
                "[enrichment_proposal_handler] Case {} submitted to broker",
                case_id
            );
        }
        Ok(Err(e)) => {
            error!(
                "[enrichment_proposal_handler] Broker rejected case {}: {}",
                case_id, e
            );
            return HttpResponse::UnprocessableEntity().json(serde_json::json!({
                "error": "Broker rejected proposal",
                "message": e,
            }));
        }
        Err(e) => {
            error!(
                "[enrichment_proposal_handler] Broker mailbox error for case {}: {}",
                case_id, e
            );
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Broker unavailable",
                "message": e.to_string(),
            }));
        }
    }

    // Fire-and-forget: sign a kind 30301 Nostr event for relay watchers.
    if let Some(ref nostr_addr) = nostr {
        nostr_addr.do_send(SignEnrichmentProposal {
            case_id: case_id.clone(),
            agent_did: req.agent_did,
            entity_urn: req.entity_urn,
            enrichment_type: req.enrichment_type,
            target_path: req.target_path,
            reasoning_hash: req.reasoning_hash,
        });
    }

    HttpResponse::Created().json(EnrichmentProposalResponse {
        case_id,
        status: "submitted",
    })
}

// ---------------------------------------------------------------------------
// Decide handler (BrokerActor path)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DecideEnrichmentRequest {
    pub outcome: String,
    pub broker_pubkey: String,
    #[serde(default)]
    pub reasoning: String,
}

/// `POST /api/enrichment-proposals/{case_id}/decide`
///
/// Routes the decision through the `BrokerActor` (which triggers WriteBackSaga
/// for approved `KnowledgeEnrichment` cases and emits kind 30300 events).
pub async fn decide_enrichment_proposal(
    _user: AuthenticatedUser,
    broker: web::Data<Addr<BrokerActor>>,
    path: web::Path<String>,
    body: web::Json<DecideEnrichmentRequest>,
) -> HttpResponse {
    use crate::actors::messages::broker_messages::DecideBrokerCase;
    use crate::domain::broker::DecisionOutcome;

    let case_id = path.into_inner();
    let req = body.into_inner();

    let outcome = match req.outcome.as_str() {
        "approve" => DecisionOutcome::Approve,
        "reject" => DecisionOutcome::Reject,
        "amend" => DecisionOutcome::Amend {
            diff: req.reasoning.clone(),
        },
        "delegate" => DecisionOutcome::Delegate {
            delegate_to: req.reasoning.clone(),
        },
        "promote" => DecisionOutcome::Promote {
            pattern_id: format!("pattern-{}", Uuid::new_v4()),
        },
        other => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": format!("unknown outcome: '{}'. Valid: approve, reject, amend, delegate, promote", other),
            }));
        }
    };

    let decision_id = format!("dec-{}", Uuid::new_v4());

    match broker
        .send(DecideBrokerCase {
            case_id: case_id.clone(),
            decision_id: decision_id.clone(),
            outcome,
            broker_pubkey: req.broker_pubkey,
            reasoning: req.reasoning,
        })
        .await
    {
        Ok(Ok(id)) => HttpResponse::Ok().json(serde_json::json!({
            "case_id": case_id,
            "decision_id": id,
            "status": "decided",
        })),
        Ok(Err(e)) => {
            error!(
                "[enrichment_proposal_handler] decide failed for case {}: {}",
                case_id, e
            );
            HttpResponse::UnprocessableEntity().json(serde_json::json!({
                "error": e,
            }))
        }
        Err(e) => {
            error!("[enrichment_proposal_handler] broker mailbox error: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "broker unavailable",
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Route configuration
// ---------------------------------------------------------------------------

/// Register enrichment-proposal routes under the `/api` scope.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/enrichment-proposals")
            .route("", web::post().to(submit_enrichment_proposal))
            .route(
                "/{case_id}/decide",
                web::post().to(decide_enrichment_proposal),
            ),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- EnrichmentProposalRequest deserialization ----

    #[test]
    fn test_enrichment_proposal_request_minimal() {
        let json = r#"{
            "agent_did": "did:nostr:abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            "entity_urn": "urn:visionclaw:concept:quantum-computing",
            "enrichment_type": "property_addition",
            "target_path": "pages/Quantum_Computing.md",
            "reasoning_hash": "abc123def456"
        }"#;
        let req: EnrichmentProposalRequest = serde_json::from_str(json).unwrap();
        assert!(req.agent_did.starts_with("did:nostr:"));
        assert_eq!(req.enrichment_type, "property_addition");
        assert_eq!(req.priority, 50); // default_priority()
        assert!(req.title.is_none());
        assert!(req.summary.is_none());
        assert!(req.content.is_none());
        assert!(req.commit_subject.is_none());
        assert!(req.commit_body.is_none());
        assert!(req.remote_id.is_none());
    }

    #[test]
    fn test_enrichment_proposal_request_full() {
        let json = r#"{
            "agent_did": "did:nostr:aaaa",
            "entity_urn": "urn:visionclaw:concept:ai",
            "enrichment_type": "ontology_promotion",
            "target_path": "pages/AI.md",
            "reasoning_hash": "deadbeef",
            "title": "Promote AI concept",
            "summary": "AI should be top-level ontology class",
            "priority": 90,
            "content": "Artificial Intelligence overview content",
            "commit_subject": "feat: promote AI to owl:Class",
            "commit_body": "This enrichment promotes...",
            "remote_id": "remote-github-001"
        }"#;
        let req: EnrichmentProposalRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.priority, 90);
        assert_eq!(req.title.as_deref(), Some("Promote AI concept"));
        assert!(req.content.is_some());
        assert!(req.commit_subject.is_some());
        assert!(req.remote_id.is_some());
    }

    #[test]
    fn test_default_priority() {
        assert_eq!(default_priority(), 50);
    }

    // ---- EnrichmentProposalResponse serialization ----

    #[test]
    fn test_enrichment_response_serialization() {
        let resp = EnrichmentProposalResponse {
            case_id: "enrich-test-001".to_string(),
            status: "submitted",
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("enrich-test-001"));
        assert!(json.contains("submitted"));
    }

    // ---- DecideEnrichmentRequest deserialization ----

    #[test]
    fn test_decide_request_approve() {
        let json = r#"{
            "outcome": "approve",
            "broker_pubkey": "abcdef",
            "reasoning": "Looks correct"
        }"#;
        let req: DecideEnrichmentRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.outcome, "approve");
        assert_eq!(req.broker_pubkey, "abcdef");
        assert_eq!(req.reasoning, "Looks correct");
    }

    #[test]
    fn test_decide_request_with_empty_reasoning() {
        let json = r#"{
            "outcome": "reject",
            "broker_pubkey": "abcdef"
        }"#;
        let req: DecideEnrichmentRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.outcome, "reject");
        assert_eq!(req.reasoning, ""); // default
    }

    // ---- BrokerCase construction logic ----

    #[test]
    fn test_broker_case_construction_from_request() {
        let req = EnrichmentProposalRequest {
            agent_did: "did:nostr:1234".to_string(),
            entity_urn: "urn:visionclaw:concept:test".to_string(),
            enrichment_type: "embedding_update".to_string(),
            target_path: "pages/Test.md".to_string(),
            reasoning_hash: "hash123".to_string(),
            title: None,
            summary: None,
            priority: 75,
            content: Some("test content".to_string()),
            commit_subject: Some("feat: test".to_string()),
            commit_body: None,
            remote_id: None,
        };

        let case_id = "enrich-test-case";
        let title = req
            .title
            .clone()
            .unwrap_or_else(|| format!("{} on {}", req.enrichment_type, req.entity_urn));
        let summary = req
            .summary
            .clone()
            .unwrap_or_else(|| format!("Agent {} proposes {}", req.agent_did, req.enrichment_type));

        let mut case = BrokerCase::new(
            case_id,
            CaseCategory::KnowledgeEnrichment,
            SubjectRef {
                kind: SubjectKind::Opaque,
                id: req.entity_urn.clone(),
                from_state: None,
                to_state: None,
            },
            &title,
            &summary,
            &req.agent_did,
            req.priority,
        );

        case.metadata
            .insert("entity_urn".to_string(), req.entity_urn.clone());
        case.metadata
            .insert("enrichment_type".to_string(), req.enrichment_type.clone());
        case.metadata
            .insert("target_path".to_string(), req.target_path.clone());
        case.metadata
            .insert("reasoning_hash".to_string(), req.reasoning_hash.clone());
        if let Some(ref content) = req.content {
            case.metadata.insert("content".to_string(), content.clone());
        }
        if let Some(ref subject) = req.commit_subject {
            case.metadata
                .insert("commit_subject".to_string(), subject.clone());
        }

        assert_eq!(case.id, "enrich-test-case");
        assert_eq!(case.category, CaseCategory::KnowledgeEnrichment);
        assert_eq!(case.subject.kind, SubjectKind::Opaque);
        assert_eq!(case.priority, 75);
        assert_eq!(case.title, "embedding_update on urn:visionclaw:concept:test");
        assert!(case.summary.contains("did:nostr:1234"));
        assert_eq!(case.metadata.get("content").unwrap(), "test content");
        assert_eq!(case.metadata.get("commit_subject").unwrap(), "feat: test");
        assert!(!case.metadata.contains_key("commit_body"));
        assert!(!case.metadata.contains_key("remote_id"));
    }

    // ---- Decision outcome parsing (matches handler logic) ----

    #[test]
    fn test_decision_outcome_parsing() {
        use crate::domain::broker::DecisionOutcome;

        let cases = vec![
            ("approve", true),
            ("reject", true),
            ("amend", true),
            ("delegate", true),
            ("promote", true),
            ("unknown", false),
            ("", false),
        ];

        for (input, should_match) in cases {
            let result = match input {
                "approve" => Some(DecisionOutcome::Approve),
                "reject" => Some(DecisionOutcome::Reject),
                "amend" => Some(DecisionOutcome::Amend {
                    diff: "test".to_string(),
                }),
                "delegate" => Some(DecisionOutcome::Delegate {
                    delegate_to: "test".to_string(),
                }),
                "promote" => Some(DecisionOutcome::Promote {
                    pattern_id: "test".to_string(),
                }),
                _ => None,
            };
            assert_eq!(
                result.is_some(),
                should_match,
                "Failed for input: '{}'",
                input
            );
        }
    }
}
