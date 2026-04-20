//! MigrationCandidate broker aggregate (ADR-049).
//!
//! This module owns the `MigrationCandidate` lifecycle as a self-contained
//! aggregate that adapts to the existing `BrokerCase` aggregate (ADR-041).
//!
//! ## Responsibilities
//!
//! 1. Hold `MigrationPayload` fields (KG note id, target IRI, confidence,
//!    signal sources, OWL delta, PR url) per ADR-049 §persistence.
//! 2. Enforce the candidate-lane state machine:
//!    `candidate → promoted | rejected | revoked` — a superset of the broker
//!    workflow states, with transitions that mirror `BRIDGE_TO.kind` mutations
//!    owned by ADR-048 P3.
//! 3. Adapt a candidate to a `BrokerCase` with
//!    `category = "contributor_mesh_share"` and
//!    `subject_kind = "ontology_term"` for presentation to the Broker Inbox.
//! 4. Apply approval: caller dispatches the `ontology_propose` MCP tool and
//!    feeds the resulting PR URL back in via [`on_pr_assigned`].
//!
//! ## Relationship to ADR-048 (BRIDGE_TO)
//!
//! This module **never** writes the `BRIDGE_TO` edge. The `kind` flip from
//! `candidate` → `promoted` is owned by agent P3 and lives in
//! [`crate::services::bridge_edge::BridgeEdgeService::promote`]. This module
//! only records the aggregate-level status transition so the Broker Workbench
//! can render it; the graph-tier truth is BRIDGE_TO.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::models::enterprise::{
    BrokerCase, CasePriority, CaseStatus, EscalationSource, EvidenceItem,
};

// ── Canonical discriminators (ADR-049, Contributor Nexus swarm plan) ─────────

/// `BrokerCase.category` discriminator used by the Broker UI to route items
/// into the Contributor Mesh Share lane. Enforced by the swarm plan dated
/// 2026-04-20 and ADR-049 §persistence.
pub const CATEGORY_CONTRIBUTOR_MESH_SHARE: &str = "contributor_mesh_share";

/// `BrokerCase.metadata["subject_kind"]` discriminator. Identifies this item as
/// an ontology-term migration candidate (as opposed to e.g. a work-artifact
/// share or policy exception).
pub const SUBJECT_KIND_ONTOLOGY_TERM: &str = "ontology_term";

/// Metadata keys written by [`MigrationCandidateAggregate::to_broker_case`].
/// Mirror the ADR-049 `MigrationPayload` shape but flattened into
/// `BrokerCase.metadata: HashMap<String, String>`.
pub mod meta_keys {
    pub const SUBJECT_KIND: &str = "subject_kind";
    pub const KG_NOTE_ID: &str = "kg_note_id";
    pub const KG_NOTE_LABEL: &str = "kg_note_label";
    pub const ONTOLOGY_IRI: &str = "ontology_iri";
    pub const CONFIDENCE: &str = "confidence";
    pub const SIGNAL_SOURCES: &str = "signal_sources";
    pub const AGENT_SOURCE: &str = "agent_source";
    pub const OWL_DELTA_JSON: &str = "owl_delta_json";
    pub const PR_URL: &str = "pr_url";
    pub const DEFER_UNTIL: &str = "defer_until";
    pub const CANDIDATE_ID: &str = "candidate_id";
    pub const CANDIDATE_STATUS: &str = "candidate_status";
}

/// ADR-048 + ADR-049 confidence gate for surfacing candidates.
pub const SURFACE_CONFIDENCE_THRESHOLD: f64 = 0.60;

// ── Aggregate state ─────────────────────────────────────────────────────────

/// State of a [`MigrationCandidateAggregate`]. Maps onto `BRIDGE_TO.kind`
/// where applicable — but the edge flip itself is owned by ADR-048 P3.
///
/// Serialises as lowercase snake for compatibility with the broker JSON
/// surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationCandidateState {
    /// Discovered + surfaced; present in the broker inbox.
    /// `BRIDGE_TO.kind = "candidate"`.
    Candidate,
    /// Broker has claimed and is actively reviewing.
    UnderReview,
    /// Broker approved; awaiting PR assignment from `ontology_propose`.
    Approved,
    /// PR was opened and written back onto the aggregate.
    PrAssigned,
    /// PR merged; the BRIDGE_TO flip has been performed by ADR-048 P3.
    /// `BRIDGE_TO.kind = "promoted"`.
    Promoted,
    /// Broker rejected. `BRIDGE_TO.kind = "rejected"`.
    Rejected,
    /// Promoted candidate later revoked by a rollback case.
    /// `BRIDGE_TO.kind = "revoked"`.
    Revoked,
}

impl MigrationCandidateState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::UnderReview => "under_review",
            Self::Approved => "approved",
            Self::PrAssigned => "pr_assigned",
            Self::Promoted => "promoted",
            Self::Rejected => "rejected",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MigrationError {
    #[error("confidence {0:.3} is below surface threshold {1:.3}")]
    BelowThreshold(f64, f64),
    #[error("invalid transition: {from:?} → {to:?}")]
    InvalidTransition {
        from: MigrationCandidateState,
        to: MigrationCandidateState,
    },
    #[error("reject reason is required (non-empty)")]
    MissingRejectReason,
    #[error("revoke is only valid from Promoted (got {0:?})")]
    RevokeFromNonPromoted(MigrationCandidateState),
    #[error("ontology_iri must not be empty")]
    EmptyIri,
    #[error("kg_note_id must not be empty")]
    EmptyKgNoteId,
}

// ── Aggregate root ──────────────────────────────────────────────────────────

/// Migration candidate aggregate. Holds all ADR-049 payload fields and the
/// lifecycle state. Adapt to a `BrokerCase` via
/// [`MigrationCandidateAggregate::to_broker_case`] for broker-inbox rendering.
///
/// Construction enforces the surfacing invariant: callers must supply a
/// confidence ≥ [`SURFACE_CONFIDENCE_THRESHOLD`]. Candidates below threshold
/// remain in the discovery engine and do not become broker cases (ADR-049
/// §scoring-and-suppression).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationCandidateAggregate {
    pub id: String,
    pub kg_note_id: String,
    pub kg_note_label: String,
    pub ontology_iri: String,
    pub confidence: f64,
    pub signal_sources: Vec<String>,
    pub agent_source: Option<String>,
    pub owl_delta_json: String,
    pub pr_url: Option<String>,
    pub defer_until: Option<DateTime<Utc>>,
    pub reject_reason: Option<String>,
    pub revoke_reason: Option<String>,
    pub state: MigrationCandidateState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MigrationCandidateAggregate {
    /// Construct a new candidate. Caller must ensure the confidence is
    /// ≥ [`SURFACE_CONFIDENCE_THRESHOLD`] — mirrors ADR-049 §scoring.
    pub fn new(
        id: impl Into<String>,
        kg_note_id: impl Into<String>,
        kg_note_label: impl Into<String>,
        ontology_iri: impl Into<String>,
        confidence: f64,
        signal_sources: Vec<String>,
        agent_source: Option<String>,
        owl_delta_json: impl Into<String>,
    ) -> Result<Self, MigrationError> {
        let kg_note_id = kg_note_id.into();
        let ontology_iri = ontology_iri.into();
        if kg_note_id.is_empty() {
            return Err(MigrationError::EmptyKgNoteId);
        }
        if ontology_iri.is_empty() {
            return Err(MigrationError::EmptyIri);
        }
        if confidence < SURFACE_CONFIDENCE_THRESHOLD {
            return Err(MigrationError::BelowThreshold(
                confidence,
                SURFACE_CONFIDENCE_THRESHOLD,
            ));
        }

        let now = Utc::now();
        Ok(Self {
            id: id.into(),
            kg_note_id,
            kg_note_label: kg_note_label.into(),
            ontology_iri,
            confidence,
            signal_sources,
            agent_source,
            owl_delta_json: owl_delta_json.into(),
            pr_url: None,
            defer_until: None,
            reject_reason: None,
            revoke_reason: None,
            state: MigrationCandidateState::Candidate,
            created_at: now,
            updated_at: now,
        })
    }

    // ── Transitions ─────────────────────────────────────────────────────────

    /// Broker claims the candidate: `Candidate → UnderReview`.
    pub fn claim(&mut self) -> Result<(), MigrationError> {
        self.assert_transition(
            self.state,
            MigrationCandidateState::UnderReview,
            matches!(self.state, MigrationCandidateState::Candidate),
        )?;
        self.state = MigrationCandidateState::UnderReview;
        self.touch();
        Ok(())
    }

    /// Broker releases a claimed candidate back to the inbox:
    /// `UnderReview → Candidate`.
    pub fn release(&mut self) -> Result<(), MigrationError> {
        self.assert_transition(
            self.state,
            MigrationCandidateState::Candidate,
            matches!(self.state, MigrationCandidateState::UnderReview),
        )?;
        self.state = MigrationCandidateState::Candidate;
        self.touch();
        Ok(())
    }

    /// Broker approves: `UnderReview → Approved`. After this the caller must
    /// invoke `ontology_propose` and feed the PR URL back via
    /// [`on_pr_assigned`].
    pub fn approve(&mut self) -> Result<(), MigrationError> {
        self.assert_transition(
            self.state,
            MigrationCandidateState::Approved,
            matches!(self.state, MigrationCandidateState::UnderReview),
        )?;
        self.state = MigrationCandidateState::Approved;
        self.touch();
        Ok(())
    }

    /// PR was opened. `Approved → PrAssigned`.
    pub fn on_pr_assigned(&mut self, pr_url: impl Into<String>) -> Result<(), MigrationError> {
        self.assert_transition(
            self.state,
            MigrationCandidateState::PrAssigned,
            matches!(self.state, MigrationCandidateState::Approved),
        )?;
        self.pr_url = Some(pr_url.into());
        self.state = MigrationCandidateState::PrAssigned;
        self.touch();
        Ok(())
    }

    /// GitHub merged the PR. `PrAssigned → Promoted`. The `BRIDGE_TO.kind`
    /// flip is performed by ADR-048 P3 in response to the same event — this
    /// aggregate only records the broker-surface state.
    pub fn on_pr_merged(&mut self) -> Result<(), MigrationError> {
        self.assert_transition(
            self.state,
            MigrationCandidateState::Promoted,
            matches!(self.state, MigrationCandidateState::PrAssigned),
        )?;
        self.state = MigrationCandidateState::Promoted;
        self.touch();
        Ok(())
    }

    /// Broker rejects. Reason must be non-empty (ADR-049 §api-surface).
    pub fn reject(&mut self, reason: impl Into<String>) -> Result<(), MigrationError> {
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err(MigrationError::MissingRejectReason);
        }
        self.assert_transition(
            self.state,
            MigrationCandidateState::Rejected,
            matches!(
                self.state,
                MigrationCandidateState::Candidate | MigrationCandidateState::UnderReview
            ),
        )?;
        self.reject_reason = Some(reason);
        self.state = MigrationCandidateState::Rejected;
        self.touch();
        Ok(())
    }

    /// Rollback a promoted candidate. Valid only from Promoted.
    /// `Promoted → Revoked`.
    pub fn revoke(&mut self, reason: impl Into<String>) -> Result<(), MigrationError> {
        if !matches!(self.state, MigrationCandidateState::Promoted) {
            return Err(MigrationError::RevokeFromNonPromoted(self.state));
        }
        self.revoke_reason = Some(reason.into());
        self.state = MigrationCandidateState::Revoked;
        self.touch();
        Ok(())
    }

    // ── Adaptation ──────────────────────────────────────────────────────────

    /// Synthesise a `BrokerCase` with
    /// `category = "contributor_mesh_share"` and
    /// `metadata["subject_kind"] = "ontology_term"` per the swarm plan
    /// canonicalisation rules.
    ///
    /// The `BrokerCase.status` is mapped from the aggregate state:
    /// - `Candidate` → `Open`
    /// - `UnderReview` → `InReview`
    /// - `Approved` / `PrAssigned` / `Promoted` → `Decided`
    /// - `Rejected` / `Revoked` → `Closed`
    pub fn to_broker_case(&self) -> BrokerCase {
        let status = match self.state {
            MigrationCandidateState::Candidate => CaseStatus::Open,
            MigrationCandidateState::UnderReview => CaseStatus::InReview,
            MigrationCandidateState::Approved
            | MigrationCandidateState::PrAssigned
            | MigrationCandidateState::Promoted => CaseStatus::Decided,
            MigrationCandidateState::Rejected | MigrationCandidateState::Revoked => {
                CaseStatus::Closed
            }
        };

        let priority = if self.confidence >= 0.85 {
            CasePriority::High
        } else if self.confidence >= 0.70 {
            CasePriority::Medium
        } else {
            CasePriority::Low
        };

        let mut metadata: HashMap<String, String> = HashMap::new();
        metadata.insert(
            meta_keys::SUBJECT_KIND.to_string(),
            SUBJECT_KIND_ONTOLOGY_TERM.to_string(),
        );
        metadata.insert(meta_keys::CANDIDATE_ID.to_string(), self.id.clone());
        metadata.insert(
            meta_keys::CANDIDATE_STATUS.to_string(),
            self.state.as_str().to_string(),
        );
        metadata.insert(meta_keys::KG_NOTE_ID.to_string(), self.kg_note_id.clone());
        metadata.insert(
            meta_keys::KG_NOTE_LABEL.to_string(),
            self.kg_note_label.clone(),
        );
        metadata.insert(
            meta_keys::ONTOLOGY_IRI.to_string(),
            self.ontology_iri.clone(),
        );
        metadata.insert(
            meta_keys::CONFIDENCE.to_string(),
            format!("{:.4}", self.confidence),
        );
        metadata.insert(
            meta_keys::SIGNAL_SOURCES.to_string(),
            self.signal_sources.join(","),
        );
        if let Some(agent) = &self.agent_source {
            metadata.insert(meta_keys::AGENT_SOURCE.to_string(), agent.clone());
        }
        metadata.insert(
            meta_keys::OWL_DELTA_JSON.to_string(),
            self.owl_delta_json.clone(),
        );
        if let Some(pr) = &self.pr_url {
            metadata.insert(meta_keys::PR_URL.to_string(), pr.clone());
        }
        if let Some(defer) = self.defer_until {
            metadata.insert(meta_keys::DEFER_UNTIL.to_string(), defer.to_rfc3339());
        }

        let title = format!(
            "Promote `{}` → `{}`",
            self.kg_note_label, self.ontology_iri
        );
        let description = format!(
            "Migration candidate (confidence {:.2}). Signals: {}. Agent: {}.",
            self.confidence,
            if self.signal_sources.is_empty() {
                "n/a".to_string()
            } else {
                self.signal_sources.join(", ")
            },
            self.agent_source.as_deref().unwrap_or("system")
        );

        // Carry KG note id + ontology IRI as evidence so the workbench can
        // hydrate the split-pane DecisionCanvas.
        let evidence = vec![
            EvidenceItem {
                item_type: "kg_note".to_string(),
                source_id: self.kg_note_id.clone(),
                description: self.kg_note_label.clone(),
                timestamp: self.created_at.to_rfc3339(),
            },
            EvidenceItem {
                item_type: "ontology_iri".to_string(),
                source_id: self.ontology_iri.clone(),
                description: "Proposed OWL class IRI".to_string(),
                timestamp: self.created_at.to_rfc3339(),
            },
        ];

        BrokerCase {
            id: format!("mc-{}", self.id),
            title,
            description,
            priority,
            // Per ADR-049: migration candidates are surfaced from Insight
            // Discovery, which reports via the WorkflowProposal event bus.
            source: EscalationSource::WorkflowProposal,
            status,
            created_at: self.created_at.to_rfc3339(),
            updated_at: self.updated_at.to_rfc3339(),
            assigned_to: None,
            evidence,
            metadata,
        }
    }

    /// True if the aggregate's derived `BrokerCase` should be routed to the
    /// Contributor Mesh Share lane (i.e. always — this is how we declare it).
    /// Kept as a helper so upstream callers can switch on the metadata without
    /// hard-coding the discriminator string.
    pub fn broker_category(&self) -> &'static str {
        CATEGORY_CONTRIBUTOR_MESH_SHARE
    }

    pub fn subject_kind(&self) -> &'static str {
        SUBJECT_KIND_ONTOLOGY_TERM
    }

    // ── Internals ───────────────────────────────────────────────────────────

    fn assert_transition(
        &self,
        from: MigrationCandidateState,
        to: MigrationCandidateState,
        is_valid: bool,
    ) -> Result<(), MigrationError> {
        if is_valid {
            Ok(())
        } else {
            Err(MigrationError::InvalidTransition { from, to })
        }
    }

    fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

// ── BrokerCase adapter ──────────────────────────────────────────────────────

/// Pure function wrapper around `to_broker_case`. Lets handlers talk in
/// closure terms without pulling in the full aggregate type parameter.
pub fn adapt_to_broker_case(agg: &MigrationCandidateAggregate) -> BrokerCase {
    agg.to_broker_case()
}

/// Helper: extract the subject_kind from a `BrokerCase`'s metadata, or `None`
/// if not set. Supports the broker handler in routing incoming requests.
pub fn subject_kind_of(case: &BrokerCase) -> Option<&str> {
    case.metadata
        .get(meta_keys::SUBJECT_KIND)
        .map(String::as_str)
}

/// Helper: true if the case belongs to the Contributor Mesh Share lane carrying
/// an ontology-term migration candidate.
pub fn is_migration_candidate_case(case: &BrokerCase) -> bool {
    // BrokerCase has no `category` field today (ADR-041 base aggregate E3 owns
    // that). We key purely on metadata.subject_kind which is canonical per
    // the swarm plan and preserves round-trip fidelity.
    subject_kind_of(case) == Some(SUBJECT_KIND_ONTOLOGY_TERM)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn new_candidate() -> MigrationCandidateAggregate {
        MigrationCandidateAggregate::new(
            "3f9a1c",
            "insight://Foo",
            "Foo".to_string(),
            "https://ex.org/owl#Foo",
            0.84,
            vec!["wikilink".into(), "agent".into()],
            Some("agent-alpha".into()),
            "{\"add\":[\"owl:Class\"]}",
        )
        .expect("valid candidate")
    }

    #[test]
    fn rejects_below_threshold() {
        let err = MigrationCandidateAggregate::new(
            "x",
            "insight://bar",
            "Bar",
            "https://ex.org/owl#Bar",
            0.55,
            vec![],
            None,
            "{}",
        )
        .unwrap_err();
        assert!(matches!(err, MigrationError::BelowThreshold(_, _)));
    }

    #[test]
    fn requires_non_empty_iri_and_note_id() {
        let err = MigrationCandidateAggregate::new("x", "", "L", "iri", 0.7, vec![], None, "{}")
            .unwrap_err();
        assert_eq!(err, MigrationError::EmptyKgNoteId);
        let err =
            MigrationCandidateAggregate::new("x", "note", "L", "", 0.7, vec![], None, "{}")
                .unwrap_err();
        assert_eq!(err, MigrationError::EmptyIri);
    }

    #[test]
    fn starts_in_candidate_state() {
        let c = new_candidate();
        assert_eq!(c.state, MigrationCandidateState::Candidate);
    }

    #[test]
    fn happy_path_lifecycle() {
        let mut c = new_candidate();
        c.claim().unwrap();
        assert_eq!(c.state, MigrationCandidateState::UnderReview);
        c.approve().unwrap();
        assert_eq!(c.state, MigrationCandidateState::Approved);
        c.on_pr_assigned("https://github.com/o/r/pull/42").unwrap();
        assert_eq!(c.state, MigrationCandidateState::PrAssigned);
        assert_eq!(c.pr_url.as_deref(), Some("https://github.com/o/r/pull/42"));
        c.on_pr_merged().unwrap();
        assert_eq!(c.state, MigrationCandidateState::Promoted);
    }

    #[test]
    fn release_returns_to_candidate() {
        let mut c = new_candidate();
        c.claim().unwrap();
        c.release().unwrap();
        assert_eq!(c.state, MigrationCandidateState::Candidate);
    }

    #[test]
    fn reject_requires_reason_and_blocks_post_review() {
        let mut c = new_candidate();
        c.claim().unwrap();
        assert_eq!(
            c.reject("").unwrap_err(),
            MigrationError::MissingRejectReason
        );
        c.reject("duplicates owl:Person").unwrap();
        assert_eq!(c.state, MigrationCandidateState::Rejected);
        // No transitions out of Rejected.
        let err = c.claim().unwrap_err();
        assert!(matches!(err, MigrationError::InvalidTransition { .. }));
    }

    #[test]
    fn cannot_approve_without_claim() {
        let mut c = new_candidate();
        let err = c.approve().unwrap_err();
        assert!(matches!(err, MigrationError::InvalidTransition { .. }));
    }

    #[test]
    fn cannot_pr_assign_before_approve() {
        let mut c = new_candidate();
        c.claim().unwrap();
        let err = c.on_pr_assigned("https://x").unwrap_err();
        assert!(matches!(err, MigrationError::InvalidTransition { .. }));
    }

    #[test]
    fn cannot_merge_before_pr_assigned() {
        let mut c = new_candidate();
        c.claim().unwrap();
        c.approve().unwrap();
        let err = c.on_pr_merged().unwrap_err();
        assert!(matches!(err, MigrationError::InvalidTransition { .. }));
    }

    #[test]
    fn revoke_only_valid_from_promoted() {
        let mut c = new_candidate();
        // Fresh candidate cannot be revoked.
        assert!(matches!(
            c.revoke("bad class").unwrap_err(),
            MigrationError::RevokeFromNonPromoted(MigrationCandidateState::Candidate)
        ));

        c.claim().unwrap();
        c.approve().unwrap();
        c.on_pr_assigned("https://x").unwrap();
        c.on_pr_merged().unwrap();
        c.revoke("deprecated").unwrap();
        assert_eq!(c.state, MigrationCandidateState::Revoked);
        assert_eq!(c.revoke_reason.as_deref(), Some("deprecated"));
    }

    #[test]
    fn broker_case_has_correct_category_and_subject_kind() {
        let c = new_candidate();
        let case = c.to_broker_case();
        assert_eq!(
            case.metadata.get(meta_keys::SUBJECT_KIND).map(String::as_str),
            Some(SUBJECT_KIND_ONTOLOGY_TERM)
        );
        assert_eq!(c.broker_category(), CATEGORY_CONTRIBUTOR_MESH_SHARE);
        assert_eq!(c.subject_kind(), SUBJECT_KIND_ONTOLOGY_TERM);
        assert!(is_migration_candidate_case(&case));
        assert_eq!(case.status, CaseStatus::Open);
        assert_eq!(
            case.metadata.get(meta_keys::KG_NOTE_ID).unwrap(),
            "insight://Foo"
        );
        assert_eq!(
            case.metadata.get(meta_keys::ONTOLOGY_IRI).unwrap(),
            "https://ex.org/owl#Foo"
        );
        assert!(case
            .metadata
            .get(meta_keys::CONFIDENCE)
            .unwrap()
            .starts_with("0.84"));
        assert_eq!(case.evidence.len(), 2);
    }

    #[test]
    fn priority_mapping_uses_confidence_bands() {
        let make = |conf: f64| {
            MigrationCandidateAggregate::new(
                "x", "note", "L", "iri", conf, vec![], None, "{}",
            )
            .unwrap()
            .to_broker_case()
            .priority
        };
        assert_eq!(make(0.90), CasePriority::High);
        assert_eq!(make(0.75), CasePriority::Medium);
        assert_eq!(make(0.60), CasePriority::Low);
    }

    #[test]
    fn broker_case_status_reflects_aggregate_state() {
        let mut c = new_candidate();
        assert_eq!(c.to_broker_case().status, CaseStatus::Open);
        c.claim().unwrap();
        assert_eq!(c.to_broker_case().status, CaseStatus::InReview);
        c.approve().unwrap();
        assert_eq!(c.to_broker_case().status, CaseStatus::Decided);
        c.on_pr_assigned("https://x").unwrap();
        assert_eq!(c.to_broker_case().status, CaseStatus::Decided);
        c.on_pr_merged().unwrap();
        assert_eq!(c.to_broker_case().status, CaseStatus::Decided);
    }

    #[test]
    fn pr_url_surfaces_in_metadata() {
        let mut c = new_candidate();
        c.claim().unwrap();
        c.approve().unwrap();
        c.on_pr_assigned("https://github.com/o/r/pull/1").unwrap();
        let case = c.to_broker_case();
        assert_eq!(
            case.metadata.get(meta_keys::PR_URL).unwrap(),
            "https://github.com/o/r/pull/1"
        );
    }

    #[test]
    fn subject_kind_helper_round_trips() {
        let c = new_candidate();
        let case = c.to_broker_case();
        assert_eq!(subject_kind_of(&case), Some(SUBJECT_KIND_ONTOLOGY_TERM));
    }

    #[test]
    fn non_migration_case_is_not_flagged() {
        let case = BrokerCase {
            id: "case-1".into(),
            title: "t".into(),
            description: "d".into(),
            priority: CasePriority::Low,
            source: EscalationSource::ManualSubmission,
            status: CaseStatus::Open,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            assigned_to: None,
            evidence: vec![],
            metadata: HashMap::new(),
        };
        assert!(!is_migration_candidate_case(&case));
        assert_eq!(subject_kind_of(&case), None);
    }
}
