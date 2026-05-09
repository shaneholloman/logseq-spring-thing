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
use crate::settings::auth_extractor::AuthenticatedUser;
use crate::types::user_context::{BriefingRequest, RoleTask, UserContext};

/// POST /api/briefs — Submit a new briefing request.
///
/// Expects a JSON body with content, roles, and user_context.
/// Returns the brief ID, path, bead ID, and spawned role task IDs.
pub async fn submit_brief(
    _user: AuthenticatedUser,
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
    _user: AuthenticatedUser,
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- SubmitBriefRequest deserialization ----

    #[test]
    fn test_submit_brief_request_deserialization() {
        let json = r#"{
            "briefing": {
                "content": "Analyze the security posture",
                "roles": ["architect", "ciso"]
            },
            "user_context": {
                "user_id": "npub1test",
                "pubkey": "aabbccdd",
                "display_name": "test_user",
                "session_id": "sess-001",
                "is_power_user": false
            }
        }"#;
        let req: SubmitBriefRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.briefing.content, "Analyze the security posture");
        assert_eq!(req.briefing.roles.len(), 2);
        assert!(req.briefing.roles.contains(&"architect".to_string()));
        assert!(req.briefing.roles.contains(&"ciso".to_string()));
        assert_eq!(req.user_context.display_name, "test_user");
        assert!(!req.user_context.is_power_user);
    }

    #[test]
    fn test_submit_brief_request_with_optional_fields() {
        let json = r#"{
            "briefing": {
                "content": "Test brief",
                "roles": ["dev"],
                "version": "v0.2.33",
                "brief_type": "daily-brief",
                "slug": "daily-2026-05-09"
            },
            "user_context": {
                "user_id": "npub1test",
                "pubkey": "aabbccdd",
                "display_name": "developer",
                "session_id": "sess-002",
                "is_power_user": true
            }
        }"#;
        let req: SubmitBriefRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.briefing.version.as_deref(), Some("v0.2.33"));
        assert_eq!(req.briefing.brief_type.as_deref(), Some("daily-brief"));
        assert_eq!(req.briefing.slug.as_deref(), Some("daily-2026-05-09"));
        assert!(req.user_context.is_power_user);
    }

    // ---- DebriefRequest deserialization ----

    #[test]
    fn test_debrief_request_deserialization() {
        let json = r#"{
            "role_tasks": [
                {
                    "role": "architect",
                    "task_id": "task-arch-001",
                    "bead_id": "bead-arch-001",
                    "response_path": "/briefs/test/architect_response.md"
                },
                {
                    "role": "dev",
                    "task_id": "task-dev-001",
                    "response_path": "/briefs/test/dev_response.md"
                }
            ],
            "user_context": {
                "user_id": "npub1test",
                "pubkey": "aabbccdd",
                "display_name": "test_user",
                "session_id": "sess-003",
                "is_power_user": false
            }
        }"#;
        let req: DebriefRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.role_tasks.len(), 2);
        assert_eq!(req.role_tasks[0].role, "architect");
        assert!(req.role_tasks[0].bead_id.is_some());
        assert_eq!(req.role_tasks[1].role, "dev");
        assert!(req.role_tasks[1].bead_id.is_none());
    }

    // ---- BriefingRequest roundtrip ----

    #[test]
    fn test_briefing_request_roundtrip_serde() {
        use crate::test_helpers::make_test_briefing_request;
        let req = make_test_briefing_request();
        let json = serde_json::to_string(&req).unwrap();
        let back: BriefingRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content, req.content);
        assert_eq!(back.roles, req.roles);
        assert_eq!(back.version, req.version);
    }

    // ---- UserContext roundtrip ----

    #[test]
    fn test_user_context_roundtrip_serde() {
        use crate::test_helpers::make_test_user_context;
        let ctx = make_test_user_context();
        let json = serde_json::to_string(&ctx).unwrap();
        let back: UserContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, ctx.user_id);
        assert_eq!(back.pubkey, ctx.pubkey);
        assert_eq!(back.display_name, ctx.display_name);
        assert_eq!(back.session_id, ctx.session_id);
        assert_eq!(back.is_power_user, ctx.is_power_user);
    }

    // ---- RoleTask bead_id extraction logic (from handler) ----

    #[test]
    fn test_bead_id_extraction_from_role_tasks() {
        let tasks = vec![
            RoleTask {
                role: "architect".to_string(),
                task_id: "t1".to_string(),
                bead_id: None,
                response_path: "/r1".to_string(),
            },
            RoleTask {
                role: "dev".to_string(),
                task_id: "t2".to_string(),
                bead_id: Some("epic-bead-42".to_string()),
                response_path: "/r2".to_string(),
            },
        ];

        let brief_id = "brief-001";
        let bead_id = tasks
            .iter()
            .find_map(|rt| rt.bead_id.as_deref())
            .unwrap_or(brief_id)
            .to_string();

        assert_eq!(bead_id, "epic-bead-42");
    }

    #[test]
    fn test_bead_id_fallback_to_brief_id() {
        let tasks = vec![
            RoleTask {
                role: "architect".to_string(),
                task_id: "t1".to_string(),
                bead_id: None,
                response_path: "/r1".to_string(),
            },
        ];

        let brief_id = "brief-fallback";
        let bead_id = tasks
            .iter()
            .find_map(|rt| rt.bead_id.as_deref())
            .unwrap_or(brief_id)
            .to_string();

        assert_eq!(bead_id, "brief-fallback");
    }

    // ---- BriefingError display ----

    #[test]
    fn test_briefing_error_display() {
        let err = BriefingError::ApiError("connection refused".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("connection refused"));
        assert!(msg.contains("Briefing API error"));
    }
}
