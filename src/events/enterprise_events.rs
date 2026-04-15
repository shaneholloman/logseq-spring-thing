//! Enterprise domain events for audit trail (ADR-040..045).
//!
//! These events capture state transitions in the enterprise governance layer:
//! broker case lifecycle, workflow proposals, and policy evaluations.
//!
//! Events are emitted as structured log entries (`log::info!`) for now.
//! The `EventBus` integration will be added in a follow-up phase.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::types::DomainEvent;
use crate::models::enterprise::*;
use crate::utils::json::to_json;

// ---------------------------------------------------------------------------
// Macro for DomainEvent impl (mirrors domain_events.rs pattern)
// ---------------------------------------------------------------------------

macro_rules! impl_enterprise_event {
    ($type:ty, $event_type:expr, $aggregate_type:expr, $id_field:ident) => {
        impl DomainEvent for $type {
            fn event_type(&self) -> &'static str {
                $event_type
            }
            fn aggregate_id(&self) -> &str {
                &self.$id_field
            }
            fn timestamp(&self) -> DateTime<Utc> {
                self.timestamp
            }
            fn aggregate_type(&self) -> &'static str {
                $aggregate_type
            }
            fn to_json_string(&self) -> Result<String, serde_json::Error> {
                to_json(self).map_err(|e| {
                    let msg = format!("JSON serialization error: {}", e);
                    serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, msg))
                })
            }
        }
    };
}

// ==================== Broker Events (ADR-041) ====================

/// Emitted when a new broker case is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseCreatedEvent {
    pub case_id: String,
    pub title: String,
    pub priority: CasePriority,
    pub source: EscalationSource,
    pub created_by: String,
    pub timestamp: DateTime<Utc>,
}

impl_enterprise_event!(CaseCreatedEvent, "CaseCreated", "BrokerCase", case_id);

/// Emitted when a broker records a decision on a case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseDecidedEvent {
    pub case_id: String,
    pub decision_id: String,
    pub action: DecisionAction,
    pub decided_by: String,
    pub timestamp: DateTime<Utc>,
}

impl_enterprise_event!(CaseDecidedEvent, "CaseDecided", "BrokerCase", case_id);

// ==================== Workflow Events (ADR-042) ====================

/// Emitted when a workflow proposal is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalCreatedEvent {
    pub proposal_id: String,
    pub title: String,
    pub submitted_by: String,
    pub timestamp: DateTime<Utc>,
}

impl_enterprise_event!(
    ProposalCreatedEvent,
    "ProposalCreated",
    "WorkflowProposal",
    proposal_id
);

/// Emitted when a proposal transitions between lifecycle statuses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalStatusChangedEvent {
    pub proposal_id: String,
    pub from_status: WorkflowStatus,
    pub to_status: WorkflowStatus,
    pub changed_by: String,
    pub timestamp: DateTime<Utc>,
}

impl_enterprise_event!(
    ProposalStatusChangedEvent,
    "ProposalStatusChanged",
    "WorkflowProposal",
    proposal_id
);

/// Emitted when a proposal is promoted to a deployed workflow pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPromotedEvent {
    pub proposal_id: String,
    pub pattern_id: String,
    pub promoted_by: String,
    pub timestamp: DateTime<Utc>,
}

impl_enterprise_event!(
    WorkflowPromotedEvent,
    "WorkflowPromoted",
    "WorkflowProposal",
    proposal_id
);

// ==================== Policy Events (ADR-045) ====================

/// Emitted after a policy evaluation completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluatedEvent {
    pub evaluation_id: String,
    pub context_action: String,
    pub outcome: PolicyOutcome,
    pub rule_count: usize,
    pub actor_id: String,
    pub timestamp: DateTime<Utc>,
}

impl_enterprise_event!(
    PolicyEvaluatedEvent,
    "PolicyEvaluated",
    "PolicyEngine",
    evaluation_id
);

// ---------------------------------------------------------------------------
// Helper: emit enterprise event as structured log
// ---------------------------------------------------------------------------

/// Logs an enterprise event as a structured JSON `info!` line.
/// This is the interim emit mechanism until the EventBus integration is wired.
pub fn emit_enterprise_event<E: DomainEvent + Serialize>(event: &E) {
    match serde_json::to_string(event) {
        Ok(json) => {
            log::info!(
                "ENTERPRISE_EVENT type={} aggregate={} id={} payload={}",
                event.event_type(),
                event.aggregate_type(),
                event.aggregate_id(),
                json
            );
        }
        Err(e) => {
            log::warn!(
                "Failed to serialize enterprise event {}: {}",
                event.event_type(),
                e
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::time;

    #[test]
    fn test_case_created_event() {
        let event = CaseCreatedEvent {
            case_id: "case-001".to_string(),
            title: "Test escalation".to_string(),
            priority: CasePriority::High,
            source: EscalationSource::ManualSubmission,
            created_by: "user-abc".to_string(),
            timestamp: time::now(),
        };
        assert_eq!(event.event_type(), "CaseCreated");
        assert_eq!(event.aggregate_type(), "BrokerCase");
        assert_eq!(event.aggregate_id(), "case-001");
    }

    #[test]
    fn test_case_decided_event() {
        let event = CaseDecidedEvent {
            case_id: "case-001".to_string(),
            decision_id: "dec-001".to_string(),
            action: DecisionAction::Approve,
            decided_by: "broker-xyz".to_string(),
            timestamp: time::now(),
        };
        assert_eq!(event.event_type(), "CaseDecided");
        assert_eq!(event.aggregate_id(), "case-001");
    }

    #[test]
    fn test_proposal_created_event() {
        let event = ProposalCreatedEvent {
            proposal_id: "wp-001".to_string(),
            title: "New workflow".to_string(),
            submitted_by: "user-abc".to_string(),
            timestamp: time::now(),
        };
        assert_eq!(event.event_type(), "ProposalCreated");
        assert_eq!(event.aggregate_type(), "WorkflowProposal");
    }

    #[test]
    fn test_proposal_status_changed_event() {
        let event = ProposalStatusChangedEvent {
            proposal_id: "wp-001".to_string(),
            from_status: WorkflowStatus::Draft,
            to_status: WorkflowStatus::Submitted,
            changed_by: "user-abc".to_string(),
            timestamp: time::now(),
        };
        assert_eq!(event.event_type(), "ProposalStatusChanged");
    }

    #[test]
    fn test_policy_evaluated_event() {
        let event = PolicyEvaluatedEvent {
            evaluation_id: "eval-001".to_string(),
            context_action: "approve_workflow".to_string(),
            outcome: PolicyOutcome::Allow,
            rule_count: 3,
            actor_id: "user-abc".to_string(),
            timestamp: time::now(),
        };
        assert_eq!(event.event_type(), "PolicyEvaluated");
        assert_eq!(event.aggregate_type(), "PolicyEngine");
    }

    #[test]
    fn test_event_serialization() {
        let event = CaseCreatedEvent {
            case_id: "case-ser".to_string(),
            title: "Serialization test".to_string(),
            priority: CasePriority::Medium,
            source: EscalationSource::PolicyViolation,
            created_by: "test".to_string(),
            timestamp: time::now(),
        };
        let json = event.to_json_string();
        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.contains("case-ser"));
        assert!(json_str.contains("medium"));
    }
}
