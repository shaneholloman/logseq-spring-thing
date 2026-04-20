//! Integration tests for the BC11 Broker Workbench domain (ADR-041/042).
//!
//! These tests exercise the aggregate + orchestrator in process, without
//! standing up Actix or Neo4j. The BrokerActor has its own in-crate unit
//! test module covering the actor message contract.

use webxr::domain::broker::{
    BrokerCase, CaseCategory, CaseState, DecisionOrchestrator, DecisionOutcome,
    ShareIntentBrokerAdapter, ShareState, SubjectKind, SubjectRef,
};

fn fresh_case(author: &str) -> BrokerCase {
    BrokerCase::new(
        "case-1",
        CaseCategory::ContributorMeshShare,
        SubjectRef {
            kind: SubjectKind::WorkArtifact,
            id: "art-1".into(),
            from_state: Some(ShareState::Private),
            to_state: Some(ShareState::Team),
        },
        "Promote",
        "Summary",
        author,
        50,
    )
}

// ---------------------------------------------------------------------------
// 1. Approval flow
// ---------------------------------------------------------------------------

#[test]
fn approval_flow_end_to_end() {
    let orch = DecisionOrchestrator::new();
    let mut case = fresh_case("alice");
    case.claim("bob").unwrap();
    let report = orch
        .decide(
            &mut case,
            "dec-1",
            DecisionOutcome::Approve,
            "bob",
            "looks good",
        )
        .expect("approve ok");
    assert_eq!(case.state, CaseState::Decided);
    assert_eq!(case.history.len(), 1);
    assert_eq!(report.entry.decision_id, "dec-1");
    let plan = report
        .share_plan
        .expect("contributor_mesh_share approve emits plan");
    assert_eq!(plan.from, ShareState::Private);
    assert_eq!(plan.to, ShareState::Team);
    assert_eq!(plan.approved_by, "bob");
}

#[test]
fn share_intent_adapter_round_trip() {
    let case = ShareIntentBrokerAdapter::case_from_intent(
        "case-42",
        "artifact-99",
        ShareState::Private,
        ShareState::Mesh,
        "alice",
        "Publish widget",
        "Move from private to public mesh",
    );
    assert_eq!(case.category, CaseCategory::ContributorMeshShare);
    assert_eq!(case.subject.kind, SubjectKind::WorkArtifact);
    assert_eq!(case.subject.from_state, Some(ShareState::Private));
    assert_eq!(case.subject.to_state, Some(ShareState::Mesh));
}

// ---------------------------------------------------------------------------
// 2. Self-review invariant
// ---------------------------------------------------------------------------

#[test]
fn self_review_rejected_on_claim() {
    let mut case = fresh_case("alice");
    let err = case.claim("alice").unwrap_err();
    assert!(format!("{err}").contains("self-review"));
}

#[test]
fn self_review_rejected_on_decision() {
    let orch = DecisionOrchestrator::new();
    let mut case = fresh_case("alice");
    // Force into UnderReview manually so we go through the decide path.
    // A claim by any other broker would also work, but this exercises the
    // orchestrator's guard specifically.
    case.state = CaseState::UnderReview;
    case.assigned_to = Some("alice".into());
    let err = orch
        .decide(&mut case, "dec-1", DecisionOutcome::Approve, "alice", "ok")
        .unwrap_err();
    assert!(format!("{err}").contains("self-review"));
}

// ---------------------------------------------------------------------------
// 3. Delegation flow
// ---------------------------------------------------------------------------

#[test]
fn delegation_transitions_state_and_records_history() {
    let orch = DecisionOrchestrator::new();
    let mut case = fresh_case("alice");
    case.claim("bob").unwrap();
    orch.decide(
        &mut case,
        "dec-1",
        DecisionOutcome::Delegate {
            delegate_to: "carol".into(),
        },
        "bob",
        "domain expert",
    )
    .expect("delegate ok");
    assert_eq!(case.state, CaseState::Delegated);
    assert_eq!(case.history.len(), 1);
    // Decision history is append-only — the original decider is preserved.
    assert_eq!(case.history[0].broker_pubkey, "bob");
    match &case.history[0].outcome {
        DecisionOutcome::Delegate { delegate_to } => assert_eq!(delegate_to, "carol"),
        _ => panic!("expected Delegate outcome"),
    }
}

#[test]
fn terminal_state_rejects_further_decisions() {
    let orch = DecisionOrchestrator::new();
    let mut case = fresh_case("alice");
    case.claim("bob").unwrap();
    orch.decide(&mut case, "dec-1", DecisionOutcome::Approve, "bob", "ok")
        .unwrap();
    let err = orch
        .decide(
            &mut case,
            "dec-2",
            DecisionOutcome::Reject,
            "bob",
            "changed mind",
        )
        .unwrap_err();
    assert!(format!("{err}").contains("terminal"));
}

// ---------------------------------------------------------------------------
// 4. Six-variant coverage smoke test
// ---------------------------------------------------------------------------

#[test]
fn all_six_outcomes_representable() {
    let outcomes = vec![
        DecisionOutcome::Approve,
        DecisionOutcome::Reject,
        DecisionOutcome::Amend { diff: "+1 line".into() },
        DecisionOutcome::Delegate {
            delegate_to: "pubkey".into(),
        },
        DecisionOutcome::Promote {
            pattern_id: "pat-1".into(),
        },
        DecisionOutcome::Precedent {
            scope: "team:regulatory".into(),
        },
    ];
    let actions: Vec<&str> = outcomes.iter().map(|o| o.action_str()).collect();
    assert_eq!(
        actions,
        vec!["approve", "reject", "amend", "delegate", "promote", "precedent"]
    );
}
