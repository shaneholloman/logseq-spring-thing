//! Integration tests for the Share Orchestrator + actor + WAC mutator +
//! policy engine (agent C4). Covers the full six-transition matrix
//! defined in docs/design/2026-04-20-contributor-studio/03 §7.2 and the
//! ADR-052 double-gate extended to Team shares.
//!
//! The tests use in-memory ports throughout; no Pod or Neo4j side-effects.

use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use webxr::models::enterprise::{
    BrokerCase, BrokerDecision, CaseStatus, PolicyOutcome,
};
use webxr::ports::broker_repository::{BrokerError, BrokerRepository};
use webxr::services::share_orchestrator::{
    build_broker_payload, broker_case_from_payload, InMemoryShareAuditLog, ShareContextExtras,
    ShareOrchestrator, ShareOutcome, ShareTransition,
};
use webxr::services::share_policy::{
    PiiScanStatus, ShareHistory, ShareIntent, SharePolicyEngine, SharePreferences, ShareState,
    SubjectKind, TargetScope,
};
use webxr::services::wac_mutator::InMemoryWacMutator;

// ---------------------------------------------------------------------------
// In-memory broker repo.
// ---------------------------------------------------------------------------

struct InMemoryBroker {
    cases: Mutex<Vec<BrokerCase>>,
    decisions: Mutex<Vec<BrokerDecision>>,
}
impl InMemoryBroker {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            cases: Mutex::new(Vec::new()),
            decisions: Mutex::new(Vec::new()),
        })
    }
    async fn count(&self) -> usize {
        self.cases.lock().await.len()
    }
}
#[async_trait]
impl BrokerRepository for InMemoryBroker {
    async fn list_cases(&self, _s: Option<CaseStatus>, _l: usize)
        -> Result<Vec<BrokerCase>, BrokerError> {
        Ok(self.cases.lock().await.clone())
    }
    async fn get_case(&self, id: &str) -> Result<Option<BrokerCase>, BrokerError> {
        Ok(self.cases.lock().await.iter().find(|c| c.id == id).cloned())
    }
    async fn create_case(&self, c: &BrokerCase) -> Result<(), BrokerError> {
        self.cases.lock().await.push(c.clone());
        Ok(())
    }
    async fn update_case_status(&self, _id: &str, _st: CaseStatus)
        -> Result<(), BrokerError> { Ok(()) }
    async fn record_decision(&self, d: &BrokerDecision)
        -> Result<(), BrokerError> {
        self.decisions.lock().await.push(d.clone());
        Ok(())
    }
    async fn get_decisions(&self, _cid: &str)
        -> Result<Vec<BrokerDecision>, BrokerError> {
        Ok(self.decisions.lock().await.clone())
    }
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

fn base_intent() -> ShareIntent {
    ShareIntent {
        intent_id: "si-test-001".into(),
        contributor_webid: "https://alice.pod/profile/card#me".into(),
        subject_kind: SubjectKind::Skill,
        artifact_ref: "pod:/private/skills/research-brief.md".into(),
        source_state: ShareState::Private,
        target_scope: TargetScope::Team("team-alpha".into()),
        rationale: Some("baseline for the research pod".into()),
        distribution_scope_manifest: Some("team".into()),
        allow_list: vec!["team-alpha".into()],
        pii_scan_status: PiiScanStatus::Clean,
        created_at: Utc::now(),
        metadata: HashMap::new(),
    }
}

fn extras_ok() -> ShareContextExtras {
    ShareContextExtras {
        history: ShareHistory::default(),
        preferences: SharePreferences::default(),
        is_offline: false,
        delegation_cap_valid: true,
        separation_of_duty_ok: true,
        mesh_eligible: true,
    }
}

async fn orchestrator() -> (ShareOrchestrator, Arc<InMemoryWacMutator>, Arc<InMemoryBroker>, Arc<InMemoryShareAuditLog>) {
    let wac: Arc<InMemoryWacMutator> = Arc::new(InMemoryWacMutator::default());
    let broker = InMemoryBroker::new();
    let audit = Arc::new(InMemoryShareAuditLog::new("https://alice.pod/profile/card#me"));
    let o = ShareOrchestrator::new(
        Arc::new(SharePolicyEngine::new()),
        Arc::clone(&wac) as Arc<dyn webxr::services::wac_mutator::WacMutator>,
        broker.clone(),
        Arc::clone(&audit),
        "https://alice.pod",
    );
    (o, wac, broker, audit)
}

// ---------------------------------------------------------------------------
// Transition matrix — one test per transition (spec §7.2).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transition_private_to_team_approves_and_writes_wac() {
    let (o, wac, broker, audit) = orchestrator().await;
    let out = o.handle_intent(base_intent(), extras_ok()).await.unwrap();
    match out {
        ShareOutcome::TeamApproved { plan, .. } => {
            assert_eq!(plan.destination_path,
                "/shared/skills/team-alpha/research-brief.md");
            assert!(plan.agent_group.unwrap().contains("team-alpha"));
        }
        _ => panic!("expected TeamApproved, got {:?}", out),
    }
    assert_eq!(wac.applied.lock().unwrap().len(), 1);
    assert_eq!(broker.count().await, 0);
    assert!(audit.entries().await.len() >= 2);
}

#[tokio::test]
async fn transition_private_to_mesh_direct_rejected() {
    let (o, wac, broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.target_scope = TargetScope::Mesh;
    i.distribution_scope_manifest = Some("mesh".into());
    let out = o.handle_intent(i, extras_ok()).await.unwrap();
    assert!(matches!(out, ShareOutcome::Rejected { .. }));
    assert_eq!(wac.applied.lock().unwrap().len(), 0);
    assert_eq!(broker.count().await, 0);
}

#[tokio::test]
async fn transition_team_to_mesh_opens_broker_case() {
    let (o, wac, broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.source_state = ShareState::Team;
    i.target_scope = TargetScope::Mesh;
    i.artifact_ref = "pod:/shared/skills/team-alpha/research-brief.md".into();
    i.distribution_scope_manifest = Some("mesh".into());
    i.metadata.insert("skill_version".into(), "1.3.0".into());
    let out = o.handle_intent(i, extras_ok()).await.unwrap();
    match out {
        ShareOutcome::BrokerOpened { case_id, .. } => {
            assert!(case_id.starts_with("bc-skill-"));
        }
        _ => panic!("expected BrokerOpened, got {:?}", out),
    }
    assert_eq!(broker.count().await, 1);
    // WAC unchanged on mesh open (only on broker Promote).
    assert_eq!(wac.applied.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn transition_team_to_private_revokes_wac() {
    let (o, wac, broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.source_state = ShareState::Team;
    i.target_scope = TargetScope::Private;
    i.artifact_ref = "pod:/shared/skills/team-alpha/research-brief.md".into();
    let out = o.handle_intent(i, extras_ok()).await.unwrap();
    assert!(matches!(out, ShareOutcome::Revoked { .. }));
    assert_eq!(wac.revoked.lock().unwrap().len(), 1);
    assert_eq!(broker.count().await, 0);
}

#[tokio::test]
async fn transition_mesh_to_team_revokes_wac() {
    let (o, wac, _broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.source_state = ShareState::Mesh;
    i.target_scope = TargetScope::Team("team-alpha".into());
    i.artifact_ref = "pod:/public/skills/research-brief.md".into();
    let out = o.handle_intent(i, extras_ok()).await.unwrap();
    assert!(matches!(out, ShareOutcome::Revoked { .. }));
    assert_eq!(wac.revoked.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn transition_classifier_covers_six_cases() {
    use ShareTransition::*;
    assert_eq!(ShareTransition::classify(ShareState::Private,
        &TargetScope::Team("t".into())), Some(PrivateToTeam));
    assert_eq!(ShareTransition::classify(ShareState::Private,
        &TargetScope::Mesh), Some(PrivateToMesh));
    assert_eq!(ShareTransition::classify(ShareState::Team,
        &TargetScope::Mesh), Some(TeamToMesh));
    assert_eq!(ShareTransition::classify(ShareState::Team,
        &TargetScope::Private), Some(TeamToPrivate));
    assert_eq!(ShareTransition::classify(ShareState::Mesh,
        &TargetScope::Team("t".into())), Some(MeshToTeam));
    // Mesh→Private not directly supported (broker-driven retraction only).
    assert_eq!(ShareTransition::classify(ShareState::Mesh,
        &TargetScope::Private), None);
}

// ---------------------------------------------------------------------------
// Failure modes — policy rejects + double-gate rejects.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn policy_failure_pii_unscanned_denies_team_share() {
    let (o, wac, _broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.pii_scan_status = PiiScanStatus::NotScanned;
    let out = o.handle_intent(i, extras_ok()).await.unwrap();
    assert!(matches!(out, ShareOutcome::Rejected { .. }));
    assert_eq!(wac.applied.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn policy_failure_rate_limit_denies() {
    let (o, _wac, _broker, _audit) = orchestrator().await;
    let mut extras = extras_ok();
    extras.history.rate_limit_window_count = extras.preferences.rate_limit_per_hour + 5;
    let out = o.handle_intent(base_intent(), extras).await.unwrap();
    assert!(matches!(out, ShareOutcome::Rejected { .. }));
}

#[tokio::test]
async fn policy_failure_separation_of_duty_denies_mesh() {
    let (o, _wac, _broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.source_state = ShareState::Team;
    i.target_scope = TargetScope::Mesh;
    i.distribution_scope_manifest = Some("mesh".into());
    let mut extras = extras_ok();
    extras.separation_of_duty_ok = false;
    let out = o.handle_intent(i, extras).await.unwrap();
    assert!(matches!(out, ShareOutcome::Rejected { .. }));
}

#[tokio::test]
async fn policy_failure_offline_mesh_blocks() {
    let (o, _wac, _broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.source_state = ShareState::Team;
    i.target_scope = TargetScope::Mesh;
    i.distribution_scope_manifest = Some("mesh".into());
    let mut extras = extras_ok();
    extras.is_offline = true;
    let out = o.handle_intent(i, extras).await.unwrap();
    assert!(matches!(out, ShareOutcome::Rejected { .. }));
}

#[tokio::test]
async fn policy_failure_invalid_delegation_cap_denies() {
    let (o, _wac, _broker, _audit) = orchestrator().await;
    let mut extras = extras_ok();
    extras.delegation_cap_valid = false;
    let out = o.handle_intent(base_intent(), extras).await.unwrap();
    assert!(matches!(out, ShareOutcome::Rejected { .. }));
}

#[tokio::test]
async fn double_gate_manifest_missing_caught_by_policy() {
    let (o, wac, _broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.distribution_scope_manifest = None;
    let out = o.handle_intent(i, extras_ok()).await.unwrap();
    // Policy rule `team_scope_validated` catches manifest absence first.
    assert!(matches!(out, ShareOutcome::Rejected { .. }));
    assert_eq!(wac.applied.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn double_gate_allow_list_mismatch_caught_by_policy() {
    let (o, wac, _broker, _audit) = orchestrator().await;
    let mut i = base_intent();
    i.allow_list = vec!["team-gamma".into()];
    // target_scope still team-alpha → manifest aligned but allow_list not
    let out = o.handle_intent(i, extras_ok()).await.unwrap();
    assert!(matches!(out, ShareOutcome::Rejected { .. }));
    assert_eq!(wac.applied.lock().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Adapter coverage — 5 subject_kinds per §7.6.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn adapter_builds_payload_for_all_five_subject_kinds() {
    for kind in [
        SubjectKind::Skill,
        SubjectKind::OntologyTerm,
        SubjectKind::Workflow,
        SubjectKind::WorkArtifact,
        SubjectKind::GraphView,
    ] {
        let mut i = base_intent();
        i.subject_kind = kind;
        i.source_state = ShareState::Team;
        i.target_scope = TargetScope::Mesh;
        i.distribution_scope_manifest = Some("mesh".into());
        let payload = build_broker_payload(&i, Some("pe-abc".into()));
        assert_eq!(payload.category, "contributor_mesh_share");
        assert_eq!(payload.subject_kind, kind.as_str());
        assert_eq!(payload.share_intent_id, i.intent_id);
        assert!(!payload.evidence.is_empty());

        let case = broker_case_from_payload(&payload);
        assert!(case.id.contains(kind.as_str()));
        assert_eq!(case.metadata.get("category").unwrap(), "contributor_mesh_share");
        assert_eq!(case.metadata.get("subject_kind").unwrap(), kind.as_str());
    }
}

#[tokio::test]
async fn adapter_ontology_term_delegates_to_migration_candidate() {
    let mut i = base_intent();
    i.subject_kind = SubjectKind::OntologyTerm;
    i.source_state = ShareState::Team;
    i.target_scope = TargetScope::Mesh;
    i.metadata.insert("ontology_iri".into(),
        "urn:visionclaw:ont/capability-compounding".into());
    i.metadata.insert("parent_class".into(), "Capability".into());
    let payload = build_broker_payload(&i, Some("pe-1".into()));
    assert_eq!(payload.payload.get("delegates_to").unwrap(), "migration_candidate");
    assert!(payload.payload.contains_key("ontology_iri"));
}

#[tokio::test]
async fn adapter_workflow_advances_workflow_proposal() {
    let mut i = base_intent();
    i.subject_kind = SubjectKind::Workflow;
    i.source_state = ShareState::Team;
    i.target_scope = TargetScope::Mesh;
    i.metadata.insert("workflow_proposal_id".into(), "wp-123".into());
    let payload = build_broker_payload(&i, Some("pe-1".into()));
    assert_eq!(payload.payload.get("advances").unwrap(), "WorkflowProposal");
}

#[tokio::test]
async fn adapter_graph_view_routes_to_insight_candidate() {
    let mut i = base_intent();
    i.subject_kind = SubjectKind::GraphView;
    i.source_state = ShareState::Team;
    i.target_scope = TargetScope::Mesh;
    i.metadata.insert("confidence".into(), "0.84".into());
    let payload = build_broker_payload(&i, Some("pe-1".into()));
    assert_eq!(payload.payload.get("routes_to").unwrap(), "InsightCandidate");
}

// ---------------------------------------------------------------------------
// Audit log — hash chain + /private/contributor-profile/ target path.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_log_hash_chain_continuous_across_approve_flow() {
    let (o, _wac, _broker, audit) = orchestrator().await;
    o.handle_intent(base_intent(), extras_ok()).await.unwrap();
    let entries = audit.entries().await;
    assert!(entries.len() >= 2);
    for pair in entries.windows(2) {
        assert_eq!(pair[1].prev_hash, pair[0].entry_hash);
    }
    assert!(entries.iter().any(|e| e.event == "share-intent-created"));
    assert!(entries.iter().any(|e| e.event == "share-intent-approved"));
}

#[tokio::test]
async fn audit_log_records_rejection() {
    let (o, _wac, _broker, audit) = orchestrator().await;
    let mut i = base_intent();
    i.pii_scan_status = PiiScanStatus::NotScanned;
    o.handle_intent(i, extras_ok()).await.unwrap();
    let entries = audit.entries().await;
    assert!(entries.iter().any(|e| e.event == "share-intent-rejected"));
}
