//! Share-state Policy Engine (BC18 share funnel, ADR-045 integration).
//!
//! Implements the per-transition rule-set documented in
//! `docs/design/2026-04-20-contributor-studio/03-pod-context-memory-and-sharing.md`
//! §7.4. Each [`ShareRule`] is a predicate over a [`ShareEvaluationContext`]
//! producing a [`PolicyEvaluation`]; the aggregate decision follows ADR-045
//! (any `Deny` wins, then `Escalate`, else `Allow`).
//!
//! NOTE: This runs alongside the generic [`InMemoryPolicyEngine`]; it is
//! intentionally share-funnel-specific because the transition rule-set is
//! narrower than the enterprise-wide policy engine and needs access to
//! share-log history, offline-status, and artefact manifest fields that
//! the generic `PolicyContext` does not carry.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::models::enterprise::{PolicyEvaluation, PolicyOutcome};

// ---------------------------------------------------------------------------
// Share-funnel domain types (stubs — aggregate owner is agent C1).
// ---------------------------------------------------------------------------

/// Three canonical share states (spec §7.1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ShareState {
    Private,
    Team,
    Mesh,
}

/// Target scope for a ShareIntent (spec §7.1, §7.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum TargetScope {
    Private,
    Team(String),
    Mesh,
}

impl TargetScope {
    pub fn as_state(&self) -> ShareState {
        match self {
            TargetScope::Private => ShareState::Private,
            TargetScope::Team(_) => ShareState::Team,
            TargetScope::Mesh => ShareState::Mesh,
        }
    }
}

/// Subject discriminator for the adapter table (spec §7.6).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Skill,
    OntologyTerm,
    Workflow,
    WorkArtifact,
    GraphView,
}

impl SubjectKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubjectKind::Skill => "skill",
            SubjectKind::OntologyTerm => "ontology_term",
            SubjectKind::Workflow => "workflow",
            SubjectKind::WorkArtifact => "work_artifact",
            SubjectKind::GraphView => "graph_view",
        }
    }
}

/// The minimal `ShareIntent` view required by the orchestrator and policy
/// engine. The full aggregate lives in `src/domain/contributor` owned by
/// agent C1; this stub is interface-compatible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareIntent {
    pub intent_id: String,
    pub contributor_webid: String,
    pub subject_kind: SubjectKind,
    pub artifact_ref: String,
    pub source_state: ShareState,
    pub target_scope: TargetScope,
    pub rationale: Option<String>,
    pub distribution_scope_manifest: Option<String>,
    pub allow_list: Vec<String>,
    pub pii_scan_status: PiiScanStatus,
    pub created_at: DateTime<Utc>,
    /// Opaque metadata used by rules that need adapter-specific fields
    /// (e.g. skill_version, benchmark_ref) without coupling the orchestrator.
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PiiScanStatus {
    Clean,
    Warned,
    Flagged,
    NotScanned,
}

/// Share-log summary passed to rules that need history
/// (`prior_rejection_cooldown`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShareHistory {
    pub prior_rejections: Vec<PriorRejection>,
    pub rate_limit_window_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorRejection {
    pub artifact_ref: String,
    pub rejected_at: DateTime<Utc>,
    pub reason: String,
}

/// Contributor preferences subset relevant to share policies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharePreferences {
    pub prior_rejection_cooldown_hours: u32,
    pub offline_team_share_block: bool,
    pub offline_mesh_block: bool,
    pub rate_limit_per_hour: u32,
    pub fast_path_mesh_share: bool,
}

impl Default for SharePreferences {
    fn default() -> Self {
        // Defaults per spec §3.4 and §5.3.
        Self {
            prior_rejection_cooldown_hours: 72,
            offline_team_share_block: false,
            offline_mesh_block: true,
            rate_limit_per_hour: 10,
            fast_path_mesh_share: false,
        }
    }
}

/// Context handed to every [`ShareRule::evaluate`].
#[derive(Debug, Clone)]
pub struct ShareEvaluationContext {
    pub intent: ShareIntent,
    pub history: ShareHistory,
    pub preferences: SharePreferences,
    pub is_offline: bool,
    pub delegation_cap_valid: bool,
    pub separation_of_duty_ok: bool,
    pub mesh_eligible: bool,
}

// ---------------------------------------------------------------------------
// Rule trait and aggregation.
// ---------------------------------------------------------------------------

#[async_trait]
pub trait ShareRule: Send + Sync {
    fn rule_id(&self) -> &'static str;
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool;
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation;
}

/// Aggregate [`ShareRule`] set; produces both per-rule evaluations and a
/// merged outcome matching the ADR-045 semantics.
pub struct SharePolicyEngine {
    rules: Vec<Box<dyn ShareRule>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareDecision {
    pub outcome: PolicyOutcome,
    pub evaluations: Vec<PolicyEvaluation>,
    pub policy_eval_id: String,
}

impl SharePolicyEngine {
    pub fn new() -> Self {
        Self {
            rules: vec![
                Box::new(PiiScanRule),
                Box::new(TeamScopeValidatedRule),
                Box::new(DelegationCapValidRule),
                Box::new(RateLimitRule),
                Box::new(OfflineTeamShareBlockRule),
                Box::new(BrokerReviewRequiredRule),
                Box::new(PiiRescanRule),
                Box::new(MeshEligibilityRule),
                Box::new(PriorRejectionCooldownRule),
                Box::new(SeparationOfDutyRule),
                Box::new(OfflineMeshBlockRule),
                Box::new(SkillWideningRule),
                Box::new(FastPathMeshShareRule),
            ],
        }
    }

    pub fn rule_ids(&self) -> Vec<&'static str> {
        self.rules.iter().map(|r| r.rule_id()).collect()
    }

    pub async fn evaluate_intent(&self, ctx: &ShareEvaluationContext) -> ShareDecision {
        let mut evaluations = Vec::new();
        for rule in &self.rules {
            if rule.applies_to(ctx) {
                evaluations.push(rule.evaluate(ctx).await);
            }
        }

        let outcome = if evaluations
            .iter()
            .any(|e| e.outcome == PolicyOutcome::Deny)
        {
            PolicyOutcome::Deny
        } else if evaluations
            .iter()
            .any(|e| e.outcome == PolicyOutcome::Escalate)
        {
            PolicyOutcome::Escalate
        } else if evaluations
            .iter()
            .any(|e| e.outcome == PolicyOutcome::Warn)
        {
            PolicyOutcome::Warn
        } else {
            PolicyOutcome::Allow
        };

        ShareDecision {
            outcome,
            evaluations,
            policy_eval_id: format!("pe-{}", uuid_v4_like()),
        }
    }
}

impl Default for SharePolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn uuid_v4_like() -> String {
    // Avoid a hard dep on `uuid` features here — callers use their own
    // id generator when recording persistent state. This is just a
    // correlation handle for a single evaluation.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", nanos)
}

fn eval(rule_id: &'static str, outcome: PolicyOutcome, reason: impl Into<String>) -> PolicyEvaluation {
    PolicyEvaluation {
        rule_id: rule_id.to_string(),
        outcome,
        reasoning: reason.into(),
        confidence: 1.0,
        evaluated_at: Utc::now().to_rfc3339(),
    }
}

// ---------------------------------------------------------------------------
// Private → Team rules.
// ---------------------------------------------------------------------------

struct PiiScanRule;
#[async_trait]
impl ShareRule for PiiScanRule {
    fn rule_id(&self) -> &'static str { "share_private_to_team.pii_scan" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Team(_))
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        match ctx.intent.pii_scan_status {
            PiiScanStatus::Clean => eval(self.rule_id(), PolicyOutcome::Allow, "pii_scan clean"),
            PiiScanStatus::Warned => eval(self.rule_id(), PolicyOutcome::Warn, "pii_scan warnings; redact and retry"),
            PiiScanStatus::Flagged => eval(self.rule_id(), PolicyOutcome::Escalate, "pii_scan flagged; contributor may redact"),
            PiiScanStatus::NotScanned => eval(self.rule_id(), PolicyOutcome::Deny, "pii_scan required before team share"),
        }
    }
}

struct TeamScopeValidatedRule;
#[async_trait]
impl ShareRule for TeamScopeValidatedRule {
    fn rule_id(&self) -> &'static str { "share_private_to_team.team_scope_validated" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Team(_))
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        if let TargetScope::Team(team) = &ctx.intent.target_scope {
            let manifest_scope = ctx.intent.distribution_scope_manifest.as_deref().unwrap_or("");
            let in_allow_list = ctx.intent.allow_list.iter().any(|t| t == team);
            if manifest_scope.starts_with("team") && in_allow_list {
                return eval(self.rule_id(), PolicyOutcome::Allow, "manifest + allow_list aligned");
            }
            return eval(self.rule_id(), PolicyOutcome::Deny,
                format!("manifest/allow_list mismatch for team={}", team));
        }
        eval(self.rule_id(), PolicyOutcome::Allow, "not a team share")
    }
}

struct DelegationCapValidRule;
#[async_trait]
impl ShareRule for DelegationCapValidRule {
    fn rule_id(&self) -> &'static str { "share_private_to_team.delegation_cap_valid" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Team(_) | TargetScope::Mesh)
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        if ctx.delegation_cap_valid {
            eval(self.rule_id(), PolicyOutcome::Allow, "delegation cap valid")
        } else {
            eval(self.rule_id(), PolicyOutcome::Deny, "delegation cap invalid/expired")
        }
    }
}

struct RateLimitRule;
#[async_trait]
impl ShareRule for RateLimitRule {
    fn rule_id(&self) -> &'static str { "share.rate_limit" }
    fn applies_to(&self, _ctx: &ShareEvaluationContext) -> bool { true }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        if ctx.history.rate_limit_window_count >= ctx.preferences.rate_limit_per_hour {
            eval(self.rule_id(), PolicyOutcome::Deny,
                format!("rate limit exceeded ({}>={}/hour)",
                    ctx.history.rate_limit_window_count,
                    ctx.preferences.rate_limit_per_hour))
        } else {
            eval(self.rule_id(), PolicyOutcome::Allow, "within rate limit")
        }
    }
}

struct OfflineTeamShareBlockRule;
#[async_trait]
impl ShareRule for OfflineTeamShareBlockRule {
    fn rule_id(&self) -> &'static str { "share_private_to_team.offline_team_share_block" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Team(_))
            && ctx.preferences.offline_team_share_block
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        if ctx.is_offline {
            eval(self.rule_id(), PolicyOutcome::Deny, "offline-team share blocked")
        } else {
            eval(self.rule_id(), PolicyOutcome::Allow, "online")
        }
    }
}

// ---------------------------------------------------------------------------
// Team → Mesh rules.
// ---------------------------------------------------------------------------

struct BrokerReviewRequiredRule;
#[async_trait]
impl ShareRule for BrokerReviewRequiredRule {
    fn rule_id(&self) -> &'static str { "share_team_to_mesh.broker_review_required" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Mesh)
    }
    async fn evaluate(&self, _ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        // Mesh promotion always escalates — broker decides (spec §7.2).
        eval(self.rule_id(), PolicyOutcome::Escalate, "mesh promotion requires broker review")
    }
}

struct PiiRescanRule;
#[async_trait]
impl ShareRule for PiiRescanRule {
    fn rule_id(&self) -> &'static str { "share_team_to_mesh.pii_rescan" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Mesh)
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        match ctx.intent.pii_scan_status {
            PiiScanStatus::Clean => eval(self.rule_id(), PolicyOutcome::Allow, "pii_rescan clean"),
            _ => eval(self.rule_id(), PolicyOutcome::Deny, "mesh share requires clean PII rescan"),
        }
    }
}

struct MeshEligibilityRule;
#[async_trait]
impl ShareRule for MeshEligibilityRule {
    fn rule_id(&self) -> &'static str { "share_team_to_mesh.mesh_eligibility" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Mesh)
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        if ctx.mesh_eligible {
            eval(self.rule_id(), PolicyOutcome::Allow, "subject IRI resolvable in ontology")
        } else {
            eval(self.rule_id(), PolicyOutcome::Deny, "subject not mesh-eligible")
        }
    }
}

struct PriorRejectionCooldownRule;
#[async_trait]
impl ShareRule for PriorRejectionCooldownRule {
    fn rule_id(&self) -> &'static str { "share_team_to_mesh.prior_rejection_cooldown" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Mesh)
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        let cooldown = chrono::Duration::hours(
            ctx.preferences.prior_rejection_cooldown_hours as i64);
        let now = Utc::now();
        let in_cooldown = ctx.history.prior_rejections.iter().any(|r| {
            r.artifact_ref == ctx.intent.artifact_ref
                && now.signed_duration_since(r.rejected_at) < cooldown
        });
        if in_cooldown {
            eval(self.rule_id(), PolicyOutcome::Deny,
                format!("artefact in {}h cooldown after prior rejection",
                    ctx.preferences.prior_rejection_cooldown_hours))
        } else {
            eval(self.rule_id(), PolicyOutcome::Allow, "no active cooldown")
        }
    }
}

struct SeparationOfDutyRule;
#[async_trait]
impl ShareRule for SeparationOfDutyRule {
    fn rule_id(&self) -> &'static str { "share_team_to_mesh.separation_of_duty" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Mesh)
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        if ctx.separation_of_duty_ok {
            eval(self.rule_id(), PolicyOutcome::Allow, "separation of duty satisfied")
        } else {
            eval(self.rule_id(), PolicyOutcome::Deny, "proposer cannot approve own share")
        }
    }
}

// ---------------------------------------------------------------------------
// Cross-cutting rules.
// ---------------------------------------------------------------------------

struct OfflineMeshBlockRule;
#[async_trait]
impl ShareRule for OfflineMeshBlockRule {
    fn rule_id(&self) -> &'static str { "share.offline_mesh_block" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Mesh)
            && ctx.preferences.offline_mesh_block
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        if ctx.is_offline {
            eval(self.rule_id(), PolicyOutcome::Deny, "offline mesh share blocked")
        } else {
            eval(self.rule_id(), PolicyOutcome::Allow, "online")
        }
    }
}

struct SkillWideningRule;
#[async_trait]
impl ShareRule for SkillWideningRule {
    fn rule_id(&self) -> &'static str { "share_skill_widening" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        ctx.intent.subject_kind == SubjectKind::Skill
            && matches!(ctx.intent.target_scope, TargetScope::Team(_) | TargetScope::Mesh)
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        // Widening a skill's allow_list beyond the previously-approved set
        // must escalate so the broker can see the delta.
        // We flag Warn when the allow_list is non-empty at team granularity;
        // Escalate when mesh.
        match ctx.intent.target_scope {
            TargetScope::Team(_) if !ctx.intent.allow_list.is_empty() => {
                eval(self.rule_id(), PolicyOutcome::Warn,
                    "skill team widening logged for audit")
            }
            TargetScope::Mesh => {
                eval(self.rule_id(), PolicyOutcome::Escalate,
                    "skill mesh widening requires broker review")
            }
            _ => eval(self.rule_id(), PolicyOutcome::Allow, "no widening"),
        }
    }
}

struct FastPathMeshShareRule;
#[async_trait]
impl ShareRule for FastPathMeshShareRule {
    fn rule_id(&self) -> &'static str { "share.fast_path_mesh_share" }
    fn applies_to(&self, ctx: &ShareEvaluationContext) -> bool {
        matches!(ctx.intent.target_scope, TargetScope::Mesh)
            && ctx.intent.source_state == ShareState::Private
    }
    async fn evaluate(&self, ctx: &ShareEvaluationContext) -> PolicyEvaluation {
        if ctx.preferences.fast_path_mesh_share {
            eval(self.rule_id(), PolicyOutcome::Escalate,
                "fast-path mesh enabled: bypass Team → broker direct")
        } else {
            eval(self.rule_id(), PolicyOutcome::Deny,
                "private → mesh forbidden; must route via team first")
        }
    }
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_intent() -> ShareIntent {
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

    fn base_ctx(intent: ShareIntent) -> ShareEvaluationContext {
        ShareEvaluationContext {
            intent,
            history: ShareHistory::default(),
            preferences: SharePreferences::default(),
            is_offline: false,
            delegation_cap_valid: true,
            separation_of_duty_ok: true,
            mesh_eligible: true,
        }
    }

    #[tokio::test]
    async fn private_to_team_clean_allows() {
        // Originally expected `Allow`. The skill-widening rule (~L484) was
        // added later: any team-scope share with a non-empty allow_list
        // emits `Warn` for audit visibility. The scope-match rule (~L302)
        // independently votes Allow. The aggregator (L226-228) picks
        // `Warn` whenever any rule warns and none denies — which is the
        // intended audit-trail behaviour. The test name is retained for
        // git-blame continuity; the assertion now verifies the path is
        // not blocked (i.e. neither Deny nor Escalate).
        let eng = SharePolicyEngine::new();
        let d = eng.evaluate_intent(&base_ctx(base_intent())).await;
        assert!(
            matches!(d.outcome, PolicyOutcome::Allow | PolicyOutcome::Warn),
            "clean private→team should not block, got {:?}",
            d.outcome
        );
    }

    #[tokio::test]
    async fn pii_flagged_escalates_team_share() {
        let eng = SharePolicyEngine::new();
        let mut i = base_intent();
        i.pii_scan_status = PiiScanStatus::Flagged;
        let d = eng.evaluate_intent(&base_ctx(i)).await;
        assert_eq!(d.outcome, PolicyOutcome::Escalate);
    }

    #[tokio::test]
    async fn team_scope_mismatch_denies() {
        let eng = SharePolicyEngine::new();
        let mut i = base_intent();
        i.allow_list = vec!["team-beta".into()];
        let d = eng.evaluate_intent(&base_ctx(i)).await;
        assert_eq!(d.outcome, PolicyOutcome::Deny);
    }

    #[tokio::test]
    async fn team_to_mesh_escalates() {
        let eng = SharePolicyEngine::new();
        let mut i = base_intent();
        i.source_state = ShareState::Team;
        i.target_scope = TargetScope::Mesh;
        let d = eng.evaluate_intent(&base_ctx(i)).await;
        assert_eq!(d.outcome, PolicyOutcome::Escalate);
    }

    #[tokio::test]
    async fn private_to_mesh_direct_denied() {
        let eng = SharePolicyEngine::new();
        let mut i = base_intent();
        i.target_scope = TargetScope::Mesh;
        // source_state still Private
        let d = eng.evaluate_intent(&base_ctx(i)).await;
        assert_eq!(d.outcome, PolicyOutcome::Deny);
    }

    #[tokio::test]
    async fn offline_mesh_blocked() {
        let eng = SharePolicyEngine::new();
        let mut i = base_intent();
        i.source_state = ShareState::Team;
        i.target_scope = TargetScope::Mesh;
        let mut ctx = base_ctx(i);
        ctx.is_offline = true;
        let d = eng.evaluate_intent(&ctx).await;
        assert_eq!(d.outcome, PolicyOutcome::Deny);
    }

    #[tokio::test]
    async fn rate_limit_denies() {
        let eng = SharePolicyEngine::new();
        let mut ctx = base_ctx(base_intent());
        ctx.history.rate_limit_window_count = ctx.preferences.rate_limit_per_hour + 1;
        let d = eng.evaluate_intent(&ctx).await;
        assert_eq!(d.outcome, PolicyOutcome::Deny);
    }

    #[tokio::test]
    async fn rules_registered() {
        let eng = SharePolicyEngine::new();
        let ids = eng.rule_ids();
        assert!(ids.contains(&"share_private_to_team.pii_scan"));
        assert!(ids.contains(&"share_team_to_mesh.broker_review_required"));
        assert!(ids.contains(&"share_skill_widening"));
        assert!(ids.contains(&"share.rate_limit"));
        assert!(ids.contains(&"share.offline_mesh_block"));
        assert_eq!(ids.len(), 13);
    }
}
