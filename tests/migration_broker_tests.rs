// tests/migration_broker_tests.rs
//! Integration tests for ADR-049 Insight Migration Broker Workflow.
//!
//! Covers:
//!   1. Full MigrationCandidate lifecycle
//!      (candidate → under_review → approved → pr_assigned → promoted).
//!   2. Rejection path.
//!   3. Revocation path (only valid from promoted).
//!   4. BrokerCase adapter: category = "contributor_mesh_share",
//!      subject_kind = "ontology_term", confidence → priority mapping.
//!   5. ontology_propose MCP tool shape (title/body/default patch path).
//!   6. Confidence-threshold gate (< 0.60 is rejected at construction).
//!
//! These tests are unit-style and do not require Neo4j, GitHub, or an
//! OntologyActor instance. End-to-end PR creation is covered by
//! `tests/bridge_signing_fanout.rs` and live smoke tests.

use webxr::actors::ontology_actor::OntologyPropose;
use webxr::models::enterprise::{CasePriority, CaseStatus, EscalationSource};
use webxr::services::migration_broker::{
    adapt_to_broker_case, is_migration_candidate_case, meta_keys, subject_kind_of,
    MigrationCandidateAggregate, MigrationCandidateState, MigrationError,
    CATEGORY_CONTRIBUTOR_MESH_SHARE, SUBJECT_KIND_ONTOLOGY_TERM, SURFACE_CONFIDENCE_THRESHOLD,
};
use webxr::types::ontology_tools::AgentContext;

fn sample_candidate() -> MigrationCandidateAggregate {
    MigrationCandidateAggregate::new(
        "3f9a1c",
        "insight://Widget",
        "Widget",
        "https://ex.org/owl#Widget",
        0.84,
        vec!["wikilink".into(), "agent".into(), "cooccurrence".into()],
        Some("agent-alpha".into()),
        "{\"add\":[\"owl:Class\"]}",
    )
    .expect("valid candidate")
}

fn sample_ctx() -> AgentContext {
    AgentContext {
        agent_id: "agent-deadbeef".into(),
        agent_type: "migration-broker".into(),
        task_description: "promote ontology term".into(),
        session_id: Some("sess-1".into()),
        confidence: 0.9,
        user_id: "user-1".into(),
    }
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

#[test]
fn full_lifecycle_candidate_to_promoted() {
    let mut c = sample_candidate();
    assert_eq!(c.state, MigrationCandidateState::Candidate);

    c.claim().unwrap();
    assert_eq!(c.state, MigrationCandidateState::UnderReview);

    c.approve().unwrap();
    assert_eq!(c.state, MigrationCandidateState::Approved);

    c.on_pr_assigned("https://github.com/o/r/pull/142").unwrap();
    assert_eq!(c.state, MigrationCandidateState::PrAssigned);
    assert_eq!(
        c.pr_url.as_deref(),
        Some("https://github.com/o/r/pull/142")
    );

    c.on_pr_merged().unwrap();
    assert_eq!(c.state, MigrationCandidateState::Promoted);
}

#[test]
fn rejection_path_requires_reason_and_terminates() {
    let mut c = sample_candidate();
    c.claim().unwrap();

    assert_eq!(
        c.reject("").unwrap_err(),
        MigrationError::MissingRejectReason
    );

    c.reject("duplicates owl:Person").unwrap();
    assert_eq!(c.state, MigrationCandidateState::Rejected);
    assert_eq!(c.reject_reason.as_deref(), Some("duplicates owl:Person"));

    // Terminal: no transitions out of Rejected.
    assert!(matches!(
        c.claim().unwrap_err(),
        MigrationError::InvalidTransition { .. }
    ));
    assert!(matches!(
        c.approve().unwrap_err(),
        MigrationError::InvalidTransition { .. }
    ));
}

#[test]
fn reject_direct_from_candidate_state() {
    // Broker may reject without claiming first; this is the "dismiss"
    // shortcut from the inbox.
    let mut c = sample_candidate();
    c.reject("low-quality signal").unwrap();
    assert_eq!(c.state, MigrationCandidateState::Rejected);
}

#[test]
fn revocation_only_valid_from_promoted() {
    let mut c = sample_candidate();

    // Fresh candidate cannot be revoked.
    let err = c.revoke("bad class").unwrap_err();
    assert!(matches!(err, MigrationError::RevokeFromNonPromoted(_)));

    // Drive to Promoted.
    c.claim().unwrap();
    c.approve().unwrap();
    c.on_pr_assigned("https://github.com/o/r/pull/7").unwrap();
    c.on_pr_merged().unwrap();

    c.revoke("schema drift").unwrap();
    assert_eq!(c.state, MigrationCandidateState::Revoked);
    assert_eq!(c.revoke_reason.as_deref(), Some("schema drift"));
}

#[test]
fn cannot_skip_states() {
    let mut c = sample_candidate();
    assert!(matches!(
        c.approve().unwrap_err(),
        MigrationError::InvalidTransition { .. }
    ));
    assert!(matches!(
        c.on_pr_merged().unwrap_err(),
        MigrationError::InvalidTransition { .. }
    ));
    c.claim().unwrap();
    assert!(matches!(
        c.on_pr_assigned("https://x").unwrap_err(),
        MigrationError::InvalidTransition { .. }
    ));
    c.approve().unwrap();
    assert!(matches!(
        c.on_pr_merged().unwrap_err(),
        MigrationError::InvalidTransition { .. }
    ));
}

// ── Surfacing threshold ─────────────────────────────────────────────────────

#[test]
fn below_threshold_is_rejected_at_construction() {
    let err = MigrationCandidateAggregate::new(
        "x", "insight://low", "Low", "https://ex.org/owl#Low", 0.59,
        vec![], None, "{}",
    )
    .unwrap_err();
    match err {
        MigrationError::BelowThreshold(conf, thresh) => {
            assert!((conf - 0.59).abs() < 1e-9);
            assert!((thresh - SURFACE_CONFIDENCE_THRESHOLD).abs() < 1e-9);
        }
        other => panic!("expected BelowThreshold, got {:?}", other),
    }
}

#[test]
fn at_threshold_is_accepted() {
    let c = MigrationCandidateAggregate::new(
        "x",
        "insight://edge",
        "Edge",
        "https://ex.org/owl#Edge",
        SURFACE_CONFIDENCE_THRESHOLD,
        vec![],
        None,
        "{}",
    );
    assert!(c.is_ok());
}

// ── BrokerCase adapter ──────────────────────────────────────────────────────

#[test]
fn broker_case_carries_canonical_discriminators() {
    let c = sample_candidate();
    let case = adapt_to_broker_case(&c);

    assert_eq!(
        case.metadata.get(meta_keys::SUBJECT_KIND).map(String::as_str),
        Some(SUBJECT_KIND_ONTOLOGY_TERM)
    );
    assert_eq!(c.broker_category(), CATEGORY_CONTRIBUTOR_MESH_SHARE);
    assert!(is_migration_candidate_case(&case));
    assert_eq!(subject_kind_of(&case), Some(SUBJECT_KIND_ONTOLOGY_TERM));

    // Payload is preserved in metadata.
    assert_eq!(
        case.metadata.get(meta_keys::KG_NOTE_ID).unwrap(),
        "insight://Widget"
    );
    assert_eq!(
        case.metadata.get(meta_keys::ONTOLOGY_IRI).unwrap(),
        "https://ex.org/owl#Widget"
    );
    assert!(case
        .metadata
        .get(meta_keys::CONFIDENCE)
        .unwrap()
        .starts_with("0.84"));
    assert_eq!(case.status, CaseStatus::Open);
    // Candidates come from the Insight Discovery pipeline; ADR-049 §related
    // aligns them with the WorkflowProposal event family.
    assert_eq!(case.source, EscalationSource::WorkflowProposal);
    // Evidence hydrates the DecisionCanvas split-pane.
    assert_eq!(case.evidence.len(), 2);
    assert_eq!(case.evidence[0].item_type, "kg_note");
    assert_eq!(case.evidence[1].item_type, "ontology_iri");
}

#[test]
fn priority_reflects_confidence_band() {
    let mk = |conf: f64| {
        MigrationCandidateAggregate::new(
            "x", "note", "L", "iri", conf, vec![], None, "{}",
        )
        .unwrap()
        .to_broker_case()
        .priority
    };
    assert_eq!(mk(0.95), CasePriority::High);
    assert_eq!(mk(0.75), CasePriority::Medium);
    assert_eq!(mk(0.61), CasePriority::Low);
}

#[test]
fn pr_url_surfaces_in_adapted_case_metadata() {
    let mut c = sample_candidate();
    c.claim().unwrap();
    c.approve().unwrap();
    c.on_pr_assigned("https://github.com/o/r/pull/99").unwrap();
    let case = c.to_broker_case();
    assert_eq!(
        case.metadata.get(meta_keys::PR_URL).unwrap(),
        "https://github.com/o/r/pull/99"
    );
    // Approved/PrAssigned/Promoted all map to Decided.
    assert_eq!(case.status, CaseStatus::Decided);
}

#[test]
fn rejected_case_status_is_closed() {
    let mut c = sample_candidate();
    c.reject("low signal").unwrap();
    let case = c.to_broker_case();
    assert_eq!(case.status, CaseStatus::Closed);
}

#[test]
fn promoted_then_revoked_case_status_is_closed() {
    let mut c = sample_candidate();
    c.claim().unwrap();
    c.approve().unwrap();
    c.on_pr_assigned("https://x").unwrap();
    c.on_pr_merged().unwrap();
    c.revoke("rollback").unwrap();
    let case = c.to_broker_case();
    assert_eq!(case.status, CaseStatus::Closed);
}

// ── ontology_propose MCP tool shape ─────────────────────────────────────────

#[test]
fn ontology_propose_default_patch_path() {
    assert_eq!(
        OntologyPropose::default_patch_path("mc-3f9a1c"),
        "patches/migrations/mc-3f9a1c.sparql"
    );
}

#[test]
fn ontology_propose_pr_body_includes_patch_and_adr_reference() {
    let msg = OntologyPropose {
        candidate_id: "mc-42".into(),
        ontology_iri: "https://ex.org/owl#Widget".into(),
        kg_note_label: "Widget".into(),
        sparql_patch: "INSERT DATA { <https://ex.org/owl#Widget> a owl:Class }".into(),
        patch_path: None,
        agent_ctx: sample_ctx(),
    };

    let title = msg.pr_title();
    assert!(title.contains("ontology-migration"));
    assert!(title.contains("https://ex.org/owl#Widget"));

    let body = msg.pr_body();
    assert!(body.contains("mc-42"));
    assert!(body.contains("Widget"));
    assert!(body.contains("INSERT DATA"));
    assert!(body.contains("ADR-049"));
    assert!(body.contains("ADR-048 P3"));
    assert!(body.contains("agent-deadbeef"));
}

#[test]
fn ontology_propose_respects_override_patch_path() {
    let msg = OntologyPropose {
        candidate_id: "mc-77".into(),
        ontology_iri: "https://ex.org/owl#Foo".into(),
        kg_note_label: "Foo".into(),
        sparql_patch: "CLEAR ALL".into(),
        patch_path: Some("custom/path.sparql".into()),
        agent_ctx: sample_ctx(),
    };
    // Default helper is independent of the instance path override.
    assert_eq!(
        OntologyPropose::default_patch_path(&msg.candidate_id),
        "patches/migrations/mc-77.sparql"
    );
    // Override remains on the message for the handler to consume.
    assert_eq!(msg.patch_path.as_deref(), Some("custom/path.sparql"));
}

// ── Defensive checks ────────────────────────────────────────────────────────

#[test]
fn release_from_under_review_returns_to_candidate() {
    let mut c = sample_candidate();
    c.claim().unwrap();
    c.release().unwrap();
    assert_eq!(c.state, MigrationCandidateState::Candidate);
    // Re-claiming works.
    c.claim().unwrap();
    assert_eq!(c.state, MigrationCandidateState::UnderReview);
}

#[test]
fn release_not_valid_without_claim() {
    let mut c = sample_candidate();
    assert!(matches!(
        c.release().unwrap_err(),
        MigrationError::InvalidTransition { .. }
    ));
}
