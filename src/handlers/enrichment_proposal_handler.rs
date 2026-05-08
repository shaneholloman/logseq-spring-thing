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
