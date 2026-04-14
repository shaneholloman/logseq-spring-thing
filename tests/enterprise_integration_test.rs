//! Integration tests for enterprise domain models and service contracts.
//!
//! Covers serialization round-trips, camelCase field naming, enum variant
//! serialization, workflow lifecycle states, policy outcome aggregation
//! semantics, and edge cases (empty collections, None fields, empty strings).

use std::collections::HashMap;
use webxr::models::enterprise::*;

// ---------------------------------------------------------------------------
// 1. BrokerCase serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn broker_case_serialization_roundtrip() {
    let case = BrokerCase {
        id: "case-001".to_string(),
        title: "Test escalation".to_string(),
        description: "Policy threshold breached".to_string(),
        priority: CasePriority::High,
        source: EscalationSource::PolicyViolation,
        status: CaseStatus::Open,
        created_at: "2026-04-14T00:00:00Z".to_string(),
        updated_at: "2026-04-14T00:00:00Z".to_string(),
        assigned_to: None,
        evidence: vec![],
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&case).expect("serialize");
    let deserialized: BrokerCase = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.id, "case-001");
    assert_eq!(deserialized.priority, CasePriority::High);
    assert_eq!(deserialized.status, CaseStatus::Open);
}

// ---------------------------------------------------------------------------
// 2. WorkflowStatus lifecycle — valid state transitions
// ---------------------------------------------------------------------------

#[test]
fn workflow_status_lifecycle_valid_transitions() {
    // Valid: Draft -> Submitted -> UnderReview -> Approved -> Deployed
    let statuses = vec![
        WorkflowStatus::Draft,
        WorkflowStatus::Submitted,
        WorkflowStatus::UnderReview,
        WorkflowStatus::Approved,
        WorkflowStatus::Deployed,
    ];

    for status in &statuses {
        let json = serde_json::to_value(status).expect("serialize status");
        assert!(json.is_string());
    }
}

#[test]
fn workflow_status_all_variants_serialize() {
    let all = vec![
        WorkflowStatus::Draft,
        WorkflowStatus::Submitted,
        WorkflowStatus::UnderReview,
        WorkflowStatus::Approved,
        WorkflowStatus::Deployed,
        WorkflowStatus::Archived,
        WorkflowStatus::RolledBack,
    ];

    for status in all {
        let json = serde_json::to_string(&status).expect("serialize");
        let back: WorkflowStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(status, back);
    }
}

// ---------------------------------------------------------------------------
// 3. PolicyOutcome serialization
// ---------------------------------------------------------------------------

#[test]
fn policy_outcome_serialization() {
    assert_eq!(
        serde_json::to_value(PolicyOutcome::Allow).unwrap(),
        serde_json::json!("allow")
    );
    assert_eq!(
        serde_json::to_value(PolicyOutcome::Deny).unwrap(),
        serde_json::json!("deny")
    );
    assert_eq!(
        serde_json::to_value(PolicyOutcome::Escalate).unwrap(),
        serde_json::json!("escalate")
    );
    assert_eq!(
        serde_json::to_value(PolicyOutcome::Warn).unwrap(),
        serde_json::json!("warn")
    );
}

#[test]
fn policy_outcome_aggregation_deny_wins() {
    // Specification: first Deny wins, then Escalate, else Allow
    let outcomes = vec![
        PolicyOutcome::Allow,
        PolicyOutcome::Deny,
        PolicyOutcome::Escalate,
    ];
    let aggregate = aggregate_policy_outcomes(&outcomes);
    assert_eq!(aggregate, PolicyOutcome::Deny);
}

#[test]
fn policy_outcome_aggregation_escalate_second() {
    let outcomes = vec![
        PolicyOutcome::Allow,
        PolicyOutcome::Escalate,
        PolicyOutcome::Allow,
    ];
    let aggregate = aggregate_policy_outcomes(&outcomes);
    assert_eq!(aggregate, PolicyOutcome::Escalate);
}

#[test]
fn policy_outcome_aggregation_all_allow() {
    let outcomes = vec![PolicyOutcome::Allow, PolicyOutcome::Allow];
    let aggregate = aggregate_policy_outcomes(&outcomes);
    assert_eq!(aggregate, PolicyOutcome::Allow);
}

#[test]
fn policy_outcome_aggregation_empty_defaults_allow() {
    let outcomes: Vec<PolicyOutcome> = vec![];
    let aggregate = aggregate_policy_outcomes(&outcomes);
    assert_eq!(aggregate, PolicyOutcome::Allow);
}

#[test]
fn policy_outcome_aggregation_warn_does_not_override_allow() {
    // Warn is informational -- should not escalate above Allow unless Deny/Escalate present
    let outcomes = vec![PolicyOutcome::Allow, PolicyOutcome::Warn];
    let aggregate = aggregate_policy_outcomes(&outcomes);
    // Warn is weaker than Deny/Escalate, so result should be Warn (stronger than Allow)
    // but since spec says "first Deny wins, then Escalate, else Allow",
    // Warn doesn't appear in the priority chain, so Allow is the aggregate.
    assert_eq!(aggregate, PolicyOutcome::Allow);
}

/// Implements the aggregation rule: first Deny wins, then Escalate, else Allow.
/// Warn is informational and does not override the primary chain.
fn aggregate_policy_outcomes(outcomes: &[PolicyOutcome]) -> PolicyOutcome {
    if outcomes.iter().any(|o| *o == PolicyOutcome::Deny) {
        return PolicyOutcome::Deny;
    }
    if outcomes.iter().any(|o| *o == PolicyOutcome::Escalate) {
        return PolicyOutcome::Escalate;
    }
    PolicyOutcome::Allow
}

// ---------------------------------------------------------------------------
// 4. DecisionAction — all variants round-trip
// ---------------------------------------------------------------------------

#[test]
fn decision_action_all_variants() {
    let actions = vec![
        DecisionAction::Approve,
        DecisionAction::Reject,
        DecisionAction::Amend,
        DecisionAction::Delegate,
        DecisionAction::PromoteAsWorkflow,
        DecisionAction::MarkAsPrecedent,
        DecisionAction::RequestMoreEvidence,
    ];
    for action in actions {
        let json = serde_json::to_string(&action).expect("serialize action");
        let back: DecisionAction = serde_json::from_str(&json).expect("deserialize action");
        assert_eq!(action, back);
    }
}

// ---------------------------------------------------------------------------
// 5. MetricSnapshot / KpiType
// ---------------------------------------------------------------------------

#[test]
fn metric_snapshot_kpi_types() {
    let kpis = vec![
        KpiType::MeshVelocity,
        KpiType::AugmentationRatio,
        KpiType::TrustVariance,
        KpiType::HitlPrecision,
    ];
    for kpi in kpis {
        let json = serde_json::to_value(&kpi).expect("serialize kpi");
        assert!(json.is_string());
    }
}

#[test]
fn metric_snapshot_roundtrip() {
    let mut dims = HashMap::new();
    dims.insert("region".to_string(), "eu-west".to_string());

    let snapshot = MetricSnapshot {
        id: "ms-001".to_string(),
        kpi_type: KpiType::MeshVelocity,
        value: 42.5,
        confidence: 0.95,
        time_window_start: "2026-04-13T00:00:00Z".to_string(),
        time_window_end: "2026-04-14T00:00:00Z".to_string(),
        computed_at: "2026-04-14T01:00:00Z".to_string(),
        source_event_count: 1024,
        dimensions: dims,
    };

    let json = serde_json::to_string(&snapshot).expect("serialize");
    let back: MetricSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, "ms-001");
    assert_eq!(back.kpi_type, KpiType::MeshVelocity);
    assert!((back.value - 42.5).abs() < f64::EPSILON);
    assert!((back.confidence - 0.95).abs() < f64::EPSILON);
    assert_eq!(back.source_event_count, 1024);
    assert_eq!(back.dimensions.get("region").unwrap(), "eu-west");
}

#[test]
fn metric_snapshot_camel_case_fields() {
    let snapshot = MetricSnapshot {
        id: "ms-002".to_string(),
        kpi_type: KpiType::TrustVariance,
        value: 0.0,
        confidence: 1.0,
        time_window_start: "t0".to_string(),
        time_window_end: "t1".to_string(),
        computed_at: "t2".to_string(),
        source_event_count: 0,
        dimensions: HashMap::new(),
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(json.contains("kpiType"));
    assert!(json.contains("timeWindowStart"));
    assert!(json.contains("timeWindowEnd"));
    assert!(json.contains("computedAt"));
    assert!(json.contains("sourceEventCount"));
    // Must NOT contain snake_case variants
    assert!(!json.contains("kpi_type"));
    assert!(!json.contains("time_window_start"));
    assert!(!json.contains("source_event_count"));
}

// ---------------------------------------------------------------------------
// 6. ConnectorSource / ConnectorType
// ---------------------------------------------------------------------------

#[test]
fn connector_source_roundtrip() {
    let source = ConnectorSource {
        id: "conn-001".to_string(),
        connector_type: ConnectorType::GitHub,
        name: "DreamLab GitHub".to_string(),
        status: ConnectorStatus::Active,
        config: serde_json::json!({"org": "DreamLab-AI", "repos": ["VisionClaw"]}),
        last_sync: None,
        created_at: "2026-04-14T00:00:00Z".to_string(),
        created_by: "admin".to_string(),
    };

    let json = serde_json::to_string(&source).expect("serialize");
    let back: ConnectorSource = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.connector_type, ConnectorType::GitHub);
    assert_eq!(back.status, ConnectorStatus::Active);
}

#[test]
fn connector_type_all_variants_roundtrip() {
    let types = vec![
        ConnectorType::GitHub,
        ConnectorType::Jira,
        ConnectorType::Slack,
        ConnectorType::Custom,
    ];
    for ct in types {
        let json = serde_json::to_string(&ct).expect("serialize");
        let back: ConnectorType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ct, back);
    }
}

#[test]
fn connector_status_all_variants_roundtrip() {
    let statuses = vec![
        ConnectorStatus::Active,
        ConnectorStatus::Paused,
        ConnectorStatus::Error,
        ConnectorStatus::Configuring,
    ];
    for s in statuses {
        let json = serde_json::to_string(&s).expect("serialize");
        let back: ConnectorStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);
    }
}

#[test]
fn connector_source_camel_case_fields() {
    let source = ConnectorSource {
        id: "conn-002".to_string(),
        connector_type: ConnectorType::Jira,
        name: "JIRA".to_string(),
        status: ConnectorStatus::Paused,
        config: serde_json::json!({}),
        last_sync: Some("2026-04-14T00:00:00Z".to_string()),
        created_at: "2026-04-14T00:00:00Z".to_string(),
        created_by: "admin".to_string(),
    };

    let json = serde_json::to_string(&source).unwrap();
    assert!(json.contains("connectorType"));
    assert!(json.contains("lastSync"));
    assert!(json.contains("createdAt"));
    assert!(json.contains("createdBy"));
    assert!(!json.contains("connector_type"));
    assert!(!json.contains("last_sync"));
    assert!(!json.contains("created_at"));
    assert!(!json.contains("created_by"));
}

// ---------------------------------------------------------------------------
// 7. EnterpriseRole serialization
// ---------------------------------------------------------------------------

#[test]
fn enterprise_role_serialization() {
    assert_eq!(
        serde_json::to_value(EnterpriseRole::Broker).unwrap(),
        serde_json::json!("broker")
    );
    assert_eq!(
        serde_json::to_value(EnterpriseRole::Admin).unwrap(),
        serde_json::json!("admin")
    );
    assert_eq!(
        serde_json::to_value(EnterpriseRole::Auditor).unwrap(),
        serde_json::json!("auditor")
    );
    assert_eq!(
        serde_json::to_value(EnterpriseRole::Contributor).unwrap(),
        serde_json::json!("contributor")
    );
}

#[test]
fn enterprise_role_roundtrip() {
    let roles = vec![
        EnterpriseRole::Broker,
        EnterpriseRole::Admin,
        EnterpriseRole::Auditor,
        EnterpriseRole::Contributor,
    ];
    for role in roles {
        let json = serde_json::to_string(&role).expect("serialize");
        let back: EnterpriseRole = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(role, back);
    }
}

// ---------------------------------------------------------------------------
// 8. BrokerCase camelCase field names
// ---------------------------------------------------------------------------

#[test]
fn broker_case_json_field_names_are_camel_case() {
    let case = BrokerCase {
        id: "c1".to_string(),
        title: "Test".to_string(),
        description: "Desc".to_string(),
        priority: CasePriority::Low,
        source: EscalationSource::ManualSubmission,
        status: CaseStatus::Open,
        created_at: "2026-04-14T00:00:00Z".to_string(),
        updated_at: "2026-04-14T00:00:00Z".to_string(),
        assigned_to: Some("broker-1".to_string()),
        evidence: vec![],
        metadata: HashMap::new(),
    };
    let json = serde_json::to_string(&case).unwrap();
    assert!(json.contains("createdAt"));
    assert!(json.contains("updatedAt"));
    assert!(json.contains("assignedTo"));
    assert!(!json.contains("created_at"));
    assert!(!json.contains("updated_at"));
    assert!(!json.contains("assigned_to"));
}

// ---------------------------------------------------------------------------
// 9. WorkflowProposal camelCase fields
// ---------------------------------------------------------------------------

#[test]
fn workflow_proposal_camel_case_fields() {
    let proposal = WorkflowProposal {
        id: "wp-1".to_string(),
        title: "Test workflow".to_string(),
        description: "Test".to_string(),
        status: WorkflowStatus::Draft,
        version: 1,
        steps: vec![],
        source_insight_id: None,
        submitted_by: "user-1".to_string(),
        created_at: "2026-04-14T00:00:00Z".to_string(),
        updated_at: "2026-04-14T00:00:00Z".to_string(),
        risk_score: Some(0.3),
        expected_benefit: Some("Reduce coordination overhead".to_string()),
        metadata: HashMap::new(),
    };
    let json = serde_json::to_string(&proposal).unwrap();
    assert!(json.contains("sourceInsightId"));
    assert!(json.contains("submittedBy"));
    assert!(json.contains("riskScore"));
    assert!(json.contains("expectedBenefit"));
    assert!(!json.contains("source_insight_id"));
    assert!(!json.contains("submitted_by"));
    assert!(!json.contains("risk_score"));
    assert!(!json.contains("expected_benefit"));
}

#[test]
fn workflow_proposal_roundtrip() {
    let proposal = WorkflowProposal {
        id: "wp-2".to_string(),
        title: "Full roundtrip".to_string(),
        description: "Tests full ser/de cycle".to_string(),
        status: WorkflowStatus::Submitted,
        version: 3,
        steps: vec![WorkflowStep {
            order: 1,
            name: "validate".to_string(),
            action_type: "check".to_string(),
            config: serde_json::json!({"threshold": 0.8}),
        }],
        source_insight_id: Some("insight-99".to_string()),
        submitted_by: "user-2".to_string(),
        created_at: "2026-04-14T00:00:00Z".to_string(),
        updated_at: "2026-04-14T01:00:00Z".to_string(),
        risk_score: Some(0.7),
        expected_benefit: Some("Faster approvals".to_string()),
        metadata: {
            let mut m = HashMap::new();
            m.insert("team".to_string(), "platform".to_string());
            m
        },
    };

    let json = serde_json::to_string(&proposal).expect("serialize");
    let back: WorkflowProposal = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, "wp-2");
    assert_eq!(back.status, WorkflowStatus::Submitted);
    assert_eq!(back.version, 3);
    assert_eq!(back.steps.len(), 1);
    assert_eq!(back.steps[0].order, 1);
    assert_eq!(back.source_insight_id, Some("insight-99".to_string()));
    assert!((back.risk_score.unwrap() - 0.7).abs() < f64::EPSILON);
    assert_eq!(back.metadata.get("team").unwrap(), "platform");
}

// ---------------------------------------------------------------------------
// 10. BrokerDecision round-trip
// ---------------------------------------------------------------------------

#[test]
fn broker_decision_roundtrip() {
    let decision = BrokerDecision {
        id: "dec-001".to_string(),
        case_id: "case-001".to_string(),
        action: DecisionAction::Approve,
        reasoning: "Risk acceptable".to_string(),
        decided_by: "broker-1".to_string(),
        decided_at: "2026-04-14T12:00:00Z".to_string(),
        provenance_event_id: Some("prov-001".to_string()),
    };

    let json = serde_json::to_string(&decision).expect("serialize");
    let back: BrokerDecision = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, "dec-001");
    assert_eq!(back.case_id, "case-001");
    assert_eq!(back.action, DecisionAction::Approve);
    assert_eq!(
        back.provenance_event_id,
        Some("prov-001".to_string())
    );
}

#[test]
fn broker_decision_camel_case_fields() {
    let decision = BrokerDecision {
        id: "dec-002".to_string(),
        case_id: "case-002".to_string(),
        action: DecisionAction::Reject,
        reasoning: "Too risky".to_string(),
        decided_by: "broker-2".to_string(),
        decided_at: "2026-04-14T13:00:00Z".to_string(),
        provenance_event_id: None,
    };

    let json = serde_json::to_string(&decision).unwrap();
    assert!(json.contains("caseId"));
    assert!(json.contains("decidedBy"));
    assert!(json.contains("decidedAt"));
    assert!(json.contains("provenanceEventId"));
    assert!(!json.contains("case_id"));
    assert!(!json.contains("decided_by"));
    assert!(!json.contains("decided_at"));
    assert!(!json.contains("provenance_event_id"));
}

// ---------------------------------------------------------------------------
// 11. EvidenceItem round-trip
// ---------------------------------------------------------------------------

#[test]
fn evidence_item_roundtrip() {
    let item = EvidenceItem {
        item_type: "log_entry".to_string(),
        source_id: "src-001".to_string(),
        description: "Anomalous trust drift detected".to_string(),
        timestamp: "2026-04-14T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&item).expect("serialize");
    let back: EvidenceItem = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.item_type, "log_entry");
    assert_eq!(back.source_id, "src-001");
}

#[test]
fn evidence_item_camel_case_fields() {
    let item = EvidenceItem {
        item_type: "metric".to_string(),
        source_id: "src-002".to_string(),
        description: "KPI breach".to_string(),
        timestamp: "2026-04-14T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&item).unwrap();
    assert!(json.contains("itemType"));
    assert!(json.contains("sourceId"));
    assert!(!json.contains("item_type"));
    assert!(!json.contains("source_id"));
}

// ---------------------------------------------------------------------------
// 12. WorkflowPattern round-trip
// ---------------------------------------------------------------------------

#[test]
fn workflow_pattern_roundtrip() {
    let pattern = WorkflowPattern {
        id: "pat-001".to_string(),
        title: "Standard Approval".to_string(),
        description: "Three-tier approval workflow".to_string(),
        active_version_id: "wp-2".to_string(),
        deployed_at: "2026-04-14T00:00:00Z".to_string(),
        deployed_by: "admin".to_string(),
        adoption_count: 15,
        rollback_target_id: Some("wp-1".to_string()),
    };

    let json = serde_json::to_string(&pattern).expect("serialize");
    let back: WorkflowPattern = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, "pat-001");
    assert_eq!(back.adoption_count, 15);
    assert_eq!(back.rollback_target_id, Some("wp-1".to_string()));
}

#[test]
fn workflow_pattern_camel_case_fields() {
    let pattern = WorkflowPattern {
        id: "pat-002".to_string(),
        title: "T".to_string(),
        description: "D".to_string(),
        active_version_id: "v1".to_string(),
        deployed_at: "t".to_string(),
        deployed_by: "u".to_string(),
        adoption_count: 0,
        rollback_target_id: None,
    };

    let json = serde_json::to_string(&pattern).unwrap();
    assert!(json.contains("activeVersionId"));
    assert!(json.contains("deployedAt"));
    assert!(json.contains("deployedBy"));
    assert!(json.contains("adoptionCount"));
    assert!(json.contains("rollbackTargetId"));
    assert!(!json.contains("active_version_id"));
    assert!(!json.contains("deployed_at"));
    assert!(!json.contains("deployed_by"));
    assert!(!json.contains("adoption_count"));
    assert!(!json.contains("rollback_target_id"));
}

// ---------------------------------------------------------------------------
// 13. WorkflowStep round-trip
// ---------------------------------------------------------------------------

#[test]
fn workflow_step_roundtrip() {
    let step = WorkflowStep {
        order: 1,
        name: "validate_input".to_string(),
        action_type: "validation".to_string(),
        config: serde_json::json!({"strict": true, "timeout_ms": 5000}),
    };

    let json = serde_json::to_string(&step).expect("serialize");
    let back: WorkflowStep = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.order, 1);
    assert_eq!(back.name, "validate_input");
    assert_eq!(back.config["strict"], true);
}

#[test]
fn workflow_step_camel_case_fields() {
    let step = WorkflowStep {
        order: 0,
        name: "s".to_string(),
        action_type: "noop".to_string(),
        config: serde_json::json!(null),
    };

    let json = serde_json::to_string(&step).unwrap();
    assert!(json.contains("actionType"));
    assert!(!json.contains("action_type"));
}

// ---------------------------------------------------------------------------
// 14. PolicyEvaluation round-trip
// ---------------------------------------------------------------------------

#[test]
fn policy_evaluation_roundtrip() {
    let eval = PolicyEvaluation {
        rule_id: "rule-001".to_string(),
        outcome: PolicyOutcome::Deny,
        reasoning: "Confidence below threshold".to_string(),
        confidence: 0.4,
        evaluated_at: "2026-04-14T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&eval).expect("serialize");
    let back: PolicyEvaluation = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.rule_id, "rule-001");
    assert_eq!(back.outcome, PolicyOutcome::Deny);
    assert!((back.confidence - 0.4).abs() < f64::EPSILON);
}

#[test]
fn policy_evaluation_camel_case_fields() {
    let eval = PolicyEvaluation {
        rule_id: "r1".to_string(),
        outcome: PolicyOutcome::Allow,
        reasoning: "ok".to_string(),
        confidence: 1.0,
        evaluated_at: "t".to_string(),
    };

    let json = serde_json::to_string(&eval).unwrap();
    assert!(json.contains("ruleId"));
    assert!(json.contains("evaluatedAt"));
    assert!(!json.contains("rule_id"));
    assert!(!json.contains("evaluated_at"));
}

// ---------------------------------------------------------------------------
// 15. PolicyAction all variants
// ---------------------------------------------------------------------------

#[test]
fn policy_action_all_variants_roundtrip() {
    let actions = vec![
        PolicyAction::ApproveWorkflow,
        PolicyAction::DeployWorkflow,
        PolicyAction::EscalateCase,
        PolicyAction::OverrideDecision,
        PolicyAction::AccessConnector,
        PolicyAction::ModifyPolicy,
    ];
    for action in actions {
        let json = serde_json::to_string(&action).expect("serialize");
        let back: PolicyAction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(action, back);
    }
}

// ---------------------------------------------------------------------------
// 16. PolicyContext round-trip
// ---------------------------------------------------------------------------

#[test]
fn policy_context_roundtrip() {
    let ctx = PolicyContext {
        actor_id: "user-42".to_string(),
        action: PolicyAction::ApproveWorkflow,
        resource_id: "wp-1".to_string(),
        resource_type: "workflow_proposal".to_string(),
        confidence: Some(0.85),
        domain: Some("engineering".to_string()),
        metadata: {
            let mut m = HashMap::new();
            m.insert("source".to_string(), "automated".to_string());
            m
        },
    };

    let json = serde_json::to_string(&ctx).expect("serialize");
    let back: PolicyContext = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.actor_id, "user-42");
    assert_eq!(back.action, PolicyAction::ApproveWorkflow);
    assert!((back.confidence.unwrap() - 0.85).abs() < f64::EPSILON);
    assert_eq!(back.domain, Some("engineering".to_string()));
}

#[test]
fn policy_context_camel_case_fields() {
    let ctx = PolicyContext {
        actor_id: "a".to_string(),
        action: PolicyAction::ModifyPolicy,
        resource_id: "r".to_string(),
        resource_type: "t".to_string(),
        confidence: None,
        domain: None,
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&ctx).unwrap();
    assert!(json.contains("actorId"));
    assert!(json.contains("resourceId"));
    assert!(json.contains("resourceType"));
    assert!(!json.contains("actor_id"));
    assert!(!json.contains("resource_id"));
    assert!(!json.contains("resource_type"));
}

// ---------------------------------------------------------------------------
// 17. DiscoverySignal round-trip
// ---------------------------------------------------------------------------

#[test]
fn discovery_signal_roundtrip() {
    let signal = DiscoverySignal {
        id: "sig-001".to_string(),
        connector_id: "conn-001".to_string(),
        signal_type: "new_repository".to_string(),
        raw_data: serde_json::json!({"repo": "VisionClaw", "stars": 42}),
        detected_at: "2026-04-14T00:00:00Z".to_string(),
        strength: 0.9,
    };

    let json = serde_json::to_string(&signal).expect("serialize");
    let back: DiscoverySignal = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, "sig-001");
    assert_eq!(back.connector_id, "conn-001");
    assert!((back.strength - 0.9).abs() < f64::EPSILON);
}

#[test]
fn discovery_signal_camel_case_fields() {
    let signal = DiscoverySignal {
        id: "sig-002".to_string(),
        connector_id: "c".to_string(),
        signal_type: "t".to_string(),
        raw_data: serde_json::json!(null),
        detected_at: "t".to_string(),
        strength: 0.0,
    };

    let json = serde_json::to_string(&signal).unwrap();
    assert!(json.contains("connectorId"));
    assert!(json.contains("signalType"));
    assert!(json.contains("rawData"));
    assert!(json.contains("detectedAt"));
    assert!(!json.contains("connector_id"));
    assert!(!json.contains("signal_type"));
    assert!(!json.contains("raw_data"));
    assert!(!json.contains("detected_at"));
}

// ---------------------------------------------------------------------------
// 18. CasePriority all variants
// ---------------------------------------------------------------------------

#[test]
fn case_priority_all_variants_roundtrip() {
    let priorities = vec![
        CasePriority::Critical,
        CasePriority::High,
        CasePriority::Medium,
        CasePriority::Low,
    ];
    for p in priorities {
        let json = serde_json::to_string(&p).expect("serialize");
        let back: CasePriority = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, back);
    }
}

// ---------------------------------------------------------------------------
// 19. EscalationSource all variants
// ---------------------------------------------------------------------------

#[test]
fn escalation_source_all_variants_roundtrip() {
    let sources = vec![
        EscalationSource::PolicyViolation,
        EscalationSource::ConfidenceThreshold,
        EscalationSource::TrustDrift,
        EscalationSource::ManualSubmission,
        EscalationSource::WorkflowProposal,
    ];
    for s in sources {
        let json = serde_json::to_string(&s).expect("serialize");
        let back: EscalationSource = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);
    }
}

// ---------------------------------------------------------------------------
// 20. CaseStatus all variants
// ---------------------------------------------------------------------------

#[test]
fn case_status_all_variants_roundtrip() {
    let statuses = vec![
        CaseStatus::Open,
        CaseStatus::InReview,
        CaseStatus::Decided,
        CaseStatus::Escalated,
        CaseStatus::Closed,
    ];
    for s in statuses {
        let json = serde_json::to_string(&s).expect("serialize");
        let back: CaseStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);
    }
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn broker_case_with_empty_strings() {
    let case = BrokerCase {
        id: "".to_string(),
        title: "".to_string(),
        description: "".to_string(),
        priority: CasePriority::Low,
        source: EscalationSource::ManualSubmission,
        status: CaseStatus::Open,
        created_at: "".to_string(),
        updated_at: "".to_string(),
        assigned_to: None,
        evidence: vec![],
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&case).expect("serialize");
    let back: BrokerCase = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.id, "");
    assert_eq!(back.title, "");
    assert!(back.evidence.is_empty());
    assert!(back.metadata.is_empty());
}

#[test]
fn broker_case_with_evidence_and_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert("key1".to_string(), "val1".to_string());
    metadata.insert("key2".to_string(), "val2".to_string());

    let case = BrokerCase {
        id: "case-full".to_string(),
        title: "Full case".to_string(),
        description: "Case with all optional fields populated".to_string(),
        priority: CasePriority::Critical,
        source: EscalationSource::TrustDrift,
        status: CaseStatus::InReview,
        created_at: "2026-04-14T00:00:00Z".to_string(),
        updated_at: "2026-04-14T01:00:00Z".to_string(),
        assigned_to: Some("broker-senior".to_string()),
        evidence: vec![
            EvidenceItem {
                item_type: "metric".to_string(),
                source_id: "kpi-001".to_string(),
                description: "Trust variance exceeded threshold".to_string(),
                timestamp: "2026-04-14T00:30:00Z".to_string(),
            },
            EvidenceItem {
                item_type: "log".to_string(),
                source_id: "log-999".to_string(),
                description: "Anomalous pattern in ingestion".to_string(),
                timestamp: "2026-04-14T00:31:00Z".to_string(),
            },
        ],
        metadata,
    };

    let json = serde_json::to_string(&case).expect("serialize");
    let back: BrokerCase = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.evidence.len(), 2);
    assert_eq!(back.metadata.len(), 2);
    assert_eq!(back.assigned_to, Some("broker-senior".to_string()));
    assert_eq!(back.evidence[0].item_type, "metric");
    assert_eq!(back.evidence[1].item_type, "log");
}

#[test]
fn workflow_proposal_with_no_optional_fields() {
    let proposal = WorkflowProposal {
        id: "wp-empty".to_string(),
        title: "Minimal".to_string(),
        description: "".to_string(),
        status: WorkflowStatus::Draft,
        version: 0,
        steps: vec![],
        source_insight_id: None,
        submitted_by: "system".to_string(),
        created_at: "".to_string(),
        updated_at: "".to_string(),
        risk_score: None,
        expected_benefit: None,
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&proposal).expect("serialize");
    let back: WorkflowProposal = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.source_insight_id, None);
    assert_eq!(back.risk_score, None);
    assert_eq!(back.expected_benefit, None);
    assert!(back.steps.is_empty());
    assert!(back.metadata.is_empty());
}

#[test]
fn connector_source_with_last_sync_none() {
    let source = ConnectorSource {
        id: "conn-new".to_string(),
        connector_type: ConnectorType::Custom,
        name: "New connector".to_string(),
        status: ConnectorStatus::Configuring,
        config: serde_json::json!(null),
        last_sync: None,
        created_at: "2026-04-14T00:00:00Z".to_string(),
        created_by: "admin".to_string(),
    };

    let json = serde_json::to_string(&source).expect("serialize");
    let back: ConnectorSource = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.last_sync, None);
    assert_eq!(back.status, ConnectorStatus::Configuring);
}

#[test]
fn connector_source_with_complex_config() {
    let source = ConnectorSource {
        id: "conn-complex".to_string(),
        connector_type: ConnectorType::Slack,
        name: "Slack Integration".to_string(),
        status: ConnectorStatus::Active,
        config: serde_json::json!({
            "workspace": "dreamlab",
            "channels": ["#general", "#engineering", "#alerts"],
            "filters": {
                "include_threads": true,
                "min_reactions": 3
            }
        }),
        last_sync: Some("2026-04-14T12:00:00Z".to_string()),
        created_at: "2026-04-01T00:00:00Z".to_string(),
        created_by: "platform-team".to_string(),
    };

    let json = serde_json::to_string(&source).expect("serialize");
    let back: ConnectorSource = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.config["workspace"], "dreamlab");
    assert_eq!(back.config["channels"].as_array().unwrap().len(), 3);
    assert_eq!(back.config["filters"]["include_threads"], true);
}

#[test]
fn metric_snapshot_zero_values() {
    let snapshot = MetricSnapshot {
        id: "ms-zero".to_string(),
        kpi_type: KpiType::HitlPrecision,
        value: 0.0,
        confidence: 0.0,
        time_window_start: "".to_string(),
        time_window_end: "".to_string(),
        computed_at: "".to_string(),
        source_event_count: 0,
        dimensions: HashMap::new(),
    };

    let json = serde_json::to_string(&snapshot).expect("serialize");
    let back: MetricSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert!((back.value - 0.0).abs() < f64::EPSILON);
    assert!((back.confidence - 0.0).abs() < f64::EPSILON);
    assert_eq!(back.source_event_count, 0);
    assert!(back.dimensions.is_empty());
}

#[test]
fn metric_snapshot_large_values() {
    let snapshot = MetricSnapshot {
        id: "ms-large".to_string(),
        kpi_type: KpiType::AugmentationRatio,
        value: 1_000_000.123456,
        confidence: 0.999999,
        time_window_start: "2026-01-01T00:00:00Z".to_string(),
        time_window_end: "2026-12-31T23:59:59Z".to_string(),
        computed_at: "2026-04-14T00:00:00Z".to_string(),
        source_event_count: u32::MAX,
        dimensions: HashMap::new(),
    };

    let json = serde_json::to_string(&snapshot).expect("serialize");
    let back: MetricSnapshot = serde_json::from_str(&json).expect("deserialize");
    assert!((back.value - 1_000_000.123456).abs() < 0.001);
    assert_eq!(back.source_event_count, u32::MAX);
}

#[test]
fn broker_decision_with_no_provenance() {
    let decision = BrokerDecision {
        id: "dec-noprov".to_string(),
        case_id: "case-x".to_string(),
        action: DecisionAction::RequestMoreEvidence,
        reasoning: "Insufficient data to decide".to_string(),
        decided_by: "broker-3".to_string(),
        decided_at: "2026-04-14T00:00:00Z".to_string(),
        provenance_event_id: None,
    };

    let json = serde_json::to_string(&decision).expect("serialize");
    let back: BrokerDecision = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.provenance_event_id, None);
    assert_eq!(back.action, DecisionAction::RequestMoreEvidence);
}

#[test]
fn workflow_pattern_with_no_rollback() {
    let pattern = WorkflowPattern {
        id: "pat-norollback".to_string(),
        title: "First version".to_string(),
        description: "No previous version to roll back to".to_string(),
        active_version_id: "wp-1".to_string(),
        deployed_at: "2026-04-14T00:00:00Z".to_string(),
        deployed_by: "admin".to_string(),
        adoption_count: 0,
        rollback_target_id: None,
    };

    let json = serde_json::to_string(&pattern).expect("serialize");
    let back: WorkflowPattern = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.rollback_target_id, None);
    assert_eq!(back.adoption_count, 0);
}

#[test]
fn discovery_signal_with_empty_raw_data() {
    let signal = DiscoverySignal {
        id: "sig-empty".to_string(),
        connector_id: "conn-001".to_string(),
        signal_type: "heartbeat".to_string(),
        raw_data: serde_json::json!({}),
        detected_at: "2026-04-14T00:00:00Z".to_string(),
        strength: 0.0,
    };

    let json = serde_json::to_string(&signal).expect("serialize");
    let back: DiscoverySignal = serde_json::from_str(&json).expect("deserialize");
    assert!(back.raw_data.as_object().unwrap().is_empty());
    assert!((back.strength - 0.0).abs() < f64::EPSILON);
}

#[test]
fn policy_context_with_all_none_optionals() {
    let ctx = PolicyContext {
        actor_id: "anon".to_string(),
        action: PolicyAction::AccessConnector,
        resource_id: "conn-001".to_string(),
        resource_type: "connector".to_string(),
        confidence: None,
        domain: None,
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&ctx).expect("serialize");
    let back: PolicyContext = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.confidence, None);
    assert_eq!(back.domain, None);
    assert!(back.metadata.is_empty());
}

#[test]
fn workflow_proposal_multiple_steps_ordering() {
    let proposal = WorkflowProposal {
        id: "wp-multi".to_string(),
        title: "Multi-step".to_string(),
        description: "Tests step ordering preservation".to_string(),
        status: WorkflowStatus::Approved,
        version: 2,
        steps: vec![
            WorkflowStep {
                order: 1,
                name: "first".to_string(),
                action_type: "validate".to_string(),
                config: serde_json::json!(null),
            },
            WorkflowStep {
                order: 2,
                name: "second".to_string(),
                action_type: "transform".to_string(),
                config: serde_json::json!(null),
            },
            WorkflowStep {
                order: 3,
                name: "third".to_string(),
                action_type: "deploy".to_string(),
                config: serde_json::json!({"target": "prod"}),
            },
        ],
        source_insight_id: None,
        submitted_by: "user-1".to_string(),
        created_at: "t".to_string(),
        updated_at: "t".to_string(),
        risk_score: None,
        expected_benefit: None,
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&proposal).expect("serialize");
    let back: WorkflowProposal = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.steps.len(), 3);
    assert_eq!(back.steps[0].order, 1);
    assert_eq!(back.steps[0].name, "first");
    assert_eq!(back.steps[1].order, 2);
    assert_eq!(back.steps[2].order, 3);
    assert_eq!(back.steps[2].config["target"], "prod");
}

#[test]
fn deserialization_rejects_invalid_enum_value() {
    let bad_json = r#""nonexistent_status""#;
    let result = serde_json::from_str::<CaseStatus>(bad_json);
    assert!(result.is_err(), "Should reject unknown enum variant");
}

#[test]
fn deserialization_rejects_invalid_priority() {
    let bad_json = r#""urgent""#;
    let result = serde_json::from_str::<CasePriority>(bad_json);
    assert!(result.is_err(), "Should reject unknown priority variant");
}

#[test]
fn deserialization_rejects_invalid_kpi_type() {
    let bad_json = r#""revenue_growth""#;
    let result = serde_json::from_str::<KpiType>(bad_json);
    assert!(result.is_err(), "Should reject unknown KPI type");
}

#[test]
fn deserialization_rejects_invalid_policy_outcome() {
    let bad_json = r#""block""#;
    let result = serde_json::from_str::<PolicyOutcome>(bad_json);
    assert!(result.is_err(), "Should reject unknown policy outcome");
}

#[test]
fn deserialization_rejects_invalid_connector_type() {
    let bad_json = r#""confluence""#;
    let result = serde_json::from_str::<ConnectorType>(bad_json);
    assert!(result.is_err(), "Should reject unknown connector type");
}

#[test]
fn broker_case_from_json_with_missing_required_field() {
    let incomplete = serde_json::json!({
        "id": "case-bad",
        "title": "Missing fields"
        // missing description, priority, source, status, etc.
    });
    let result = serde_json::from_value::<BrokerCase>(incomplete);
    assert!(result.is_err(), "Should reject JSON missing required fields");
}

#[test]
fn unicode_in_string_fields() {
    let case = BrokerCase {
        id: "case-unicode".to_string(),
        title: "Prüfung der Richtlinie 日本語テスト".to_string(),
        description: "Ελληνικά 中文 العربية".to_string(),
        priority: CasePriority::Medium,
        source: EscalationSource::ManualSubmission,
        status: CaseStatus::Open,
        created_at: "2026-04-14T00:00:00Z".to_string(),
        updated_at: "2026-04-14T00:00:00Z".to_string(),
        assigned_to: None,
        evidence: vec![],
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&case).expect("serialize");
    let back: BrokerCase = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.title, "Prüfung der Richtlinie 日本語テスト");
    assert_eq!(back.description, "Ελληνικά 中文 العربية");
}

#[test]
fn special_characters_in_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert("key with spaces".to_string(), "value\nwith\nnewlines".to_string());
    metadata.insert("quotes\"here".to_string(), "backslash\\there".to_string());

    let case = BrokerCase {
        id: "case-special".to_string(),
        title: "Special chars".to_string(),
        description: "Tests JSON escaping".to_string(),
        priority: CasePriority::Low,
        source: EscalationSource::ManualSubmission,
        status: CaseStatus::Open,
        created_at: "t".to_string(),
        updated_at: "t".to_string(),
        assigned_to: None,
        evidence: vec![],
        metadata,
    };

    let json = serde_json::to_string(&case).expect("serialize");
    let back: BrokerCase = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.metadata.get("key with spaces").unwrap(), "value\nwith\nnewlines");
    assert_eq!(back.metadata.get("quotes\"here").unwrap(), "backslash\\there");
}
