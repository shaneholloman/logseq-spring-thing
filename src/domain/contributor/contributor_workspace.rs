//! BC18 `ContributorWorkspace` aggregate root.
//!
//! Owns the lifecycle of every child entity: guidance sessions, work
//! artifacts, share intents. Keeps the current focus snapshot and the
//! subset-of-partner-scopes that bounds session delegations.
//!
//! Per ADR-057 and DDD §BC18, the workspace is deliberately small: bulk
//! operations go through the `ShareOrchestrator` rather than inflating this
//! aggregate.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::events::{WorkspaceClosedEvent, WorkspaceOpenedEvent};
use super::guidance_session::GuidanceSession;
use super::share_intent::ShareIntent;
use super::value_objects::WorkspaceFocus;
use super::work_artifact::WorkArtifact;
use crate::utils::time;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ContributorWorkspaceError {
    #[error("workspace already closed")]
    AlreadyClosed,
    #[error("artifact {0} does not belong to this workspace")]
    ForeignArtifact(String),
    #[error("session {0} does not belong to this workspace")]
    ForeignSession(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributorWorkspace {
    pub workspace_id: String,
    pub webid: String,
    pub name: String,
    pub session_scope: Vec<String>,
    pub focus: Option<WorkspaceFocus>,
    pub artifacts: HashMap<String, WorkArtifact>,
    pub sessions: HashMap<String, GuidanceSession>,
    pub share_intents: HashMap<String, ShareIntent>,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

impl ContributorWorkspace {
    /// Opens a fresh workspace. Emits `WorkspaceOpened`.
    pub fn open(
        webid: impl Into<String>,
        name: impl Into<String>,
        session_scope: Vec<String>,
        focus: Option<WorkspaceFocus>,
    ) -> (Self, WorkspaceOpenedEvent) {
        let webid = webid.into();
        let workspace_id = Uuid::new_v4().to_string();
        let opened_at = time::now();
        let focus_token = focus.as_ref().map(WorkspaceFocus::focus_token);
        let ws = Self {
            workspace_id: workspace_id.clone(),
            webid: webid.clone(),
            name: name.into(),
            session_scope,
            focus,
            artifacts: HashMap::new(),
            sessions: HashMap::new(),
            share_intents: HashMap::new(),
            opened_at,
            closed_at: None,
        };
        let evt = WorkspaceOpenedEvent {
            workspace_id,
            webid,
            focus_token,
            timestamp: opened_at,
        };
        (ws, evt)
    }

    /// Updates the current focus snapshot. Existing [`GuidanceSession`]s keep
    /// the focus they were opened with; only future sessions see the new one.
    pub fn set_focus(&mut self, focus: WorkspaceFocus) {
        self.focus = Some(focus);
    }

    pub fn attach_artifact(&mut self, artifact: WorkArtifact) -> Result<(), ContributorWorkspaceError> {
        if self.is_closed() {
            return Err(ContributorWorkspaceError::AlreadyClosed);
        }
        if artifact.workspace_id != self.workspace_id {
            return Err(ContributorWorkspaceError::ForeignArtifact(
                artifact.artifact_id.clone(),
            ));
        }
        self.artifacts.insert(artifact.artifact_id.clone(), artifact);
        Ok(())
    }

    pub fn attach_session(&mut self, session: GuidanceSession) -> Result<(), ContributorWorkspaceError> {
        if self.is_closed() {
            return Err(ContributorWorkspaceError::AlreadyClosed);
        }
        if session.workspace_id != self.workspace_id {
            return Err(ContributorWorkspaceError::ForeignSession(
                session.session_id.clone(),
            ));
        }
        self.sessions.insert(session.session_id.clone(), session);
        Ok(())
    }

    pub fn attach_share_intent(&mut self, intent: ShareIntent) -> Result<(), ContributorWorkspaceError> {
        if self.is_closed() {
            return Err(ContributorWorkspaceError::AlreadyClosed);
        }
        self.share_intents.insert(intent.intent_id.clone(), intent);
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.closed_at.is_some()
    }

    /// Closes the workspace. Sessions still open at close time remain
    /// append-only but inert; callers should close them first.
    pub fn close(&mut self) -> Result<WorkspaceClosedEvent, ContributorWorkspaceError> {
        if self.is_closed() {
            return Err(ContributorWorkspaceError::AlreadyClosed);
        }
        let closed_at = time::now();
        self.closed_at = Some(closed_at);
        let duration_seconds = (closed_at - self.opened_at).num_seconds();
        let evt = WorkspaceClosedEvent {
            workspace_id: self.workspace_id.clone(),
            duration_seconds,
            artifacts_created: self.artifacts.len() as u32,
            timestamp: closed_at,
        };
        Ok(evt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contributor::value_objects::{ArtifactKind, GraphSelection};

    fn focus() -> WorkspaceFocus {
        WorkspaceFocus::new(Some("proj:x".into()), GraphSelection::default(), vec![], vec![])
    }

    #[test]
    fn workspace_lifecycle_open_attach_close() {
        let (mut ws, opened) = ContributorWorkspace::open(
            "https://alice.example/profile/card#me",
            "Negentropy brief",
            vec!["read:kg".into(), "write:private".into()],
            Some(focus()),
        );
        assert_eq!(opened.workspace_id, ws.workspace_id);

        let (art, _) =
            WorkArtifact::new(&ws.workspace_id, ArtifactKind::Note, "n", "uri").unwrap();
        ws.attach_artifact(art).unwrap();

        let (sess, _) = GuidanceSession::start(&ws.workspace_id, focus());
        ws.attach_session(sess).unwrap();

        let closed = ws.close().unwrap();
        assert_eq!(closed.artifacts_created, 1);
        assert!(ws.is_closed());

        // further attaches rejected
        let (art2, _) =
            WorkArtifact::new(&ws.workspace_id, ArtifactKind::Note, "n2", "uri2").unwrap();
        assert_eq!(
            ws.attach_artifact(art2),
            Err(ContributorWorkspaceError::AlreadyClosed)
        );
    }

    #[test]
    fn foreign_artifact_rejected() {
        let (mut ws, _) = ContributorWorkspace::open("did:alice", "w", vec![], None);
        let (alien, _) =
            WorkArtifact::new("some-other-workspace", ArtifactKind::Note, "n", "uri").unwrap();
        let err = ws.attach_artifact(alien).unwrap_err();
        assert!(matches!(err, ContributorWorkspaceError::ForeignArtifact(_)));
    }
}
