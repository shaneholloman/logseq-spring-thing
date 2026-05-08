use crate::domain::broker::broker_decision::DecisionOutcome;
use crate::domain::broker::{
    BrokerCase as DomainBrokerCase, CaseCategory, CaseState, DecisionHistoryEntry,
};
use crate::models::enterprise::{
    BrokerCase as LegacyBrokerCase, BrokerDecision, CasePriority, CaseStatus, DecisionAction,
    EscalationSource,
};

impl From<&DomainBrokerCase> for LegacyBrokerCase {
    fn from(src: &DomainBrokerCase) -> Self {
        Self {
            id: src.id.clone(),
            title: src.title.clone(),
            description: src.summary.clone(),
            priority: priority_from_u8(src.priority),
            source: source_from_category(&src.category),
            status: status_from_state(src.state),
            created_at: src.created_at.to_rfc3339(),
            updated_at: src.updated_at.to_rfc3339(),
            assigned_to: src.assigned_to.clone(),
            evidence: Vec::new(),
            metadata: src.metadata.clone(),
        }
    }
}

pub fn project_case(src: &DomainBrokerCase) -> LegacyBrokerCase {
    LegacyBrokerCase::from(src)
}

pub fn project_decision(entry: &DecisionHistoryEntry, case_id: &str) -> BrokerDecision {
    BrokerDecision {
        id: entry.decision_id.clone(),
        case_id: case_id.to_owned(),
        action: action_from_outcome(&entry.outcome),
        reasoning: entry.reasoning.clone(),
        decided_by: entry.broker_pubkey.clone(),
        decided_at: entry.decided_at.to_rfc3339(),
        provenance_event_id: None,
    }
}

fn priority_from_u8(p: u8) -> CasePriority {
    match p {
        0..=25 => CasePriority::Low,
        26..=50 => CasePriority::Medium,
        51..=75 => CasePriority::High,
        _ => CasePriority::Critical,
    }
}

fn source_from_category(cat: &CaseCategory) -> EscalationSource {
    match cat {
        CaseCategory::ContributorMeshShare | CaseCategory::KnowledgeEnrichment => {
            EscalationSource::WorkflowProposal
        }
        CaseCategory::WorkflowReview => EscalationSource::WorkflowProposal,
        CaseCategory::PolicyException => EscalationSource::PolicyViolation,
        CaseCategory::TrustAlert => EscalationSource::TrustDrift,
        CaseCategory::ManualSubmission => EscalationSource::ManualSubmission,
    }
}

fn status_from_state(s: CaseState) -> CaseStatus {
    match s {
        CaseState::Open => CaseStatus::Open,
        CaseState::UnderReview => CaseStatus::InReview,
        CaseState::Decided | CaseState::Promoted | CaseState::Precedent => CaseStatus::Decided,
        CaseState::Delegated => CaseStatus::Escalated,
        CaseState::Closed => CaseStatus::Closed,
    }
}

fn action_from_outcome(o: &DecisionOutcome) -> DecisionAction {
    match o {
        DecisionOutcome::Approve => DecisionAction::Approve,
        DecisionOutcome::Reject => DecisionAction::Reject,
        DecisionOutcome::Amend { .. } => DecisionAction::Amend,
        DecisionOutcome::Delegate { .. } => DecisionAction::Delegate,
        DecisionOutcome::Promote { .. } => DecisionAction::PromoteAsWorkflow,
        DecisionOutcome::Precedent { .. } => DecisionAction::MarkAsPrecedent,
    }
}
