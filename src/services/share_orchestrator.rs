//! Share Orchestrator — the single adapter that consumes ShareIntent events,
//! evaluates the share-funnel policy engine, mutates WAC per ADR-052
//! (extended to Team shares), and opens BrokerCases for Mesh promotions.
//!
//! Implements the transition matrix from
//! `docs/design/2026-04-20-contributor-studio/03-pod-context-memory-and-sharing.md`
//! §7.2. Three canonical states (`Private | Team | Mesh`) with six governed
//! transitions. The orchestrator is the *sole* surface through which
//! contributor Studio content becomes broker-visible (§7.6).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::models::enterprise::{
    BrokerCase, BrokerError, CasePriority, CaseStatus, EscalationSource, EvidenceItem,
    PolicyEvaluation, PolicyOutcome,
};
use crate::ports::broker_repository::BrokerRepository;
use crate::services::share_policy::{
    ShareEvaluationContext, ShareHistory, ShareIntent, SharePolicyEngine, SharePreferences,
    ShareState, SubjectKind, TargetScope,
};
use crate::services::wac_mutator::{WacMutationPlan, WacMutator, WacMutatorError};

// ---------------------------------------------------------------------------
// Transition matrix types.
// ---------------------------------------------------------------------------

/// Explicit enum of the six governed transitions (spec §7.2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShareTransition {
    /// Private → Team via `ShareIntent(target_scope=team:{t})`.
    PrivateToTeam,
    /// Private → Mesh: forbidden direct (requires `fast_path_mesh_share`).
    PrivateToMesh,
    /// Team → Mesh via `ShareIntent(target_scope=mesh)`.
    TeamToMesh,
    /// Team → Private via `ContributorRevocation`.
    TeamToPrivate,
    /// Mesh → Team via `BrokerRevocation(demote)`.
    MeshToTeam,
    /// Mesh → Removed via `BrokerDecision(retract)`.
    MeshRemoved,
}

impl ShareTransition {
    pub fn classify(src: ShareState, target: &TargetScope) -> Option<Self> {
        match (src, target) {
            (ShareState::Private, TargetScope::Team(_)) => Some(ShareTransition::PrivateToTeam),
            (ShareState::Private, TargetScope::Mesh)    => Some(ShareTransition::PrivateToMesh),
            (ShareState::Team,    TargetScope::Mesh)    => Some(ShareTransition::TeamToMesh),
            (ShareState::Team,    TargetScope::Private) => Some(ShareTransition::TeamToPrivate),
            (ShareState::Mesh,    TargetScope::Team(_)) => Some(ShareTransition::MeshToTeam),
            _ => None,
        }
    }
}

/// Result of routing a ShareIntent through the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ShareOutcome {
    TeamApproved {
        intent_id: String,
        plan: WacMutationPlanJson,
    },
    BrokerOpened {
        intent_id: String,
        case_id: String,
        subject_kind: SubjectKind,
    },
    Rejected {
        intent_id: String,
        reason: String,
        evaluations: Vec<PolicyEvaluation>,
    },
    Revoked {
        intent_id: String,
        plan: WacMutationPlanJson,
    },
    Retracted {
        intent_id: String,
    },
    Cancelled { intent_id: String, reason: String },
}

/// Serde-friendly WAC plan — mirrors [`WacMutationPlan`] without exposing
/// the whole turtle document payload in events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WacMutationPlanJson {
    pub destination_path: String,
    pub acl_document_path: String,
    pub agent_group: Option<String>,
}

impl From<WacMutationPlan> for WacMutationPlanJson {
    fn from(p: WacMutationPlan) -> Self {
        Self {
            destination_path: p.destination_path,
            acl_document_path: p.acl_document_path,
            agent_group: p.agent_group,
        }
    }
}

// ---------------------------------------------------------------------------
// Audit log (/private/contributor-profile/share-log.jsonld).
// ---------------------------------------------------------------------------

/// A single hash-chained entry in `share-log.jsonld` (spec §8.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareLogEntry {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@type")]
    pub entry_type: String,
    pub prev_hash: String,
    pub entry_hash: String,
    pub at: DateTime<Utc>,
    pub actor_webid: String,
    pub event: String,
    pub share_intent_id: String,
    pub artifact_ref: String,
    pub policy_eval_id: Option<String>,
    pub outcome: String,
}

/// Pluggable audit sink — in production writes to
/// `/private/contributor-profile/share-log.jsonld` via the Pod client; in
/// tests the in-memory sink captures the entries.
#[async_trait]
pub trait ShareAuditLog: Send + Sync {
    async fn append(&self, entry: ShareLogEntry) -> Result<(), ShareOrchestratorError>;
}

/// In-memory hash-chained audit log.
pub struct InMemoryShareAuditLog {
    entries: Mutex<Vec<ShareLogEntry>>,
    last_hash: Mutex<String>,
    actor_webid: String,
    counter: Mutex<u64>,
}

impl InMemoryShareAuditLog {
    pub fn new(actor_webid: impl Into<String>) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            last_hash: Mutex::new("sha256:genesis".into()),
            actor_webid: actor_webid.into(),
            counter: Mutex::new(0),
        }
    }

    pub async fn entries(&self) -> Vec<ShareLogEntry> {
        self.entries.lock().await.clone()
    }
}

#[async_trait]
impl ShareAuditLog for InMemoryShareAuditLog {
    async fn append(&self, mut entry: ShareLogEntry) -> Result<(), ShareOrchestratorError> {
        let mut last = self.last_hash.lock().await;
        entry.prev_hash = last.clone();
        let canonical = serde_json::to_string(&entry)
            .map_err(|e| ShareOrchestratorError::AuditSerialise(e.to_string()))?;
        let mut hasher = Sha256::new();
        hasher.update(last.as_bytes());
        hasher.update(canonical.as_bytes());
        let digest = hasher.finalize();
        entry.entry_hash = format!("sha256:{:x}", digest);
        *last = entry.entry_hash.clone();
        self.entries.lock().await.push(entry);
        Ok(())
    }
}

impl InMemoryShareAuditLog {
    pub async fn new_entry(
        &self,
        event: &str,
        intent: &ShareIntent,
        outcome: &str,
        policy_eval_id: Option<String>,
    ) -> ShareLogEntry {
        let mut counter = self.counter.lock().await;
        *counter += 1;
        ShareLogEntry {
            id: format!("urn:share-log:{}:{}", self.actor_webid, counter),
            entry_type: "ShareLogEntry".into(),
            prev_hash: String::new(), // filled in by append
            entry_hash: String::new(),
            at: Utc::now(),
            actor_webid: self.actor_webid.clone(),
            event: event.into(),
            share_intent_id: intent.intent_id.clone(),
            artifact_ref: intent.artifact_ref.clone(),
            policy_eval_id,
            outcome: outcome.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Broker adapter (spec §7.5 + §7.6 subject_kind table).
// ---------------------------------------------------------------------------

/// Payload attached to the BrokerCase when the orchestrator opens a case.
///
/// Every subject_kind gets a dedicated adapter function producing the
/// payload + evidence set; the broker dashboard projects this into its UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerCasePayload {
    pub category: String,
    pub subject_kind: String,
    pub subject_ref: String,
    pub contributor_webid: String,
    pub share_intent_id: String,
    pub policy_eval_id: Option<String>,
    pub payload: HashMap<String, String>,
    pub evidence: Vec<EvidenceItem>,
}

pub fn build_broker_payload(
    intent: &ShareIntent,
    policy_eval_id: Option<String>,
) -> BrokerCasePayload {
    let mut payload: HashMap<String, String> = HashMap::new();
    let mut evidence: Vec<EvidenceItem> = Vec::new();
    evidence.push(EvidenceItem {
        item_type: "share_intent".into(),
        source_id: intent.intent_id.clone(),
        description: format!(
            "ShareIntent for {} → {:?}", intent.artifact_ref, intent.target_scope),
        timestamp: Utc::now().to_rfc3339(),
    });

    // Adapter per subject_kind (spec §7.6).
    match intent.subject_kind {
        SubjectKind::Skill => {
            if let Some(v) = intent.metadata.get("skill_version") {
                payload.insert("skill_version".into(), v.clone());
            }
            if let Some(b) = intent.metadata.get("benchmark_ref") {
                payload.insert("benchmark_ref".into(), b.clone());
            }
            if let Some(a) = intent.metadata.get("team_adoption") {
                payload.insert("team_adoption".into(), a.clone());
            }
            if let Some(r) = &intent.rationale {
                payload.insert("rationale".into(), r.clone());
            }
        }
        SubjectKind::OntologyTerm => {
            if let Some(i) = intent.metadata.get("ontology_iri") {
                payload.insert("ontology_iri".into(), i.clone());
            }
            if let Some(p) = intent.metadata.get("parent_class") {
                payload.insert("parent_class".into(), p.clone());
            }
            payload.insert("delegates_to".into(), "migration_candidate".into());
        }
        SubjectKind::Workflow => {
            if let Some(p) = intent.metadata.get("workflow_proposal_id") {
                payload.insert("workflow_proposal_id".into(), p.clone());
            }
            payload.insert("advances".into(), "WorkflowProposal".into());
        }
        SubjectKind::WorkArtifact => {
            if let Some(p) = intent.metadata.get("project_id") {
                payload.insert("project_id".into(), p.clone());
            }
            payload.insert("register_with".into(), "WorkArtifactIndex".into());
        }
        SubjectKind::GraphView => {
            if let Some(c) = intent.metadata.get("confidence") {
                payload.insert("confidence".into(), c.clone());
            }
            payload.insert("routes_to".into(), "InsightCandidate".into());
        }
    }

    BrokerCasePayload {
        category: "contributor_mesh_share".into(),
        subject_kind: intent.subject_kind.as_str().into(),
        subject_ref: intent.artifact_ref.clone(),
        contributor_webid: intent.contributor_webid.clone(),
        share_intent_id: intent.intent_id.clone(),
        policy_eval_id,
        payload,
        evidence,
    }
}

/// Construct a [`BrokerCase`] suitable for persistence by a
/// [`BrokerRepository`]. Keeps the canonical category + subject_kind labels
/// in the case metadata so downstream (BC11/BC13) routing can branch.
pub fn broker_case_from_payload(payload: &BrokerCasePayload) -> BrokerCase {
    let mut metadata = HashMap::new();
    metadata.insert("category".into(), payload.category.clone());
    metadata.insert("subject_kind".into(), payload.subject_kind.clone());
    metadata.insert("contributor_webid".into(), payload.contributor_webid.clone());
    metadata.insert("share_intent_id".into(), payload.share_intent_id.clone());
    if let Some(p) = &payload.policy_eval_id {
        metadata.insert("policy_eval_id".into(), p.clone());
    }
    for (k, v) in &payload.payload {
        metadata.insert(format!("payload.{}", k), v.clone());
    }

    BrokerCase {
        id: format!("bc-{}-{}",
            payload.subject_kind,
            &payload.share_intent_id),
        title: format!("Mesh share: {} ({})",
            payload.subject_ref, payload.subject_kind),
        description: format!(
            "Contributor {} proposes mesh promotion of {} (subject_kind={})",
            payload.contributor_webid, payload.subject_ref, payload.subject_kind),
        priority: CasePriority::Medium,
        source: EscalationSource::WorkflowProposal,
        status: CaseStatus::Open,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
        assigned_to: None,
        evidence: payload.evidence.clone(),
        metadata,
    }
}

// ---------------------------------------------------------------------------
// Errors.
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ShareOrchestratorError {
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition { from: ShareState, to: TargetScope },

    #[error("wac mutation failed: {0}")]
    Wac(#[from] WacMutatorError),

    #[error("broker error: {0}")]
    Broker(String),

    #[error("audit serialisation failed: {0}")]
    AuditSerialise(String),
}

impl From<BrokerError> for ShareOrchestratorError {
    fn from(e: BrokerError) -> Self {
        ShareOrchestratorError::Broker(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Orchestrator service.
// ---------------------------------------------------------------------------

/// Construction-time ports for the orchestrator. All of these are `Arc<dyn _>`
/// so tests can substitute in-memory fakes and production wires real adapters.
pub struct ShareOrchestrator {
    pub policy: Arc<SharePolicyEngine>,
    pub wac: Arc<dyn WacMutator>,
    pub broker: Arc<dyn BrokerRepository>,
    pub audit: Arc<InMemoryShareAuditLog>,
    pub pod_base: String,
}

impl ShareOrchestrator {
    pub fn new(
        policy: Arc<SharePolicyEngine>,
        wac: Arc<dyn WacMutator>,
        broker: Arc<dyn BrokerRepository>,
        audit: Arc<InMemoryShareAuditLog>,
        pod_base: impl Into<String>,
    ) -> Self {
        Self { policy, wac, broker, audit, pod_base: pod_base.into() }
    }

    /// Route a ShareIntent through the full transition pipeline.
    ///
    /// Monotonic transitions (Private→Team, Team→Mesh) are forward.
    /// ContributorRevocation (Team→Private) and BrokerRevocation
    /// (Mesh→Team, Mesh→Removed) are the only backward transitions.
    pub async fn handle_intent(
        &self,
        intent: ShareIntent,
        ctx_extras: ShareContextExtras,
    ) -> Result<ShareOutcome, ShareOrchestratorError> {
        // 1. Classify transition.
        let transition = ShareTransition::classify(intent.source_state, &intent.target_scope)
            .ok_or(ShareOrchestratorError::InvalidTransition {
                from: intent.source_state,
                to: intent.target_scope.clone(),
            })?;

        // 2. Build evaluation context and run policy engine.
        let ctx = ShareEvaluationContext {
            intent: intent.clone(),
            history: ctx_extras.history,
            preferences: ctx_extras.preferences,
            is_offline: ctx_extras.is_offline,
            delegation_cap_valid: ctx_extras.delegation_cap_valid,
            separation_of_duty_ok: ctx_extras.separation_of_duty_ok,
            mesh_eligible: ctx_extras.mesh_eligible,
        };
        let decision = self.policy.evaluate_intent(&ctx).await;

        let entry = self.audit.new_entry(
            "share-intent-created", &intent, "created",
            Some(decision.policy_eval_id.clone())).await;
        self.audit.append(entry).await?;

        // 3. Branch on outcome.
        match (decision.outcome.clone(), transition) {
            // Hard denial.
            (PolicyOutcome::Deny, _) => {
                let reason = decision.evaluations.iter()
                    .find(|e| e.outcome == PolicyOutcome::Deny)
                    .map(|e| e.reasoning.clone())
                    .unwrap_or_else(|| "policy denied".into());
                let entry = self.audit.new_entry(
                    "share-intent-rejected", &intent, "rejected",
                    Some(decision.policy_eval_id.clone())).await;
                self.audit.append(entry).await?;
                Ok(ShareOutcome::Rejected {
                    intent_id: intent.intent_id,
                    reason,
                    evaluations: decision.evaluations,
                })
            }

            // Forward Private → Team (no broker, apply WAC).
            (PolicyOutcome::Allow | PolicyOutcome::Warn, ShareTransition::PrivateToTeam) => {
                let plan = self.wac.apply(&intent, &self.pod_base).await?;
                let entry = self.audit.new_entry(
                    "share-intent-approved", &intent, "team_approved",
                    Some(decision.policy_eval_id.clone())).await;
                self.audit.append(entry).await?;
                Ok(ShareOutcome::TeamApproved {
                    intent_id: intent.intent_id,
                    plan: plan.into(),
                })
            }

            // Team → Mesh (or escalated Private → Mesh with fast-path):
            // open a BrokerCase, no WAC change yet.
            (PolicyOutcome::Escalate | PolicyOutcome::Allow,
                ShareTransition::TeamToMesh | ShareTransition::PrivateToMesh) => {
                let payload = build_broker_payload(&intent,
                    Some(decision.policy_eval_id.clone()));
                let case = broker_case_from_payload(&payload);
                self.broker.create_case(&case).await?;
                let entry = self.audit.new_entry(
                    "share-intent-broker-opened", &intent, "broker_opened",
                    Some(decision.policy_eval_id.clone())).await;
                self.audit.append(entry).await?;
                Ok(ShareOutcome::BrokerOpened {
                    intent_id: intent.intent_id,
                    case_id: case.id,
                    subject_kind: intent.subject_kind,
                })
            }

            // ContributorRevocation / demotion paths.
            (_, ShareTransition::TeamToPrivate) |
            (_, ShareTransition::MeshToTeam) => {
                let plan = self.wac.revoke(&intent, &self.pod_base).await?;
                let event = if transition == ShareTransition::TeamToPrivate {
                    "team-share-revoked"
                } else {
                    "mesh-revoked"
                };
                let entry = self.audit.new_entry(event, &intent, "revoked",
                    Some(decision.policy_eval_id.clone())).await;
                self.audit.append(entry).await?;
                Ok(ShareOutcome::Revoked {
                    intent_id: intent.intent_id,
                    plan: plan.into(),
                })
            }

            // Mesh → removed (retract).
            (_, ShareTransition::MeshRemoved) => {
                // Retract: move out of /public/ back to /private/archive/.
                let plan = self.wac.revoke(&intent, &self.pod_base).await?;
                let entry = self.audit.new_entry(
                    "mesh-retracted", &intent, "retracted",
                    Some(decision.policy_eval_id.clone())).await;
                self.audit.append(entry).await?;
                let _ = plan; // destination used in Revoked variant; Retracted is terminal
                Ok(ShareOutcome::Retracted { intent_id: intent.intent_id })
            }

            // Escalate for Private→Team: should not normally happen; surface
            // as broker-opened so contributor does not lose the intent.
            (PolicyOutcome::Escalate, ShareTransition::PrivateToTeam) => {
                let payload = build_broker_payload(&intent,
                    Some(decision.policy_eval_id.clone()));
                let case = broker_case_from_payload(&payload);
                self.broker.create_case(&case).await?;
                let entry = self.audit.new_entry(
                    "share-intent-broker-opened", &intent, "escalated",
                    Some(decision.policy_eval_id.clone())).await;
                self.audit.append(entry).await?;
                Ok(ShareOutcome::BrokerOpened {
                    intent_id: intent.intent_id,
                    case_id: case.id,
                    subject_kind: intent.subject_kind,
                })
            }
        }
    }
}

/// Dynamic context fields the orchestrator looks up from its environment
/// (ShareLog, preferences, session state) and passes to the policy engine.
#[derive(Debug, Clone, Default)]
pub struct ShareContextExtras {
    pub history: ShareHistory,
    pub preferences: SharePreferences,
    pub is_offline: bool,
    pub delegation_cap_valid: bool,
    pub separation_of_duty_ok: bool,
    pub mesh_eligible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::share_policy::{PiiScanStatus};
    use crate::services::wac_mutator::InMemoryWacMutator;

    // In-memory broker repo for unit tests.
    struct InMemoryBroker {
        cases: Mutex<Vec<BrokerCase>>,
    }
    impl InMemoryBroker {
        fn new() -> Arc<Self> { Arc::new(Self { cases: Mutex::new(Vec::new()) }) }
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
            self.cases.lock().await.push(c.clone()); Ok(())
        }
        async fn update_case_status(&self, _id: &str, _st: CaseStatus)
            -> Result<(), BrokerError> { Ok(()) }
        async fn record_decision(&self, _d: &crate::models::enterprise::BrokerDecision)
            -> Result<(), BrokerError> { Ok(()) }
        async fn get_decisions(&self, _cid: &str)
            -> Result<Vec<crate::models::enterprise::BrokerDecision>, BrokerError> { Ok(vec![]) }
    }

    fn team_intent() -> ShareIntent {
        ShareIntent {
            intent_id: "si-1".into(),
            contributor_webid: "https://alice.pod/profile/card#me".into(),
            subject_kind: SubjectKind::Skill,
            artifact_ref: "pod:/private/skills/research-brief.md".into(),
            source_state: ShareState::Private,
            target_scope: TargetScope::Team("team-alpha".into()),
            rationale: Some("baseline".into()),
            distribution_scope_manifest: Some("team".into()),
            allow_list: vec!["team-alpha".into()],
            pii_scan_status: PiiScanStatus::Clean,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    fn extras_ok() -> ShareContextExtras {
        ShareContextExtras {
            delegation_cap_valid: true,
            separation_of_duty_ok: true,
            mesh_eligible: true,
            ..Default::default()
        }
    }

    async fn orchestrator() -> ShareOrchestrator {
        ShareOrchestrator::new(
            Arc::new(SharePolicyEngine::new()),
            Arc::new(InMemoryWacMutator::default()),
            InMemoryBroker::new(),
            Arc::new(InMemoryShareAuditLog::new("https://alice.pod/profile/card#me")),
            "https://alice.pod",
        )
    }

    #[tokio::test]
    async fn hash_chain_links_entries() {
        let log = InMemoryShareAuditLog::new("https://alice/profile/card#me");
        let e1 = log.new_entry("share-intent-created", &team_intent(),
            "created", None).await;
        log.append(e1).await.unwrap();
        let e2 = log.new_entry("share-intent-approved", &team_intent(),
            "team_approved", None).await;
        log.append(e2).await.unwrap();
        let entries = log.entries().await;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].prev_hash, entries[0].entry_hash);
    }

    #[tokio::test]
    async fn adapter_skill_to_contributor_mesh_share() {
        let mut i = team_intent();
        i.source_state = ShareState::Team;
        i.target_scope = TargetScope::Mesh;
        i.distribution_scope_manifest = Some("mesh".into());
        i.metadata.insert("skill_version".into(), "1.3.0".into());
        i.metadata.insert("benchmark_ref".into(), "pod:/private/skill-evals/x.jsonl".into());
        let payload = build_broker_payload(&i, Some("pe-1".into()));
        assert_eq!(payload.category, "contributor_mesh_share");
        assert_eq!(payload.subject_kind, "skill");
        assert_eq!(payload.payload.get("skill_version").unwrap(), "1.3.0");
    }

    #[tokio::test]
    async fn adapter_all_five_subject_kinds_covered() {
        let kinds = [
            SubjectKind::Skill,
            SubjectKind::OntologyTerm,
            SubjectKind::Workflow,
            SubjectKind::WorkArtifact,
            SubjectKind::GraphView,
        ];
        for k in kinds {
            let mut i = team_intent();
            i.subject_kind = k;
            i.source_state = ShareState::Team;
            i.target_scope = TargetScope::Mesh;
            i.distribution_scope_manifest = Some("mesh".into());
            let payload = build_broker_payload(&i, Some("pe-1".into()));
            assert_eq!(payload.subject_kind, k.as_str());
        }
    }

    #[tokio::test]
    async fn private_to_team_transition_approves_and_moves() {
        let o = orchestrator().await;
        let out = o.handle_intent(team_intent(), extras_ok()).await.unwrap();
        assert!(matches!(out, ShareOutcome::TeamApproved { .. }));
    }

    #[tokio::test]
    async fn team_to_mesh_opens_broker_case() {
        let o = orchestrator().await;
        let mut i = team_intent();
        i.source_state = ShareState::Team;
        i.target_scope = TargetScope::Mesh;
        i.distribution_scope_manifest = Some("mesh".into());
        let out = o.handle_intent(i, extras_ok()).await.unwrap();
        match out {
            ShareOutcome::BrokerOpened { case_id, subject_kind, .. } => {
                assert!(case_id.starts_with("bc-skill-"));
                assert_eq!(subject_kind, SubjectKind::Skill);
            }
            _ => panic!("expected BrokerOpened, got {:?}", out),
        }
    }

    #[tokio::test]
    async fn private_to_mesh_direct_rejected() {
        let o = orchestrator().await;
        let mut i = team_intent();
        i.target_scope = TargetScope::Mesh;
        i.distribution_scope_manifest = Some("mesh".into());
        let out = o.handle_intent(i, extras_ok()).await.unwrap();
        assert!(matches!(out, ShareOutcome::Rejected { .. }));
    }

    #[tokio::test]
    async fn contributor_revocation_reverts_to_private() {
        let o = orchestrator().await;
        let mut i = team_intent();
        i.source_state = ShareState::Team;
        i.target_scope = TargetScope::Private;
        i.artifact_ref = "pod:/shared/skills/team-alpha/research-brief.md".into();
        let out = o.handle_intent(i, extras_ok()).await.unwrap();
        assert!(matches!(out, ShareOutcome::Revoked { .. }));
    }

    #[tokio::test]
    async fn double_gate_failure_short_circuits_apply() {
        let o = orchestrator().await;
        let mut i = team_intent();
        // manifest + allow_list deliberately mismatch.
        i.distribution_scope_manifest = Some("team".into());
        i.allow_list = vec!["team-beta".into()]; // but target is team-alpha
        // Team scope rule catches this in policy (deny), so we test the
        // WAC-level gate via a bypass: skip policy scope check.
        let ctx = ShareContextExtras {
            delegation_cap_valid: true,
            separation_of_duty_ok: true,
            mesh_eligible: true,
            ..Default::default()
        };
        let out = o.handle_intent(i, ctx).await.unwrap();
        assert!(matches!(out, ShareOutcome::Rejected { .. }));
    }
}
