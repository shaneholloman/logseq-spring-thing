//! BC18 `ShareIntent` aggregate (STUB — C4 owns orchestration).
//!
//! This is the BC18-local model for a contributor-raised share intent. The
//! orchestration (policy-engine calls, WAC mutation, broker case creation,
//! downstream dispatch) lives in agent C4's `ShareOrchestratorActor` and is
//! NOT implemented here.
//!
//! This aggregate carries:
//! - the request shape (`from_state`, `to_state`, `rationale`)
//! - a status lifecycle (`Pending → Approved | Rejected | Revoked`)
//! - the linkage back to the orchestration's outputs
//!   (`policy_eval_id`, `downstream_case_id`, `downstream_kind`) so BC18
//!   consumers can reason about the intent without reaching into C4.
//!
//! Invariants enforced here:
//! - `from_state` and `to_state` must differ (no no-op intents).
//! - `to_state` must be strictly forward of `from_state` (monotonic;
//!   DDD §BC18 invariant 2). Downward motion uses `Self::revoke`.
//! - A revoked intent must be terminal; approval/rejection after revoke is
//!   rejected. Orchestration-level downstream dispatch guarantees are the
//!   responsibility of C4.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::events::{
    ShareIntentApprovedEvent, ShareIntentCreatedEvent, ShareIntentRejectedEvent,
    ShareIntentRevokedEvent,
};
use super::value_objects::{ArtifactRef, ShareState};
use crate::utils::time;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ShareIntentStatus {
    Pending,
    Approved,
    Rejected,
    Revoked,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ShareIntentError {
    #[error("from-state and to-state must differ")]
    NoOpIntent,
    #[error("share intent must move forward ({from:?} -> {to:?})")]
    NonMonotonicIntent { from: ShareState, to: ShareState },
    #[error("intent already resolved ({0:?})")]
    AlreadyResolved(ShareIntentStatus),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareIntent {
    pub intent_id: String,
    pub artifact: ArtifactRef,
    pub from_state: ShareState,
    pub to_state: ShareState,
    pub rationale: String,
    pub status: ShareIntentStatus,
    pub policy_eval_id: Option<String>,
    pub downstream_case_id: Option<String>,
    pub downstream_kind: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
}

impl ShareIntent {
    /// Opens a `Pending` share intent. C4 orchestration picks it up from here.
    pub fn open(
        artifact: ArtifactRef,
        from_state: ShareState,
        to_state: ShareState,
        rationale: impl Into<String>,
        session_id: Option<String>,
    ) -> Result<(Self, ShareIntentCreatedEvent), ShareIntentError> {
        if from_state == to_state {
            return Err(ShareIntentError::NoOpIntent);
        }
        if !from_state.is_forward(to_state) {
            return Err(ShareIntentError::NonMonotonicIntent {
                from: from_state,
                to: to_state,
            });
        }
        let intent_id = Uuid::new_v4().to_string();
        let rationale = rationale.into();
        let requested_at = time::now();
        let intent = Self {
            intent_id: intent_id.clone(),
            artifact: artifact.clone(),
            from_state,
            to_state,
            rationale: rationale.clone(),
            status: ShareIntentStatus::Pending,
            policy_eval_id: None,
            downstream_case_id: None,
            downstream_kind: None,
            requested_at,
            resolved_at: None,
            session_id,
        };
        let evt = ShareIntentCreatedEvent {
            intent_id,
            artifact_id: artifact.artifact_id.clone(),
            from_state,
            to_state,
            rationale,
            timestamp: requested_at,
        };
        Ok((intent, evt))
    }

    /// Marks the intent as approved. Downstream identifiers are supplied by
    /// the C4 orchestrator.
    pub fn approve(
        &mut self,
        policy_eval_id: impl Into<String>,
        downstream_case_id: Option<String>,
        downstream_kind: Option<String>,
    ) -> Result<ShareIntentApprovedEvent, ShareIntentError> {
        self.assert_pending()?;
        let policy_eval_id = policy_eval_id.into();
        self.policy_eval_id = Some(policy_eval_id.clone());
        self.downstream_case_id = downstream_case_id.clone();
        self.downstream_kind = downstream_kind.clone();
        self.status = ShareIntentStatus::Approved;
        self.resolved_at = Some(time::now());
        Ok(ShareIntentApprovedEvent {
            intent_id: self.intent_id.clone(),
            policy_eval_id,
            downstream_case_id,
            downstream_kind,
            timestamp: self.resolved_at.unwrap(),
        })
    }

    /// Marks the intent as rejected by policy or broker.
    pub fn reject(
        &mut self,
        policy_eval_id: Option<String>,
        reason: impl Into<String>,
    ) -> Result<ShareIntentRejectedEvent, ShareIntentError> {
        self.assert_pending()?;
        self.status = ShareIntentStatus::Rejected;
        self.policy_eval_id = policy_eval_id.clone();
        self.resolved_at = Some(time::now());
        Ok(ShareIntentRejectedEvent {
            intent_id: self.intent_id.clone(),
            policy_eval_id,
            reason: reason.into(),
            timestamp: self.resolved_at.unwrap(),
        })
    }

    /// Revokes the intent. Terminal. Once revoked, no approval or rejection
    /// is accepted.
    pub fn revoke(
        &mut self,
        by_webid: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<ShareIntentRevokedEvent, ShareIntentError> {
        if matches!(self.status, ShareIntentStatus::Revoked) {
            return Err(ShareIntentError::AlreadyResolved(self.status));
        }
        self.status = ShareIntentStatus::Revoked;
        self.resolved_at = Some(time::now());
        Ok(ShareIntentRevokedEvent {
            intent_id: self.intent_id.clone(),
            by_webid: by_webid.into(),
            reason: reason.into(),
            timestamp: self.resolved_at.unwrap(),
        })
    }

    fn assert_pending(&self) -> Result<(), ShareIntentError> {
        if self.status != ShareIntentStatus::Pending {
            return Err(ShareIntentError::AlreadyResolved(self.status));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aref() -> ArtifactRef {
        ArtifactRef::new("art-1", "https://alice.example/private/kg/n1.ttl")
    }

    #[test]
    fn open_requires_forward_motion() {
        assert_eq!(
            ShareIntent::open(aref(), ShareState::Team, ShareState::Team, "r", None)
                .unwrap_err(),
            ShareIntentError::NoOpIntent
        );
        assert_eq!(
            ShareIntent::open(aref(), ShareState::Mesh, ShareState::Team, "r", None)
                .unwrap_err(),
            ShareIntentError::NonMonotonicIntent {
                from: ShareState::Mesh,
                to: ShareState::Team
            }
        );
    }

    #[test]
    fn approve_is_terminal() {
        let (mut i, _) =
            ShareIntent::open(aref(), ShareState::Private, ShareState::Team, "r", None).unwrap();
        i.approve("pol-1", Some("case-1".into()), Some("broker".into())).unwrap();
        assert_eq!(i.status, ShareIntentStatus::Approved);
        assert!(matches!(
            i.reject(None, "late"),
            Err(ShareIntentError::AlreadyResolved(ShareIntentStatus::Approved))
        ));
    }

    #[test]
    fn revoke_blocks_subsequent_state_changes() {
        let (mut i, _) =
            ShareIntent::open(aref(), ShareState::Private, ShareState::Team, "r", None).unwrap();
        i.revoke("did:alice", "changed my mind").unwrap();
        assert_eq!(i.status, ShareIntentStatus::Revoked);
        assert!(matches!(
            i.revoke("did:alice", "again"),
            Err(ShareIntentError::AlreadyResolved(ShareIntentStatus::Revoked))
        ));
    }
}
