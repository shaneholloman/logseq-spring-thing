//! Briefing Service — orchestrates the brief → execute → debrief workflow
//! from the VisionFlow Rust backend through the Management API.
//!
//! This service translates BriefingRequest structs into Management API calls,
//! creating briefs, spawning role-specific agents, and consolidating debriefs.

use crate::services::management_api_client::ManagementApiClient;
use crate::types::user_context::{BriefingRequest, BriefingResponse, RoleTask, UserContext};
use log::info;

pub struct BriefingService {
    api_client: ManagementApiClient,
}

impl BriefingService {
    pub fn new(api_client: ManagementApiClient) -> Self {
        Self { api_client }
    }

    /// Submit a briefing request to the Management API.
    ///
    /// This creates a brief file in the team folder structure, optionally creates
    /// a Beads epic, then spawns role-specific agents to respond.
    pub async fn submit_brief(
        &self,
        request: &BriefingRequest,
        user_context: &UserContext,
    ) -> Result<BriefingResponse, BriefingError> {
        info!(
            "[BriefingService] Submitting brief for user={}, roles={:?}",
            user_context.display_name, request.roles
        );

        // Step 1: Create the brief via Management API
        let brief_result = self
            .api_client
            .create_brief(
                &request.content,
                &request.roles,
                user_context,
                request.version.as_deref(),
                request.brief_type.as_deref(),
                request.slug.as_deref(),
            )
            .await
            .map_err(|e| BriefingError::ApiError(format!("Failed to create brief: {}", e)))?;

        let brief_id = brief_result.brief_id.clone();
        let brief_path = brief_result.brief_path.clone();
        let bead_id = brief_result.bead_id.clone();

        // Step 2: Execute the brief (spawn role agents)
        let role_tasks = self
            .api_client
            .execute_brief(
                &brief_id,
                &brief_path,
                &request.roles,
                user_context,
                bead_id.as_deref(),
            )
            .await
            .map_err(|e| BriefingError::ApiError(format!("Failed to execute brief: {}", e)))?;

        info!(
            "[BriefingService] Brief {} submitted: {} role agents spawned",
            brief_id,
            role_tasks.len()
        );

        Ok(BriefingResponse {
            brief_id,
            brief_path,
            bead_id,
            role_tasks,
        })
    }

    /// Request a debrief consolidation for a completed brief.
    pub async fn request_debrief(
        &self,
        brief_id: &str,
        role_tasks: &[RoleTask],
        user_context: &UserContext,
    ) -> Result<String, BriefingError> {
        info!(
            "[BriefingService] Requesting debrief for brief={}, user={}",
            brief_id, user_context.display_name
        );

        let debrief_path = self
            .api_client
            .create_debrief(brief_id, role_tasks, user_context)
            .await
            .map_err(|e| BriefingError::ApiError(format!("Failed to create debrief: {}", e)))?;

        info!("[BriefingService] Debrief created at {}", debrief_path);

        Ok(debrief_path)
    }
}

#[derive(Debug)]
pub enum BriefingError {
    ApiError(String),
}

impl std::fmt::Display for BriefingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BriefingError::ApiError(msg) => write!(f, "Briefing API error: {}", msg),
        }
    }
}

impl std::error::Error for BriefingError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- BriefingError display and error trait ----

    #[test]
    fn test_briefing_error_display() {
        let err = BriefingError::ApiError("connection timed out".to_string());
        let msg = format!("{}", err);
        assert_eq!(msg, "Briefing API error: connection timed out");
    }

    #[test]
    fn test_briefing_error_is_std_error() {
        let err = BriefingError::ApiError("test".to_string());
        // Verify it implements std::error::Error
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_briefing_error_debug() {
        let err = BriefingError::ApiError("debug test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("ApiError"));
        assert!(debug_str.contains("debug test"));
    }

    // ---- BriefingRequest type validation ----

    #[test]
    fn test_briefing_request_serde_roundtrip() {
        let req = BriefingRequest {
            content: "Analyze security posture for Q3".to_string(),
            roles: vec![
                "architect".to_string(),
                "ciso".to_string(),
                "dev".to_string(),
            ],
            version: Some("v0.3.0".to_string()),
            brief_type: Some("security-review".to_string()),
            slug: Some("q3-security".to_string()),
        };

        let json = serde_json::to_string(&req).unwrap();
        let back: BriefingRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(back.content, req.content);
        assert_eq!(back.roles, req.roles);
        assert_eq!(back.version, req.version);
        assert_eq!(back.brief_type, req.brief_type);
        assert_eq!(back.slug, req.slug);
    }

    #[test]
    fn test_briefing_request_minimal() {
        let json = r#"{"content": "Hello", "roles": ["dev"]}"#;
        let req: BriefingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.content, "Hello");
        assert_eq!(req.roles, vec!["dev"]);
        assert!(req.version.is_none());
        assert!(req.brief_type.is_none());
        assert!(req.slug.is_none());
    }

    // ---- BriefingResponse serde ----

    #[test]
    fn test_briefing_response_serialization() {
        let resp = BriefingResponse {
            brief_id: "brief-001".to_string(),
            brief_path: "/briefs/2026/brief-001.md".to_string(),
            bead_id: Some("bead-epic-001".to_string()),
            role_tasks: vec![RoleTask {
                role: "architect".to_string(),
                task_id: "task-arch-001".to_string(),
                bead_id: Some("bead-arch-001".to_string()),
                response_path: "/briefs/2026/architect_response.md".to_string(),
            }],
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("brief-001"));
        assert!(json.contains("bead-epic-001"));
        assert!(json.contains("architect"));

        let back: BriefingResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.brief_id, resp.brief_id);
        assert_eq!(back.role_tasks.len(), 1);
    }

    // ---- RoleTask serde ----

    #[test]
    fn test_role_task_with_bead() {
        let task = RoleTask {
            role: "dev".to_string(),
            task_id: "task-dev-001".to_string(),
            bead_id: Some("bead-dev-001".to_string()),
            response_path: "/briefs/test/dev.md".to_string(),
        };
        let json = serde_json::to_string(&task).unwrap();
        let back: RoleTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.bead_id, Some("bead-dev-001".to_string()));
    }

    #[test]
    fn test_role_task_without_bead() {
        let json = r#"{
            "role": "reviewer",
            "task_id": "task-rev-001",
            "response_path": "/briefs/test/reviewer.md"
        }"#;
        let task: RoleTask = serde_json::from_str(json).unwrap();
        assert_eq!(task.role, "reviewer");
        assert!(task.bead_id.is_none());
    }

    // ---- UserContext serde ----

    #[test]
    fn test_user_context_full_roundtrip() {
        let ctx = UserContext {
            user_id: "npub1abc".to_string(),
            pubkey: "deadbeef".repeat(8),
            display_name: "Dr_Test".to_string(),
            session_id: "session-uuid-123".to_string(),
            is_power_user: true,
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let back: UserContext = serde_json::from_str(&json).unwrap();
        assert_eq!(back.display_name, "Dr_Test");
        assert!(back.is_power_user);
        assert_eq!(back.pubkey.len(), 64);
    }

    // ---- BriefingService construction ----

    #[test]
    fn test_briefing_service_can_be_constructed() {
        // Verify the ManagementApiClient can be constructed with test values
        // (it won't connect, but the struct should build)
        let client =
            ManagementApiClient::new("localhost".to_string(), 9190, "test-api-key".to_string());
        let _service = BriefingService::new(client);
        // Service is constructed without panicking — success
    }
}
