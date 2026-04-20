//! BC11 — Judgment Broker Workbench domain (ADR-041 + ADR-042).
//!
//! Exposes the `BrokerCase` aggregate root, the `BrokerDecision` value-object
//! with its six canonical outcomes, and the `DecisionOrchestrator` domain
//! service. The adapter boundary for `contributor_mesh_share` cases is the
//! `ShareIntentBrokerAdapter` scaffold — the orchestrator that actually moves
//! share state lives under agent C4's `ShareOrchestratorActor` and is wired
//! in a later sprint.
//!
//! Canonical naming (per ADR-057 reconciliation):
//! - Category for contributor work-artifact promotion flows:
//!   `CaseCategory::ContributorMeshShare`
//! - Concrete subject is disambiguated via `SubjectKind` discriminator:
//!   `WorkArtifact | SkillPackage | AutomationProposal | PolicyException`
//! - Share state ladder: `Private → Team → Mesh`

pub mod broker_case;
pub mod broker_decision;

pub use broker_case::{
    BrokerCase, CaseCategory, CaseInvariantError, CaseState, DecisionHistoryEntry, ShareState,
    SubjectKind, SubjectRef,
};
pub use broker_decision::{
    DecisionOrchestrator, DecisionOutcome, OrchestrationError, ShareIntentBrokerAdapter,
    ShareTransitionPlan,
};
