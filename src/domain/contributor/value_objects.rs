//! BC18 Contributor Enablement — value objects.
//!
//! All types here are immutable by convention. Mutation of aggregate state goes
//! through methods on the aggregate root; value objects are replaced, not edited.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::utils::time;

// ---------------------------------------------------------------------------
// ShareState — the three legitimate visibility levels.
// ---------------------------------------------------------------------------

/// Monotonic three-level share state for a [`crate::domain::contributor::WorkArtifact`].
///
/// Per ADR-057 §Share-State Transition Rules and DDD §BC18 invariant 2, forward
/// transitions are the only legal moves. Downward motion requires an explicit
/// [`ContributorRevocation`](crate::domain::contributor::ShareIntent) carrying a
/// rationale and emitting `ShareIntentRevoked`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ShareState {
    Private,
    Team,
    Mesh,
}

impl ShareState {
    /// Ordinal rank, used to enforce monotonic progression.
    pub fn rank(self) -> u8 {
        match self {
            ShareState::Private => 0,
            ShareState::Team => 1,
            ShareState::Mesh => 2,
        }
    }

    /// Returns `true` iff `to` is the next state in the allowed sequence.
    /// Skipping (Private → Mesh) is forbidden per ADR-057 invariant
    /// "A ShareIntent cannot skip a state".
    pub fn is_next(self, to: ShareState) -> bool {
        to.rank() == self.rank() + 1
    }

    /// Returns `true` iff `to` is strictly greater than `self`.
    /// Used as a weaker check when both adjacent and skip-through transitions
    /// are validated by a higher-level orchestrator.
    pub fn is_forward(self, to: ShareState) -> bool {
        to.rank() > self.rank()
    }
}

impl std::fmt::Display for ShareState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShareState::Private => f.write_str("Private"),
            ShareState::Team => f.write_str("Team"),
            ShareState::Mesh => f.write_str("Mesh"),
        }
    }
}

// ---------------------------------------------------------------------------
// WorkspaceFocus — immutable focus snapshot per GuidanceSession.
// ---------------------------------------------------------------------------

/// Graph-selection subset embedded in a [`WorkspaceFocus`]. Opaque node IDs;
/// identity types live in BC2 and must not leak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GraphSelection {
    pub node_ids: Vec<String>,
    pub edge_ids: Vec<String>,
}

impl GraphSelection {
    pub fn is_empty(&self) -> bool {
        self.node_ids.is_empty() && self.edge_ids.is_empty()
    }
}

/// Immutable snapshot of the contributor's workspace focus. Composed by
/// [`crate::domain::contributor::ContextAssemblyService`] from pod + graph +
/// ontology + episodic sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFocus {
    pub project_ref: Option<String>,
    pub graph_selection: GraphSelection,
    pub ontology_context: Vec<String>,
    pub recent_episodes: Vec<String>,
    pub captured_at: DateTime<Utc>,
}

impl WorkspaceFocus {
    pub fn new(
        project_ref: Option<String>,
        graph_selection: GraphSelection,
        ontology_context: Vec<String>,
        recent_episodes: Vec<String>,
    ) -> Self {
        Self {
            project_ref,
            graph_selection,
            ontology_context,
            recent_episodes,
            captured_at: time::now(),
        }
    }

    /// Stable hash-token for the focus; used by `OntologyGuidanceActor` to
    /// decide whether to recompute a nudge. Not cryptographically strong.
    pub fn focus_token(&self) -> String {
        let mut s = String::new();
        if let Some(p) = &self.project_ref {
            s.push_str(p);
        }
        s.push('|');
        for n in &self.graph_selection.node_ids {
            s.push_str(n);
            s.push(',');
        }
        s.push('|');
        for o in &self.ontology_context {
            s.push_str(o);
            s.push(',');
        }
        s
    }
}

// ---------------------------------------------------------------------------
// GuidanceSuggestion — a single Sensei suggestion.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    CanonicalTerm,
    PrecedentRef,
    SkillRef,
    PolicyHint,
}

/// An individual suggestion emitted by the Sensei. Three of these are bundled
/// into a [`NudgeEnvelope`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidanceSuggestion {
    pub kind: SuggestionKind,
    pub payload: String,
    pub confidence: f32,
    pub rationale: String,
    pub source_signals: Vec<String>,
}

impl GuidanceSuggestion {
    /// Confidence is always clamped to `[0.0, 1.0]` — enforcing the invariant
    /// from DDD §BC18 value objects.
    pub fn new(
        kind: SuggestionKind,
        payload: impl Into<String>,
        confidence: f32,
        rationale: impl Into<String>,
        source_signals: Vec<String>,
    ) -> Self {
        Self {
            kind,
            payload: payload.into(),
            confidence: confidence.clamp(0.0, 1.0),
            rationale: rationale.into(),
            source_signals,
        }
    }
}

// ---------------------------------------------------------------------------
// NudgeEnvelope — the three-suggestion Sensei delivery unit.
// ---------------------------------------------------------------------------

/// Sensei emits nudges as envelopes of exactly three suggestions (ADR-057
/// decision driver "Proactive, not reactive" + DDD §BC18 value-object entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NudgeEnvelope {
    pub envelope_id: String,
    pub session_id: String,
    pub suggestions: Vec<GuidanceSuggestion>,
    pub composed_at: DateTime<Utc>,
    pub dismissable: bool,
}

impl NudgeEnvelope {
    /// Returns `Err` if `suggestions.len() != 3`. The envelope is the place
    /// the invariant is enforced so downstream actors can treat it as a given.
    pub fn new(
        envelope_id: impl Into<String>,
        session_id: impl Into<String>,
        suggestions: Vec<GuidanceSuggestion>,
    ) -> Result<Self, String> {
        if suggestions.len() != 3 {
            return Err(format!(
                "NudgeEnvelope requires exactly 3 suggestions (got {})",
                suggestions.len()
            ));
        }
        Ok(Self {
            envelope_id: envelope_id.into(),
            session_id: session_id.into(),
            suggestions,
            composed_at: time::now(),
            dismissable: true,
        })
    }
}

// ---------------------------------------------------------------------------
// PartnerBinding — session-scoped delegation.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartnerKind {
    Ai,
    Human,
    Automation,
}

/// A session-bounded delegation attaching a partner to a [`GuidanceSession`].
/// The delegation scope is a strict subset of the contributor's session scope
/// (DDD §BC18 invariant 7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartnerBinding {
    pub partner_kind: PartnerKind,
    pub partner_id: String,
    pub delegation_scope: Vec<String>,
    pub permissions: Vec<String>,
    pub bound_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl PartnerBinding {
    pub fn new(
        partner_kind: PartnerKind,
        partner_id: impl Into<String>,
        delegation_scope: Vec<String>,
        permissions: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            partner_kind,
            partner_id: partner_id.into(),
            delegation_scope,
            permissions,
            bound_at: time::now(),
            expires_at,
        }
    }

    /// Strict-subset check against a contributor session scope.
    pub fn is_strict_subset_of(&self, session_scope: &[String]) -> bool {
        self.delegation_scope
            .iter()
            .all(|s| session_scope.iter().any(|ss| ss == s))
    }
}

// ---------------------------------------------------------------------------
// ArtifactRef + ArtifactKind + lineage chain.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Note,
    Snippet,
    DraftSkill,
    DraftGraphView,
    DraftProposal,
}

/// Reference to a [`crate::domain::contributor::WorkArtifact`] in lineage
/// chains and downstream payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub artifact_id: String,
    pub pod_uri: String,
}

impl ArtifactRef {
    pub fn new(artifact_id: impl Into<String>, pod_uri: impl Into<String>) -> Self {
        Self {
            artifact_id: artifact_id.into(),
            pod_uri: pod_uri.into(),
        }
    }
}

/// Append-only chain of transition ids recording every share-state movement
/// an artifact has made.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactLineage {
    pub entries: Vec<LineageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEntry {
    pub intent_id: String,
    pub from_state: ShareState,
    pub to_state: ShareState,
    pub downstream_ref: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

impl ArtifactLineage {
    pub fn push(
        &mut self,
        intent_id: impl Into<String>,
        from_state: ShareState,
        to_state: ShareState,
        downstream_ref: Option<String>,
    ) {
        self.entries.push(LineageEntry {
            intent_id: intent_id.into(),
            from_state,
            to_state,
            downstream_ref,
            recorded_at: time::now(),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_state_is_monotonic() {
        assert!(ShareState::Private.is_next(ShareState::Team));
        assert!(ShareState::Team.is_next(ShareState::Mesh));
        assert!(!ShareState::Private.is_next(ShareState::Mesh)); // skip forbidden
        assert!(!ShareState::Mesh.is_next(ShareState::Team)); // backward forbidden
    }

    #[test]
    fn share_state_rank_ordering() {
        assert!(ShareState::Private.rank() < ShareState::Team.rank());
        assert!(ShareState::Team.rank() < ShareState::Mesh.rank());
        assert!(ShareState::Private.is_forward(ShareState::Mesh));
        assert!(!ShareState::Mesh.is_forward(ShareState::Private));
    }

    #[test]
    fn nudge_envelope_requires_three_suggestions() {
        let one = vec![GuidanceSuggestion::new(
            SuggestionKind::CanonicalTerm,
            "vc:x",
            0.9,
            "r",
            vec![],
        )];
        assert!(NudgeEnvelope::new("e1", "s1", one).is_err());
    }

    #[test]
    fn partner_binding_strict_subset() {
        let session = vec!["read:kg".to_string(), "write:private".to_string()];
        let ok = PartnerBinding::new(
            PartnerKind::Ai,
            "agent-1",
            vec!["read:kg".into()],
            vec![],
            None,
        );
        assert!(ok.is_strict_subset_of(&session));

        let bad = PartnerBinding::new(
            PartnerKind::Ai,
            "agent-2",
            vec!["write:public".into()],
            vec![],
            None,
        );
        assert!(!bad.is_strict_subset_of(&session));
    }

    #[test]
    fn guidance_suggestion_clamps_confidence() {
        let s = GuidanceSuggestion::new(SuggestionKind::SkillRef, "skill:x", 2.0, "r", vec![]);
        assert_eq!(s.confidence, 1.0);
        let s = GuidanceSuggestion::new(SuggestionKind::SkillRef, "skill:x", -1.0, "r", vec![]);
        assert_eq!(s.confidence, 0.0);
    }
}
