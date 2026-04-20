//! `OntologyGuidanceActor` — the "Sensei" (NudgeComposer stub).
//!
//! Produces a three-suggestion [`NudgeEnvelope`] for a given
//! [`WorkspaceFocus`]. The real nudge composition will consume BC7 axioms,
//! BC13 precedents, and BC19 skill recommendations; this stub returns a
//! deterministic placeholder trio so the supervision tree and the
//! `GuidanceSession` invariants can be exercised end-to-end.
//!
//! Per ADR-057 and DDD §BC18 value objects, Sensei nudges MUST be bundles of
//! exactly three suggestions (`NudgeEnvelope::new` enforces this).
//!
//! Follow-up wiring (not in this commit):
//! - replace stub composer with real OntologyGuidanceService that queries
//!   `ontology_discover` / `ontology_read` / `ontology_traverse` MCP tools
//! - plug the `sensei_nudge` MCP tool (agent X1) into this actor
//! - apply per-workspace nudge-rate limiting per ADR-057 §Consequences mitigation
//! - feed dismiss signals back to a per-contributor relevance model

use actix::prelude::*;
use log::{debug, info};
use uuid::Uuid;

use crate::domain::contributor::{
    GuidanceSuggestion, NudgeEnvelope, SuggestionKind, WorkspaceFocus,
};

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Message)]
#[rtype(result = "Result<NudgeEnvelope, String>")]
pub struct ComposeNudge {
    pub session_id: String,
    pub focus: WorkspaceFocus,
}

impl ComposeNudge {
    pub fn new(session_id: impl Into<String>, focus: WorkspaceFocus) -> Self {
        Self {
            session_id: session_id.into(),
            focus,
        }
    }
}

// ---------------------------------------------------------------------------
// Actor
// ---------------------------------------------------------------------------

/// Stubbed Sensei. Deterministic three-suggestion output from a focus token.
pub struct OntologyGuidanceActor {
    /// Per-workspace nudge counter — future rate-limiting hook.
    nudge_count: u64,
}

impl Default for OntologyGuidanceActor {
    fn default() -> Self {
        Self::new()
    }
}

impl OntologyGuidanceActor {
    pub fn new() -> Self {
        Self { nudge_count: 0 }
    }

    /// NudgeComposer stub. Given a [`WorkspaceFocus`], produce a
    /// [`NudgeEnvelope`] with exactly three suggestions of distinct kinds.
    fn compose(&self, session_id: &str, focus: &WorkspaceFocus) -> Result<NudgeEnvelope, String> {
        let token = focus.focus_token();
        let suggestions = vec![
            GuidanceSuggestion::new(
                SuggestionKind::CanonicalTerm,
                focus
                    .ontology_context
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "vc:Unknown".into()),
                0.6,
                "stub: closest canonical-term by focus token",
                vec![format!("focus_token:{}", token)],
            ),
            GuidanceSuggestion::new(
                SuggestionKind::PrecedentRef,
                focus
                    .recent_episodes
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "episode:none".into()),
                0.4,
                "stub: most recent episode from BC30",
                vec!["source:episodic".into()],
            ),
            GuidanceSuggestion::new(
                SuggestionKind::SkillRef,
                "skill:stub-summarise",
                0.5,
                "stub: placeholder BC19 skill recommendation",
                vec!["source:bc19".into()],
            ),
        ];
        NudgeEnvelope::new(Uuid::new_v4().to_string(), session_id, suggestions)
    }
}

impl Actor for OntologyGuidanceActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("[OntologyGuidanceActor] started (NudgeComposer stub)");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!(
            "[OntologyGuidanceActor] stopped after {} nudges composed",
            self.nudge_count
        );
    }
}

impl Handler<ComposeNudge> for OntologyGuidanceActor {
    type Result = Result<NudgeEnvelope, String>;

    fn handle(&mut self, msg: ComposeNudge, _ctx: &mut Self::Context) -> Self::Result {
        self.nudge_count += 1;
        debug!(
            "[OntologyGuidanceActor] composing nudge session={} nudge_count={}",
            msg.session_id, self.nudge_count
        );
        self.compose(&msg.session_id, &msg.focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contributor::GraphSelection;

    #[actix::test]
    async fn compose_nudge_emits_exactly_three_suggestions() {
        let actor = OntologyGuidanceActor::new().start();
        let focus = WorkspaceFocus::new(
            Some("proj:x".into()),
            GraphSelection {
                node_ids: vec!["n1".into()],
                ..Default::default()
            },
            vec!["vc:Negentropy".into()],
            vec!["ep:1".into()],
        );
        let env = actor
            .send(ComposeNudge::new("sess-1", focus))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(env.suggestions.len(), 3);
        assert_eq!(env.session_id, "sess-1");
        assert!(env.dismissable);
    }
}
