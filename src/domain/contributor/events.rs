//! BC18 Contributor Enablement — domain events.
//!
//! Emitted by aggregate mutations. Published via the event bus in the
//! application layer; this module is pure data.
//!
//! Event names and shapes match DDD §BC18 "Domain Events".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::value_objects::{ShareState, SuggestionKind};
use crate::events::types::DomainEvent;
use crate::utils::json::to_json;

// Local helper — mirrors the macro pattern used in `events/domain_events.rs`
// and `events/enterprise_events.rs` without depending on a cross-crate macro.
macro_rules! impl_contributor_event {
    ($type:ty, $event_type:expr, $aggregate_type:expr, $id_field:ident) => {
        impl DomainEvent for $type {
            fn event_type(&self) -> &'static str {
                $event_type
            }
            fn aggregate_id(&self) -> &str {
                &self.$id_field
            }
            fn timestamp(&self) -> DateTime<Utc> {
                self.timestamp
            }
            fn aggregate_type(&self) -> &'static str {
                $aggregate_type
            }
            fn to_json_string(&self) -> Result<String, serde_json::Error> {
                to_json(self).map_err(|e| {
                    let msg = format!("JSON serialization error: {}", e);
                    serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, msg))
                })
            }
        }
    };
}

// ==================== ContributorWorkspace ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceOpenedEvent {
    pub workspace_id: String,
    pub webid: String,
    pub focus_token: Option<String>,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    WorkspaceOpenedEvent,
    "WorkspaceOpened",
    "ContributorWorkspace",
    workspace_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceClosedEvent {
    pub workspace_id: String,
    pub duration_seconds: i64,
    pub artifacts_created: u32,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    WorkspaceClosedEvent,
    "WorkspaceClosed",
    "ContributorWorkspace",
    workspace_id
);

// ==================== GuidanceSession ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidanceSessionStartedEvent {
    pub session_id: String,
    pub workspace_id: String,
    pub focus_token: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    GuidanceSessionStartedEvent,
    "GuidanceSessionStarted",
    "GuidanceSession",
    session_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidanceSessionEndedEvent {
    pub session_id: String,
    pub suggestions_accepted: u32,
    pub suggestions_dismissed: u32,
    pub artifacts_produced: u32,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    GuidanceSessionEndedEvent,
    "GuidanceSessionEnded",
    "GuidanceSession",
    session_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionAcceptedEvent {
    pub session_id: String,
    pub suggestion_kind: SuggestionKind,
    pub suggestion_ref: String,
    pub latency_ms: u64,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    SuggestionAcceptedEvent,
    "SuggestionAccepted",
    "GuidanceSession",
    session_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionDismissedEvent {
    pub session_id: String,
    pub suggestion_ref: String,
    pub reason_hint: Option<String>,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    SuggestionDismissedEvent,
    "SuggestionDismissed",
    "GuidanceSession",
    session_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NudgeEmittedEvent {
    pub envelope_id: String,
    pub session_id: String,
    pub suggestion_count: u32,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    NudgeEmittedEvent,
    "NudgeEmitted",
    "GuidanceSession",
    envelope_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartnerBoundEvent {
    pub session_id: String,
    pub partner_id: String,
    pub scope: Vec<String>,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    PartnerBoundEvent,
    "PartnerBound",
    "GuidanceSession",
    session_id
);

// ==================== WorkArtifact ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkArtifactCreatedEvent {
    pub artifact_id: String,
    pub workspace_id: String,
    pub kind: String,
    pub pod_uri: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    WorkArtifactCreatedEvent,
    "WorkArtifactCreated",
    "WorkArtifact",
    artifact_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkArtifactUpdatedEvent {
    pub artifact_id: String,
    pub new_pod_uri: String,
    pub change_summary: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    WorkArtifactUpdatedEvent,
    "WorkArtifactUpdated",
    "WorkArtifact",
    artifact_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkArtifactShareStateChangedEvent {
    pub artifact_id: String,
    pub from_state: ShareState,
    pub to_state: ShareState,
    pub intent_id: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    WorkArtifactShareStateChangedEvent,
    "WorkArtifactShareStateChanged",
    "WorkArtifact",
    artifact_id
);

// ==================== ShareIntent ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareIntentCreatedEvent {
    pub intent_id: String,
    pub artifact_id: String,
    pub from_state: ShareState,
    pub to_state: ShareState,
    pub rationale: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    ShareIntentCreatedEvent,
    "ShareIntentCreated",
    "ShareIntent",
    intent_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareIntentApprovedEvent {
    pub intent_id: String,
    pub policy_eval_id: String,
    pub downstream_case_id: Option<String>,
    pub downstream_kind: Option<String>,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    ShareIntentApprovedEvent,
    "ShareIntentApproved",
    "ShareIntent",
    intent_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareIntentRejectedEvent {
    pub intent_id: String,
    pub policy_eval_id: Option<String>,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    ShareIntentRejectedEvent,
    "ShareIntentRejected",
    "ShareIntent",
    intent_id
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareIntentRevokedEvent {
    pub intent_id: String,
    pub by_webid: String,
    pub reason: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    ShareIntentRevokedEvent,
    "ShareIntentRevoked",
    "ShareIntent",
    intent_id
);

// ==================== ContributorProfile ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributorProfileUpdatedEvent {
    pub webid: String,
    pub change_summary: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    ContributorProfileUpdatedEvent,
    "ContributorProfileUpdated",
    "ContributorProfile",
    webid
);

// ==================== Automation ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationTriggeredEvent {
    pub routine_id: String,
    pub workspace_id: Option<String>,
    pub output_inbox_uri: String,
    pub timestamp: DateTime<Utc>,
}
impl_contributor_event!(
    AutomationTriggeredEvent,
    "AutomationTriggered",
    "AutomationRoutine",
    routine_id
);
