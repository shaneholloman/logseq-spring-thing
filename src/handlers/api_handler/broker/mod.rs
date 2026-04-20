use actix_web::{web, HttpRequest, Responder};
use log::{info, error};
use serde::Deserialize;
use serde_json::json;

use crate::events::enterprise_events::{
    CaseCreatedEvent, CaseDecidedEvent, emit_enterprise_event,
};
use crate::middleware::enterprise_auth::require_role;
use crate::models::enterprise::*;
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
            .route("/subscribe", web::get().to(subscribe_descriptor)),
    );
}
