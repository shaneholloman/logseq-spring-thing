use actix_web::{web, Responder};
use log::info;
use serde::Deserialize;
use serde_json::json;

use crate::{ok_json, bad_request, not_found, created_json};
use crate::AppState;

/// GET /api/broker/inbox
/// Returns open broker cases sorted by priority.
pub async fn get_inbox(
    _state: web::Data<AppState>,
    query: web::Query<InboxQuery>,
) -> impl Responder {
    info!("GET /api/broker/inbox");
    ok_json!(json!({
        "cases": [],
        "total": 0,
        "status_filter": query.status,
        "message": "Broker inbox — no cases yet. Submit workflow proposals or configure policy escalation rules."
    }))
}

#[derive(Debug, Deserialize)]
pub struct InboxQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
}

/// GET /api/broker/cases/{id}
pub async fn get_case(
    _state: web::Data<AppState>,
    case_id: web::Path<String>,
) -> impl Responder {
    info!("GET /api/broker/cases/{}", case_id);
    not_found!(format!("Case {} not found — broker service initializing", case_id))
}

/// POST /api/broker/cases
/// Submit a new case (manual escalation or workflow proposal).
pub async fn create_case(
    _state: web::Data<AppState>,
    body: web::Json<CreateCaseRequest>,
) -> impl Responder {
    info!("POST /api/broker/cases");

    if body.title.trim().is_empty() {
        return bad_request!("title is required");
    }

    let case_id = format!("case-{}", uuid::Uuid::new_v4());
    created_json!(json!({
        "id": case_id,
        "title": body.title,
        "status": "open",
        "message": "Case created. A broker will review it."
    }))
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
pub async fn decide_case(
    _state: web::Data<AppState>,
    case_id: web::Path<String>,
    body: web::Json<DecisionRequest>,
) -> impl Responder {
    info!("POST /api/broker/cases/{}/decide", case_id);

    if body.action.is_empty() {
        return bad_request!("action is required");
    }

    ok_json!(json!({
        "case_id": case_id.into_inner(),
        "action": body.action,
        "status": "decided",
        "message": "Decision recorded with provenance."
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRequest {
    pub action: String,
    pub reasoning: Option<String>,
}

/// Route configuration for the broker workbench.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/broker")
            .route("/inbox", web::get().to(get_inbox))
            .route("/cases", web::post().to(create_case))
            .route("/cases/{id}", web::get().to(get_case))
            .route("/cases/{id}/decide", web::post().to(decide_case)),
    );
}
