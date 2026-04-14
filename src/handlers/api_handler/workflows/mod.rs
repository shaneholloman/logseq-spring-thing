use actix_web::{web, Responder};
use log::info;
use serde::Deserialize;
use serde_json::json;

use crate::{ok_json, bad_request, not_found, created_json};
use crate::AppState;

/// GET /api/workflows/proposals
pub async fn list_proposals(
    _state: web::Data<AppState>,
    query: web::Query<ProposalQuery>,
) -> impl Responder {
    info!("GET /api/workflows/proposals");
    ok_json!(json!({
        "proposals": [],
        "total": 0,
        "status_filter": query.status
    }))
}

#[derive(Debug, Deserialize)]
pub struct ProposalQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
}

/// POST /api/workflows/proposals
pub async fn create_proposal(
    _state: web::Data<AppState>,
    body: web::Json<CreateProposalRequest>,
) -> impl Responder {
    info!("POST /api/workflows/proposals");

    if body.title.trim().is_empty() {
        return bad_request!("title is required");
    }

    let proposal_id = format!("wp-{}", uuid::Uuid::new_v4());
    created_json!(json!({
        "id": proposal_id,
        "title": body.title,
        "status": "draft",
        "version": 1
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProposalRequest {
    pub title: String,
    pub description: Option<String>,
    pub steps: Option<Vec<serde_json::Value>>,
}

/// GET /api/workflows/proposals/{id}
pub async fn get_proposal(
    _state: web::Data<AppState>,
    proposal_id: web::Path<String>,
) -> impl Responder {
    info!("GET /api/workflows/proposals/{}", proposal_id);
    not_found!(format!("Proposal {} not found", proposal_id))
}

/// GET /api/workflows/patterns
pub async fn list_patterns(_state: web::Data<AppState>) -> impl Responder {
    info!("GET /api/workflows/patterns");
    ok_json!(json!({
        "patterns": [],
        "total": 0
    }))
}

/// Route configuration for workflow proposals and patterns.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/workflows")
            .route("/proposals", web::get().to(list_proposals))
            .route("/proposals", web::post().to(create_proposal))
            .route("/proposals/{id}", web::get().to(get_proposal))
            .route("/patterns", web::get().to(list_patterns)),
    );
}
