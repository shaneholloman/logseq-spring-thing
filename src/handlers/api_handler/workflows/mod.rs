use actix_web::{web, Responder};
use log::{info, warn};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

use crate::events::enterprise_events::{ProposalCreatedEvent, emit_enterprise_event};
use crate::models::enterprise::{WorkflowPattern, WorkflowProposal, WorkflowStatus, WorkflowStep};
use crate::{ok_json, bad_request, not_found, created_json, error_json};
use crate::AppState;

/// GET /api/workflows/proposals
pub async fn list_proposals(
    state: web::Data<AppState>,
    query: web::Query<ProposalQuery>,
) -> impl Responder {
    info!("GET /api/workflows/proposals");

    let status_filter = match &query.status {
        Some(s) => {
            match serde_json::from_value::<WorkflowStatus>(serde_json::Value::String(s.clone())) {
                Ok(ws) => Some(ws),
                Err(_) => {
                    warn!("Invalid status filter: {}", s);
                    return bad_request!(format!("Invalid status filter: {}", s));
                }
            }
        }
        None => None,
    };

    let limit = query.limit.unwrap_or(100).min(1000);

    match state.workflow_repository.list_proposals(status_filter, limit).await {
        Ok(proposals) => {
            let total = proposals.len();
            ok_json!(json!({
                "proposals": proposals,
                "total": total,
                "status_filter": query.status
            }))
        }
        Err(e) => {
            warn!("Failed to list proposals: {}", e);
            error_json!("Failed to list proposals", e.to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProposalQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
}

/// POST /api/workflows/proposals
pub async fn create_proposal(
    state: web::Data<AppState>,
    body: web::Json<CreateProposalRequest>,
) -> impl Responder {
    info!("POST /api/workflows/proposals");

    if body.title.trim().is_empty() {
        return bad_request!("title is required");
    }

    if body.title.len() > 200 {
        return bad_request!("Title exceeds 200 characters");
    }

    let proposal_id = format!("wp-{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();

    let steps: Vec<WorkflowStep> = body
        .steps
        .as_ref()
        .map(|raw_steps| {
            raw_steps
                .iter()
                .enumerate()
                .map(|(i, v)| WorkflowStep {
                    order: i as u32,
                    name: v
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unnamed")
                        .to_string(),
                    action_type: v
                        .get("actionType")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    config: v.get("config").cloned().unwrap_or(serde_json::Value::Object(Default::default())),
                })
                .collect()
        })
        .unwrap_or_default();

    let proposal = WorkflowProposal {
        id: proposal_id.clone(),
        title: body.title.clone(),
        description: body.description.clone().unwrap_or_default(),
        status: WorkflowStatus::Draft,
        version: 1,
        steps,
        source_insight_id: None,
        submitted_by: "api".to_string(),
        created_at: now.clone(),
        updated_at: now,
        risk_score: None,
        expected_benefit: None,
        metadata: HashMap::new(),
    };

    match state.workflow_repository.create_proposal(&proposal).await {
        Ok(()) => {
            // Emit audit event
            emit_enterprise_event(&ProposalCreatedEvent {
                proposal_id: proposal.id.clone(),
                title: proposal.title.clone(),
                submitted_by: proposal.submitted_by.clone(),
                timestamp: chrono::Utc::now(),
            });

            created_json!(json!({
                "id": proposal.id,
                "title": proposal.title,
                "status": "draft",
                "version": 1
            }))
        }
        Err(e) => {
            warn!("Failed to create proposal: {}", e);
            error_json!("Failed to create proposal", e.to_string())
        }
    }
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
    state: web::Data<AppState>,
    proposal_id: web::Path<String>,
) -> impl Responder {
    info!("GET /api/workflows/proposals/{}", proposal_id);

    match state.workflow_repository.get_proposal(&proposal_id).await {
        Ok(Some(proposal)) => ok_json!(json!(proposal)),
        Ok(None) => not_found!(format!("Proposal {} not found", proposal_id)),
        Err(e) => {
            warn!("Failed to get proposal {}: {}", proposal_id, e);
            error_json!("Failed to get proposal", e.to_string())
        }
    }
}

/// GET /api/workflows/patterns
pub async fn list_patterns(
    state: web::Data<AppState>,
) -> impl Responder {
    info!("GET /api/workflows/patterns");

    match state.workflow_repository.get_patterns(100).await {
        Ok(patterns) => {
            let total = patterns.len();
            ok_json!(json!({
                "patterns": patterns,
                "total": total
            }))
        }
        Err(e) => {
            warn!("Failed to list patterns: {}", e);
            error_json!("Failed to list patterns", e.to_string())
        }
    }
}

/// POST /api/workflows/proposals/{id}/promote
///
/// Promotes an approved proposal to a deployed pattern. Only proposals with
/// status `approved` can be promoted. Creates a `WorkflowPattern` node and
/// transitions the proposal status to `deployed`.
pub async fn promote_proposal(
    state: web::Data<AppState>,
    proposal_id: web::Path<String>,
) -> impl Responder {
    info!("POST /api/workflows/proposals/{}/promote", proposal_id);

    // 1. Fetch the proposal and validate status
    let proposal = match state.workflow_repository.get_proposal(&proposal_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return not_found!(format!("Proposal {} not found", proposal_id)),
        Err(e) => {
            warn!("Failed to get proposal {}: {}", proposal_id, e);
            return error_json!("Failed to get proposal", e.to_string());
        }
    };

    if proposal.status != WorkflowStatus::Approved {
        return bad_request!(format!(
            "Only approved proposals can be promoted (current status: {:?})",
            proposal.status
        ));
    }

    // 2. Build the pattern from the proposal
    let now = chrono::Utc::now().to_rfc3339();
    let pattern = WorkflowPattern {
        id: format!("wfp-{}", uuid::Uuid::new_v4()),
        title: proposal.title.clone(),
        description: proposal.description.clone(),
        active_version_id: proposal.id.clone(),
        deployed_at: now,
        deployed_by: "api".to_string(),
        adoption_count: 0,
        rollback_target_id: None,
    };

    // 3. Promote in the repository (creates pattern + transitions status)
    if let Err(e) = state.workflow_repository.promote_to_pattern(&proposal_id, &pattern).await {
        warn!("Failed to promote proposal {}: {}", proposal_id, e);
        return error_json!("Failed to promote proposal", e.to_string());
    }

    ok_json!(json!({
        "patternId": pattern.id,
        "title": pattern.title,
        "status": "deployed"
    }))
}

/// Route configuration for workflow proposals and patterns.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/workflows")
            .wrap(crate::middleware::RequireAuth::authenticated())
            .route("/proposals", web::get().to(list_proposals))
            .route("/proposals", web::post().to(create_proposal))
            .route("/proposals/{id}", web::get().to(get_proposal))
            .route("/proposals/{id}/promote", web::post().to(promote_proposal))
            .route("/patterns", web::get().to(list_patterns)),
    );
}
