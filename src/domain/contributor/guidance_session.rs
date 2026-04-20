//! BC18 `GuidanceSession` aggregate.
//!
//! An append-only record of a single Sensei episode scoped by one
//! [`WorkspaceFocus`]. Accepted and dismissed suggestions drive BC15 KPI
//! guidance-hit-rate (DDD §BC18 invariant 5: sessions are append-only).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::events::{
    GuidanceSessionEndedEvent, GuidanceSessionStartedEvent, NudgeEmittedEvent, PartnerBoundEvent,
    SuggestionAcceptedEvent, SuggestionDismissedEvent,
};
use super::value_objects::{
    NudgeEnvelope, PartnerBinding, SuggestionKind, WorkspaceFocus,
};
use crate::utils::time;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum GuidanceSessionError {
    #[error("session already ended")]
    AlreadyEnded,
    #[error("partner delegation scope exceeds session scope")]
    PartnerScopeEscalation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidanceSession {
    pub session_id: String,
    pub workspace_id: String,
    pub focus_token: String,
    pub focus: WorkspaceFocus,
    pub nudges: Vec<NudgeEnvelope>,
    pub partner_bindings: Vec<PartnerBinding>,
    pub accepted_count: u32,
    pub dismissed_count: u32,
    pub artifacts_produced: u32,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl GuidanceSession {
    /// Starts a new session. Emits `GuidanceSessionStarted`.
    pub fn start(
        workspace_id: impl Into<String>,
        focus: WorkspaceFocus,
    ) -> (Self, GuidanceSessionStartedEvent) {
        let workspace_id = workspace_id.into();
        let session_id = Uuid::new_v4().to_string();
        let focus_token = focus.focus_token();
        let started_at = time::now();
        let session = Self {
            session_id: session_id.clone(),
            workspace_id: workspace_id.clone(),
            focus_token: focus_token.clone(),
            focus,
            nudges: Vec::new(),
            partner_bindings: Vec::new(),
            accepted_count: 0,
            dismissed_count: 0,
            artifacts_produced: 0,
            started_at,
            ended_at: None,
        };
        let evt = GuidanceSessionStartedEvent {
            session_id,
            workspace_id,
            focus_token,
            timestamp: started_at,
        };
        (session, evt)
    }

    /// Records a nudge envelope produced for this focus. Append-only.
    pub fn record_nudge(
        &mut self,
        envelope: NudgeEnvelope,
    ) -> Result<NudgeEmittedEvent, GuidanceSessionError> {
        if self.ended_at.is_some() {
            return Err(GuidanceSessionError::AlreadyEnded);
        }
        let evt = NudgeEmittedEvent {
            envelope_id: envelope.envelope_id.clone(),
            session_id: self.session_id.clone(),
            suggestion_count: envelope.suggestions.len() as u32,
            timestamp: time::now(),
        };
        self.nudges.push(envelope);
        Ok(evt)
    }

    /// Marks a suggestion as accepted. Append-only.
    pub fn accept_suggestion(
        &mut self,
        suggestion_kind: SuggestionKind,
        suggestion_ref: impl Into<String>,
        latency_ms: u64,
    ) -> Result<SuggestionAcceptedEvent, GuidanceSessionError> {
        if self.ended_at.is_some() {
            return Err(GuidanceSessionError::AlreadyEnded);
        }
        self.accepted_count += 1;
        Ok(SuggestionAcceptedEvent {
            session_id: self.session_id.clone(),
            suggestion_kind,
            suggestion_ref: suggestion_ref.into(),
            latency_ms,
            timestamp: time::now(),
        })
    }

    /// Marks a suggestion as dismissed. Append-only.
    pub fn dismiss_suggestion(
        &mut self,
        suggestion_ref: impl Into<String>,
        reason_hint: Option<String>,
    ) -> Result<SuggestionDismissedEvent, GuidanceSessionError> {
        if self.ended_at.is_some() {
            return Err(GuidanceSessionError::AlreadyEnded);
        }
        self.dismissed_count += 1;
        Ok(SuggestionDismissedEvent {
            session_id: self.session_id.clone(),
            suggestion_ref: suggestion_ref.into(),
            reason_hint,
            timestamp: time::now(),
        })
    }

    /// Increments artifact-produced counter for KPI projection.
    pub fn record_artifact_produced(&mut self) -> Result<(), GuidanceSessionError> {
        if self.ended_at.is_some() {
            return Err(GuidanceSessionError::AlreadyEnded);
        }
        self.artifacts_produced += 1;
        Ok(())
    }

    /// Binds a partner to the session. Enforces the strict-subset scope
    /// invariant against the supplied `session_scope`.
    pub fn bind_partner(
        &mut self,
        binding: PartnerBinding,
        session_scope: &[String],
    ) -> Result<PartnerBoundEvent, GuidanceSessionError> {
        if self.ended_at.is_some() {
            return Err(GuidanceSessionError::AlreadyEnded);
        }
        if !binding.is_strict_subset_of(session_scope) {
            return Err(GuidanceSessionError::PartnerScopeEscalation);
        }
        let evt = PartnerBoundEvent {
            session_id: self.session_id.clone(),
            partner_id: binding.partner_id.clone(),
            scope: binding.delegation_scope.clone(),
            timestamp: time::now(),
        };
        self.partner_bindings.push(binding);
        Ok(evt)
    }

    /// Closes the session. Subsequent mutations are rejected.
    pub fn end(&mut self) -> Result<GuidanceSessionEndedEvent, GuidanceSessionError> {
        if self.ended_at.is_some() {
            return Err(GuidanceSessionError::AlreadyEnded);
        }
        let ended_at = time::now();
        self.ended_at = Some(ended_at);
        Ok(GuidanceSessionEndedEvent {
            session_id: self.session_id.clone(),
            suggestions_accepted: self.accepted_count,
            suggestions_dismissed: self.dismissed_count,
            artifacts_produced: self.artifacts_produced,
            timestamp: ended_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contributor::value_objects::{
        GraphSelection, GuidanceSuggestion, PartnerKind,
    };

    fn focus() -> WorkspaceFocus {
        WorkspaceFocus::new(
            Some("proj:x".into()),
            GraphSelection::default(),
            vec!["vc:x".into()],
            Vec::new(),
        )
    }

    #[test]
    fn session_lifecycle_accumulates_counts() {
        let (mut s, _) = GuidanceSession::start("ws-1", focus());
        s.accept_suggestion(SuggestionKind::CanonicalTerm, "term:1", 40).unwrap();
        s.dismiss_suggestion("term:2", None).unwrap();
        s.record_artifact_produced().unwrap();
        let envelope = NudgeEnvelope::new(
            "env-1",
            s.session_id.clone(),
            vec![
                GuidanceSuggestion::new(SuggestionKind::CanonicalTerm, "term:1", 0.9, "r", vec![]),
                GuidanceSuggestion::new(SuggestionKind::PrecedentRef, "p:1", 0.6, "r", vec![]),
                GuidanceSuggestion::new(SuggestionKind::SkillRef, "skill:1", 0.7, "r", vec![]),
            ],
        )
        .unwrap();
        s.record_nudge(envelope).unwrap();
        let evt = s.end().unwrap();
        assert_eq!(evt.suggestions_accepted, 1);
        assert_eq!(evt.suggestions_dismissed, 1);
        assert_eq!(evt.artifacts_produced, 1);
    }

    #[test]
    fn ended_session_rejects_mutations() {
        let (mut s, _) = GuidanceSession::start("ws-1", focus());
        s.end().unwrap();
        assert_eq!(
            s.accept_suggestion(SuggestionKind::PolicyHint, "p", 1),
            Err(GuidanceSessionError::AlreadyEnded)
        );
    }

    #[test]
    fn partner_scope_must_be_subset() {
        let (mut s, _) = GuidanceSession::start("ws-1", focus());
        let session_scope = vec!["read:kg".to_string()];
        let bad = PartnerBinding::new(
            PartnerKind::Ai,
            "agent-x",
            vec!["write:public".into()],
            vec![],
            None,
        );
        assert_eq!(
            s.bind_partner(bad, &session_scope),
            Err(GuidanceSessionError::PartnerScopeEscalation)
        );
        let ok = PartnerBinding::new(
            PartnerKind::Ai,
            "agent-x",
            vec!["read:kg".into()],
            vec![],
            None,
        );
        assert!(s.bind_partner(ok, &session_scope).is_ok());
    }
}
