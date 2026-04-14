//! Enterprise domain model types for VisionClaw.
//!
//! Covers the domain types defined in ADRs 040–045:
//! - Enterprise roles (ADR-040)
//! - Broker cases and decisions (ADR-041)
//! - Workflow proposals and patterns (ADR-042)
//! - KPI metric snapshots (ADR-043)
//! - Connector sources and discovery signals (ADR-044)
//! - Policy evaluation and context (ADR-045)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Enterprise Roles (ADR-040)
// ---------------------------------------------------------------------------

/// Role within the enterprise governance layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnterpriseRole {
    Broker,
    Admin,
    Auditor,
    Contributor,
}

// ---------------------------------------------------------------------------
// Broker Types (ADR-041)
// ---------------------------------------------------------------------------

/// Priority level for broker cases.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CasePriority {
    Critical,
    High,
    Medium,
    Low,
}

/// Source of an escalation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EscalationSource {
    PolicyViolation,
    ConfidenceThreshold,
    TrustDrift,
    ManualSubmission,
    WorkflowProposal,
}

/// Lifecycle status of a broker case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Open,
    InReview,
    Decided,
    Escalated,
    Closed,
}

/// A single piece of evidence attached to a [`BrokerCase`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceItem {
    pub item_type: String,
    pub source_id: String,
    pub description: String,
    pub timestamp: String,
}

/// A case in the broker's inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerCase {
    pub id: String,
    pub title: String,
    pub description: String,
    pub priority: CasePriority,
    pub source: EscalationSource,
    pub status: CaseStatus,
    pub created_at: String,
    pub updated_at: String,
    pub assigned_to: Option<String>,
    pub evidence: Vec<EvidenceItem>,
    pub metadata: HashMap<String, String>,
}

/// Action taken by a broker on a case.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    Approve,
    Reject,
    Amend,
    Delegate,
    PromoteAsWorkflow,
    MarkAsPrecedent,
    RequestMoreEvidence,
}

/// A decision made by a broker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerDecision {
    pub id: String,
    pub case_id: String,
    pub action: DecisionAction,
    pub reasoning: String,
    pub decided_by: String,
    pub decided_at: String,
    pub provenance_event_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Workflow Types (ADR-042)
// ---------------------------------------------------------------------------

/// Lifecycle status of a workflow proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Draft,
    Submitted,
    UnderReview,
    Approved,
    Deployed,
    Archived,
    RolledBack,
}

/// A single step inside a [`WorkflowProposal`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub order: u32,
    pub name: String,
    pub action_type: String,
    pub config: serde_json::Value,
}

/// A proposal for a new or modified workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowProposal {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: WorkflowStatus,
    pub version: u32,
    pub steps: Vec<WorkflowStep>,
    pub source_insight_id: Option<String>,
    pub submitted_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub risk_score: Option<f64>,
    pub expected_benefit: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// A deployed workflow pattern derived from a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPattern {
    pub id: String,
    pub title: String,
    pub description: String,
    pub active_version_id: String,
    pub deployed_at: String,
    pub deployed_by: String,
    pub adoption_count: u32,
    pub rollback_target_id: Option<String>,
}

// ---------------------------------------------------------------------------
// KPI Types (ADR-043)
// ---------------------------------------------------------------------------

/// Well-known KPI metric identifiers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KpiType {
    MeshVelocity,
    AugmentationRatio,
    TrustVariance,
    HitlPrecision,
}

/// A point-in-time snapshot of a KPI metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricSnapshot {
    pub id: String,
    pub kpi_type: KpiType,
    pub value: f64,
    pub confidence: f64,
    pub time_window_start: String,
    pub time_window_end: String,
    pub computed_at: String,
    pub source_event_count: u32,
    pub dimensions: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Policy Types (ADR-045)
// ---------------------------------------------------------------------------

/// Outcome of a policy evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcome {
    Allow,
    Deny,
    Escalate,
    Warn,
}

/// Result of evaluating a single policy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyEvaluation {
    pub rule_id: String,
    pub outcome: PolicyOutcome,
    pub reasoning: String,
    pub confidence: f64,
    pub evaluated_at: String,
}

/// Actions governed by the policy engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    ApproveWorkflow,
    DeployWorkflow,
    EscalateCase,
    OverrideDecision,
    AccessConnector,
    ModifyPolicy,
}

/// Context supplied when requesting a policy decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyContext {
    pub actor_id: String,
    pub action: PolicyAction,
    pub resource_id: String,
    pub resource_type: String,
    pub confidence: Option<f64>,
    pub domain: Option<String>,
    pub metadata: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Connector Types (ADR-044)
// ---------------------------------------------------------------------------

/// Type of external connector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorType {
    GitHub,
    Jira,
    Slack,
    Custom,
}

/// Operational status of a connector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStatus {
    Active,
    Paused,
    Error,
    Configuring,
}

/// An external data connector source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorSource {
    pub id: String,
    pub connector_type: ConnectorType,
    pub name: String,
    pub status: ConnectorStatus,
    pub config: serde_json::Value,
    pub last_sync: Option<String>,
    pub created_at: String,
    pub created_by: String,
}

/// A signal discovered by a connector during ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySignal {
    pub id: String,
    pub connector_id: String,
    pub signal_type: String,
    pub raw_data: serde_json::Value,
    pub detected_at: String,
    pub strength: f64,
}
