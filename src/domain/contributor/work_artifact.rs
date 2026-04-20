//! BC18 `WorkArtifact` aggregate.
//!
//! A pod-resident unit of work produced inside a `ContributorWorkspace`.
//! Advances through [`ShareState`]. Lineage is append-only.
//!
//! Invariants (DDD §BC18):
//! 1. Exactly one current `ShareState` and one canonical pod URI.
//! 2. `ShareState` moves forward only. Downward motion is an explicit
//!    revocation; the lineage still records the transition.
//! 7. Every artifact is rooted in a [`super::ContributorWorkspace`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::events::{
    WorkArtifactCreatedEvent, WorkArtifactShareStateChangedEvent, WorkArtifactUpdatedEvent,
};
use super::value_objects::{ArtifactKind, ArtifactLineage, ShareState};
use crate::utils::time;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum WorkArtifactError {
    #[error("artifact lacks a rooting workspace")]
    OrphanedArtifact,
    #[error("share-state transition {from:?} -> {to:?} is not monotonic")]
    NonMonotonicTransition { from: ShareState, to: ShareState },
    #[error("share-state transition skips an intermediate state ({from:?} -> {to:?})")]
    SkipTransition { from: ShareState, to: ShareState },
    #[error("revocation must strictly lower the share state")]
    InvalidRevocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkArtifact {
    pub artifact_id: String,
    pub workspace_id: String,
    pub kind: ArtifactKind,
    pub title: String,
    pub pod_uri: String,
    pub share_state: ShareState,
    pub lineage: ArtifactLineage,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkArtifact {
    /// Creates a new private artifact rooted in the supplied workspace.
    ///
    /// Returns [`WorkArtifactError::OrphanedArtifact`] if `workspace_id` is empty.
    pub fn new(
        workspace_id: impl Into<String>,
        kind: ArtifactKind,
        title: impl Into<String>,
        pod_uri: impl Into<String>,
    ) -> Result<(Self, WorkArtifactCreatedEvent), WorkArtifactError> {
        let workspace_id = workspace_id.into();
        if workspace_id.is_empty() {
            return Err(WorkArtifactError::OrphanedArtifact);
        }
        let now = time::now();
        let artifact_id = Uuid::new_v4().to_string();
        let pod_uri = pod_uri.into();
        let kind_str = format!("{:?}", kind);
        let artifact = Self {
            artifact_id: artifact_id.clone(),
            workspace_id: workspace_id.clone(),
            kind,
            title: title.into(),
            pod_uri: pod_uri.clone(),
            share_state: ShareState::Private,
            lineage: ArtifactLineage::default(),
            created_at: now,
            updated_at: now,
        };
        let evt = WorkArtifactCreatedEvent {
            artifact_id,
            workspace_id,
            kind: kind_str,
            pod_uri,
            timestamp: now,
        };
        Ok((artifact, evt))
    }

    /// Updates the canonical pod URI (pod-first write discipline — backend
    /// mirrors the pod). Emits [`WorkArtifactUpdatedEvent`].
    pub fn update_pod_uri(
        &mut self,
        new_pod_uri: impl Into<String>,
        change_summary: impl Into<String>,
    ) -> WorkArtifactUpdatedEvent {
        let new_pod_uri = new_pod_uri.into();
        self.pod_uri = new_pod_uri.clone();
        self.updated_at = time::now();
        WorkArtifactUpdatedEvent {
            artifact_id: self.artifact_id.clone(),
            new_pod_uri,
            change_summary: change_summary.into(),
            timestamp: self.updated_at,
        }
    }

    /// Advances share state monotonically by one step. Rejects skips and
    /// backward motion — those belong to [`Self::record_revocation`].
    pub fn advance_share_state(
        &mut self,
        to: ShareState,
        intent_id: impl Into<String>,
    ) -> Result<WorkArtifactShareStateChangedEvent, WorkArtifactError> {
        if !self.share_state.is_forward(to) {
            return Err(WorkArtifactError::NonMonotonicTransition {
                from: self.share_state,
                to,
            });
        }
        if !self.share_state.is_next(to) {
            return Err(WorkArtifactError::SkipTransition {
                from: self.share_state,
                to,
            });
        }
        let intent_id = intent_id.into();
        let from = self.share_state;
        self.lineage.push(&intent_id, from, to, None);
        self.share_state = to;
        self.updated_at = time::now();
        Ok(WorkArtifactShareStateChangedEvent {
            artifact_id: self.artifact_id.clone(),
            from_state: from,
            to_state: to,
            intent_id,
            timestamp: self.updated_at,
        })
    }

    /// Records an explicit revocation — the only legal path to a lower share
    /// state. Audits through the lineage chain.
    pub fn record_revocation(
        &mut self,
        to: ShareState,
        intent_id: impl Into<String>,
    ) -> Result<WorkArtifactShareStateChangedEvent, WorkArtifactError> {
        if to.rank() >= self.share_state.rank() {
            return Err(WorkArtifactError::InvalidRevocation);
        }
        let intent_id = intent_id.into();
        let from = self.share_state;
        self.lineage.push(&intent_id, from, to, None);
        self.share_state = to;
        self.updated_at = time::now();
        Ok(WorkArtifactShareStateChangedEvent {
            artifact_id: self.artifact_id.clone(),
            from_state: from,
            to_state: to,
            intent_id,
            timestamp: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> WorkArtifact {
        let (a, _) = WorkArtifact::new(
            "ws-1",
            ArtifactKind::Note,
            "note",
            "https://alice.example/private/kg/n1.ttl",
        )
        .unwrap();
        a
    }

    #[test]
    fn new_artifact_is_private_and_rooted() {
        let a = fresh();
        assert_eq!(a.share_state, ShareState::Private);
        assert_eq!(a.workspace_id, "ws-1");
        assert!(a.lineage.is_empty());
    }

    #[test]
    fn orphaned_artifact_rejected() {
        let e = WorkArtifact::new("", ArtifactKind::Note, "n", "uri").unwrap_err();
        assert_eq!(e, WorkArtifactError::OrphanedArtifact);
    }

    #[test]
    fn advance_is_step_by_step() {
        let mut a = fresh();
        let e = a.advance_share_state(ShareState::Mesh, "intent-1").unwrap_err();
        assert!(matches!(e, WorkArtifactError::SkipTransition { .. }));
        assert_eq!(a.share_state, ShareState::Private);

        a.advance_share_state(ShareState::Team, "intent-1").unwrap();
        assert_eq!(a.share_state, ShareState::Team);
        a.advance_share_state(ShareState::Mesh, "intent-2").unwrap();
        assert_eq!(a.share_state, ShareState::Mesh);
        assert_eq!(a.lineage.entries.len(), 2);
    }

    #[test]
    fn backward_motion_requires_revocation() {
        let mut a = fresh();
        a.advance_share_state(ShareState::Team, "intent-1").unwrap();
        let err = a
            .advance_share_state(ShareState::Private, "intent-bad")
            .unwrap_err();
        assert!(matches!(err, WorkArtifactError::NonMonotonicTransition { .. }));

        let evt = a.record_revocation(ShareState::Private, "intent-rev").unwrap();
        assert_eq!(a.share_state, ShareState::Private);
        assert_eq!(evt.from_state, ShareState::Team);
        assert_eq!(evt.to_state, ShareState::Private);
    }
}
