//! Briefing workflow HTTP handler.
//!
//! Exposes REST endpoints for the briefing workflow:
//! - POST /api/briefs — Submit a new brief (triggers role agent execution)
//! - POST /api/briefs/{brief_id}/debrief — Request consolidated debrief
//!
//! These endpoints bridge the VisionFlow frontend (voice/UI) to the
//! Management API briefing service running in the agent container.

use actix_web::{web, HttpResponse};
use log::{error, info};
use serde::Deserialize;

use std::sync::Arc;

use crate::services::bead_lifecycle::BeadLifecycleOrchestrator;
use crate::services::briefing_service::{BriefingError, BriefingService};
use crate::types::user_context::{BriefingRequest, RoleTask, UserContext};

/// POST /api/briefs — Submit a new briefing request.
///
/// Expects a JSON body with content, roles, and user_context.
/// Returns the brief ID, path, bead ID, and spawned role task IDs.
pub async fn submit_brief(
    briefing_service: web::Data<BriefingService>,
    body: web::Json<SubmitBriefRequest>,
) -> HttpResponse {
    let request = &body.briefing;
    let user_context = &body.user_context;

    info!(
        "[briefing_handler] POST /api/briefs from user={}",
        user_context.display_name
    );

    match briefing_service.submit_brief(request, user_context).await {
        Ok(response) => {
            info!(
                "[briefing_handler] Brief {} created with {} role tasks",
                response.brief_id,
                response.role_tasks.len()
            );
            HttpResponse::Created().json(response)
        }
        Err(BriefingError::ApiError(msg)) => {
            error!("[briefing_handler] Brief submission failed: {}", msg);
            HttpResponse::BadGateway().json(serde_json::json!({
                "error": "Brief submission failed",
                "message": msg
            }))
        }
    }
}

/// POST /api/briefs/{brief_id}/debrief — Request a consolidated debrief.
pub async fn request_debrief(
    briefing_service: web::Data<BriefingService>,
    bead_orchestrator: web::Data<Arc<BeadLifecycleOrchestrator>>,
    path: web::Path<String>,
    body: web::Json<DebriefRequest>,
) -> HttpResponse {
    let brief_id = path.into_inner();
    let user_context = &body.user_context;

    info!(
        "[briefing_handler] POST /api/briefs/{}/debrief from user={}",
        brief_id, user_context.display_name
    );

    // Extract bead_id from the first role task that has one (epic bead).
    let bead_id = body
        .role_tasks
        .iter()
        .find_map(|rt| rt.bead_id.as_deref())
        .unwrap_or(&brief_id)
        .to_string();

    match briefing_service
        .request_debrief(&brief_id, &body.role_tasks, user_context)
        .await
    {
        Ok(debrief_path) => {
            // Fire-and-forget lifecycle orchestration -- tracks outcome in store.
            let orchestrator = bead_orchestrator.get_ref().clone();
            let bead_id = bead_id.clone();
            let brief_id_owned = brief_id.clone();
            let user_pubkey = user_context.pubkey.clone();
            let debrief_path_owned = debrief_path.clone();
            tokio::spawn(async move {
                orchestrator
                    .process_bead(
                        &bead_id,
                        &brief_id_owned,
                        Some(&user_pubkey),
                        &debrief_path_owned,
                    )
                    .await;
            });

            HttpResponse::Created().json(serde_json::json!({
                "brief_id": brief_id,
                "debrief_path": debrief_path
            }))
        }
        Err(BriefingError::ApiError(msg)) => {
            error!("[briefing_handler] Debrief creation failed: {}", msg);
            HttpResponse::BadGateway().json(serde_json::json!({
                "error": "Debrief creation failed",
                "message": msg
            }))
        }
    }
}

/// Configure briefing routes under /api scope.
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/briefs")
            .route("", web::post().to(submit_brief))
            .route("/{brief_id}/debrief", web::post().to(request_debrief)),
    );
}

// --- Request/Response types ---

#[derive(Debug, Deserialize)]
pub struct SubmitBriefRequest {
    pub briefing: BriefingRequest,
    pub user_context: UserContext,
}

#[derive(Debug, Deserialize)]
pub struct DebriefRequest {
    pub role_tasks: Vec<RoleTask>,
    pub user_context: UserContext,
}
