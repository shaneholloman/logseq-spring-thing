use actix_web::{web, HttpRequest, Responder};
use log::{info, error};
use serde::Deserialize;
use serde_json::json;

use crate::events::enterprise_events::{
    CaseCreatedEvent, CaseDecidedEvent, emit_enterprise_event,
};
use crate::middleware::enterprise_auth::require_role;
use crate::models::enterprise::*;
use crate::services::migration_broker::{
    is_migration_candidate_case, meta_keys, MigrationCandidateAggregate,
    CATEGORY_CONTRIBUTOR_MESH_SHARE, SUBJECT_KIND_ONTOLOGY_TERM,
};
use crate::{ok_json, bad_request, not_found, created_json, error_json};
use crate::AppState;

/// GET /api/broker/inbox
/// Returns broker cases, optionally filtered by status.
/// Requires Broker role or higher.
pub async fn get_inbox(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<InboxQuery>,
) -> impl Responder {
    if let Err(resp) = require_role(&req, EnterpriseRole::Broker) {
        return resp;
    }
    info!("GET /api/broker/inbox");

    let status_filter: Option<CaseStatus> = query.status.as_deref().and_then(|s| {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    });

    let limit = query.limit.unwrap_or(50).min(200);

    match state.broker_repository.list_cases(status_filter, limit).await {
        Ok(cases) => {
            let total = cases.len();
            ok_json!(json!({
                "cases": cases,
                "total": total,
                "status_filter": query.status,
            }))
        }
        Err(e) => {
            error!("Failed to list broker cases: {}", e);
            error_json!("Failed to list broker cases", e.to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct InboxQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
}

/// GET /api/broker/cases/{id}
pub async fn get_case(
    state: web::Data<AppState>,
    case_id: web::Path<String>,
) -> impl Responder {
    info!("GET /api/broker/cases/{}", case_id);

    match state.broker_repository.get_case(case_id.as_str()).await {
        Ok(Some(case)) => ok_json!(json!(case)),
        Ok(None) => not_found!(format!("Case {} not found", case_id)),
        Err(e) => {
            error!("Failed to get case {}: {}", case_id, e);
            error_json!("Failed to get case", e.to_string())
        }
    }
}

/// POST /api/broker/cases
/// Submit a new case (manual escalation or workflow proposal).
pub async fn create_case(
    state: web::Data<AppState>,
    body: web::Json<CreateCaseRequest>,
) -> impl Responder {
    info!("POST /api/broker/cases");

    if body.title.trim().is_empty() {
        return bad_request!("title is required");
    }

    if body.title.len() > 200 {
        return bad_request!("Title exceeds 200 characters");
    }

    let priority: CasePriority = match body.priority.as_deref() {
        Some("critical") => CasePriority::Critical,
        Some("high") => CasePriority::High,
        Some("low") => CasePriority::Low,
        Some("medium") | None => CasePriority::Medium,
        Some(other) => {
            return bad_request!(format!(
                "priority must be one of: critical, high, medium, low (got '{}')",
                other
            ));
        }
    };

    let source: EscalationSource = match body.source.as_deref() {
        Some("policy_violation") => EscalationSource::PolicyViolation,
        Some("confidence_threshold") => EscalationSource::ConfidenceThreshold,
        Some("trust_drift") => EscalationSource::TrustDrift,
        Some("workflow_proposal") => EscalationSource::WorkflowProposal,
        Some("manual_submission") | None => EscalationSource::ManualSubmission,
        Some(other) => {
            return bad_request!(format!(
                "source must be one of: policy_violation, confidence_threshold, trust_drift, manual_submission, workflow_proposal (got '{}')",
                other
            ));
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    let case_id = format!("case-{}", uuid::Uuid::new_v4());

    let case = BrokerCase {
        id: case_id.clone(),
        title: body.title.clone(),
        description: body.description.clone().unwrap_or_default(),
        priority,
        source,
        status: CaseStatus::Open,
        created_at: now.clone(),
        updated_at: now,
        assigned_to: None,
        evidence: vec![],
        metadata: std::collections::HashMap::new(),
    };

    match state.broker_repository.create_case(&case).await {
        Ok(()) => {
            // Emit audit event
            emit_enterprise_event(&CaseCreatedEvent {
                case_id: case.id.clone(),
                title: case.title.clone(),
                priority: case.priority.clone(),
                source: case.source.clone(),
                created_by: "api".to_string(),
                timestamp: chrono::Utc::now(),
            });

            created_json!(json!({
                "id": case.id,
                "title": case.title,
                "status": "open",
                "message": "Case created. A broker will review it."
            }))
        }
        Err(e) => {
            error!("Failed to create case: {}", e);
            error_json!("Failed to create case", e.to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCaseRequest {
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub source: Option<String>,
}

/// POST /api/broker/cases/{id}/decide
/// Requires Broker role or higher.
pub async fn decide_case(
    req: HttpRequest,
    state: web::Data<AppState>,
    case_id: web::Path<String>,
    body: web::Json<DecisionRequest>,
) -> impl Responder {
    let actor_role = match require_role(&req, EnterpriseRole::Broker) {
        Ok(role) => role,
        Err(resp) => return resp,
    };
    let cid = case_id.into_inner();
    info!("POST /api/broker/cases/{}/decide", cid);

    if body.action.is_empty() {
        return bad_request!("action is required");
    }

    // Validate that the case exists
    match state.broker_repository.get_case(&cid).await {
        Ok(None) => return not_found!(format!("Case {} not found", cid)),
        Err(e) => {
            error!("Failed to look up case {}: {}", cid, e);
            return error_json!("Failed to look up case", e.to_string());
        }
        Ok(Some(_)) => {}
    }

    let action: DecisionAction = match body.action.as_str() {
        "approve" => DecisionAction::Approve,
        "reject" => DecisionAction::Reject,
        "amend" => DecisionAction::Amend,
        "delegate" => DecisionAction::Delegate,
        "promote_as_workflow" => DecisionAction::PromoteAsWorkflow,
        "mark_as_precedent" => DecisionAction::MarkAsPrecedent,
        "request_more_evidence" => DecisionAction::RequestMoreEvidence,
        other => {
            return bad_request!(format!(
                "action must be one of: approve, reject, amend, delegate, promote_as_workflow, mark_as_precedent, request_more_evidence (got '{}')",
                other
            ));
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    let decision = BrokerDecision {
        id: format!("dec-{}", uuid::Uuid::new_v4()),
        case_id: cid.clone(),
        action,
        reasoning: body.reasoning.clone().unwrap_or_default(),
        decided_by: "broker".to_string(),
        decided_at: now,
        provenance_event_id: None,
    };

    match state.broker_repository.record_decision(&decision).await {
        Ok(()) => {
            // Emit audit event
            emit_enterprise_event(&CaseDecidedEvent {
                case_id: cid.clone(),
                decision_id: decision.id.clone(),
                action: decision.action.clone(),
                decided_by: format!("{:?}", actor_role),
                timestamp: chrono::Utc::now(),
            });

            ok_json!(json!({
                "case_id": cid,
                "decision_id": decision.id,
                "action": body.action,
                "status": "decided",
                "message": "Decision recorded with provenance."
            }))
        }
        Err(e) => {
            error!("Failed to record decision for case {}: {}", cid, e);
            error_json!("Failed to record decision", e.to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRequest {
    pub action: String,
    pub reasoning: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// ADR-049: Migration candidate endpoints (subject_kind = "ontology_term")
// ─────────────────────────────────────────────────────────────────────────────

/// POST /api/broker/migration-candidates
///
/// Surface a new ontology migration candidate as a `BrokerCase` in the
/// Contributor Mesh Share lane. The payload comes from the discovery engine
/// (ADR-048) once confidence ≥ 0.60. The resulting case carries
/// `metadata["subject_kind"] = "ontology_term"` and
/// `metadata["broker_category"] = "contributor_mesh_share"` per ADR-049.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMigrationCandidateRequest {
    pub id: String,
    pub kg_note_id: String,
    pub kg_note_label: String,
    pub ontology_iri: String,
    pub confidence: f64,
    #[serde(default)]
    pub signal_sources: Vec<String>,
    #[serde(default)]
    pub agent_source: Option<String>,
    pub owl_delta_json: String,
}

pub async fn create_migration_candidate(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<CreateMigrationCandidateRequest>,
) -> impl Responder {
    if let Err(resp) = require_role(&req, EnterpriseRole::Broker) {
        return resp;
    }
    info!(
        "POST /api/broker/migration-candidates id={} iri={} conf={:.3}",
        body.id, body.ontology_iri, body.confidence
    );

    let agg = match MigrationCandidateAggregate::new(
        body.id.clone(),
        body.kg_note_id.clone(),
        body.kg_note_label.clone(),
        body.ontology_iri.clone(),
        body.confidence,
        body.signal_sources.clone(),
        body.agent_source.clone(),
        body.owl_delta_json.clone(),
    ) {
        Ok(a) => a,
        Err(e) => return bad_request!(format!("invalid migration candidate: {}", e)),
    };

    let mut case = agg.to_broker_case();
    // Stamp the canonical category discriminator in metadata as well — the
    // base `BrokerCase` aggregate (E3) does not yet carry a typed `category`
    // field, so we surface it via metadata until that field lands.
    case.metadata.insert(
        "broker_category".to_string(),
        CATEGORY_CONTRIBUTOR_MESH_SHARE.to_string(),
    );

    match state.broker_repository.create_case(&case).await {
        Ok(()) => {
            emit_enterprise_event(&CaseCreatedEvent {
                case_id: case.id.clone(),
                title: case.title.clone(),
                priority: case.priority.clone(),
                source: case.source.clone(),
                created_by: "migration-broker".to_string(),
                timestamp: chrono::Utc::now(),
            });
            created_json!(json!({
                "id": case.id,
                "title": case.title,
                "category": CATEGORY_CONTRIBUTOR_MESH_SHARE,
                "subject_kind": SUBJECT_KIND_ONTOLOGY_TERM,
                "status": case.status,
                "confidence": body.confidence,
                "message": "Migration candidate surfaced to broker inbox.",
            }))
        }
        Err(e) => {
            error!("Failed to create migration candidate case: {}", e);
            error_json!("Failed to create migration candidate", e.to_string())
        }
    }
}

/// POST /api/broker/cases/{id}/approve-migration
///
/// Approve a migration candidate case. This is a specialised `decide`
/// endpoint that:
///   1. asserts `subject_kind = "ontology_term"` on the case metadata,
///   2. records a `BrokerDecision { action: Approve }`,
///   3. returns the decision metadata. The downstream `ontology_propose` MCP
///      tool call is orchestrated by the DecisionOrchestrator actor, which
///      feeds the resulting PR URL back into the case via
///      `MigrationCandidateAggregate::on_pr_assigned`.
pub async fn approve_migration_candidate(
    req: HttpRequest,
    state: web::Data<AppState>,
    case_id: web::Path<String>,
) -> impl Responder {
    let actor_role = match require_role(&req, EnterpriseRole::Broker) {
        Ok(r) => r,
        Err(resp) => return resp,
    };
    let cid = case_id.into_inner();
    info!("POST /api/broker/cases/{}/approve-migration", cid);

    let case = match state.broker_repository.get_case(&cid).await {
        Ok(Some(c)) => c,
        Ok(None) => return not_found!(format!("Case {} not found", cid)),
        Err(e) => {
            error!("Failed to look up case {}: {}", cid, e);
            return error_json!("Failed to look up case", e.to_string());
        }
    };

    if !is_migration_candidate_case(&case) {
        return bad_request!(format!(
            "Case {} is not a migration candidate (subject_kind != {})",
            cid, SUBJECT_KIND_ONTOLOGY_TERM
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let decision = BrokerDecision {
        id: format!("dec-{}", uuid::Uuid::new_v4()),
        case_id: cid.clone(),
        action: DecisionAction::Approve,
        reasoning: "Approved; ontology_propose will be dispatched.".to_string(),
        decided_by: format!("{:?}", actor_role),
        decided_at: now,
        provenance_event_id: None,
    };

    match state.broker_repository.record_decision(&decision).await {
        Ok(()) => {
            emit_enterprise_event(&CaseDecidedEvent {
                case_id: cid.clone(),
                decision_id: decision.id.clone(),
                action: decision.action.clone(),
                decided_by: format!("{:?}", actor_role),
                timestamp: chrono::Utc::now(),
            });
            ok_json!(json!({
                "case_id": cid,
                "decision_id": decision.id,
                "status": "pr_pending",
                "subject_kind": SUBJECT_KIND_ONTOLOGY_TERM,
                "ontology_iri": case.metadata.get(meta_keys::ONTOLOGY_IRI),
                "message": "Approval recorded. ontology_propose will open the GitHub PR and the URL will be attached to this case asynchronously.",
            }))
        }
        Err(e) => {
            error!("Failed to record approval for case {}: {}", cid, e);
            error_json!("Failed to record approval", e.to_string())
        }
    }
}

/// GET /api/broker/cases/{id}/history
/// Returns the append-only decision history for a case.
pub async fn get_case_history(
    state: web::Data<AppState>,
    case_id: web::Path<String>,
) -> impl Responder {
    info!("GET /api/broker/cases/{}/history", case_id);
    match state.broker_repository.get_decisions(case_id.as_str()).await {
        Ok(decisions) => ok_json!(json!({
            "caseId": case_id.into_inner(),
            "decisions": decisions,
            "count": decisions.len(),
        })),
        Err(e) => {
            error!("Failed to list decisions for case {}: {}", case_id, e);
            error_json!("Failed to fetch decision history", e.to_string())
        }
    }
}

/// GET /api/broker/subscribe (WebSocket upgrade stub)
///
/// The real subscription pipe is served by the platform-wide fast-websocket
/// handler; this endpoint returns a descriptor that the client can use to
/// route broker events through the same socket. Kept as a REST call so the
/// client has a stable discovery handle.
pub async fn subscribe_descriptor() -> impl Responder {
    ok_json!(json!({
        "transport": "ws",
        "channels": ["inbox", "case:{id}"],
        "events": [
            "broker:new_case",
            "broker:case_updated",
            "broker:case_claimed",
            "broker:case_decided",
            "broker:priority_changed"
        ]
    }))
}

/// Route configuration for the broker workbench.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/broker")
            .wrap(crate::middleware::RequireAuth::authenticated())
            .route("/inbox", web::get().to(get_inbox))
            .route("/cases", web::post().to(create_case))
            .route("/cases/{id}", web::get().to(get_case))
            .route("/cases/{id}/decide", web::post().to(decide_case))
            .route("/cases/{id}/history", web::get().to(get_case_history))
            .route("/subscribe", web::get().to(subscribe_descriptor))
            // ADR-049 migration-candidate routes.
            .route(
                "/migration-candidates",
                web::post().to(create_migration_candidate),
            )
            .route(
                "/cases/{id}/approve-migration",
                web::post().to(approve_migration_candidate),
            ),
    );
}
