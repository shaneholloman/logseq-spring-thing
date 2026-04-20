//! `SkillLifecycleState` enum, transitions, and guards.
//!
//! Implements the monotonic state machine defined in
//! `docs/explanation/ddd-contributor-enablement-context.md` §BC19 and the gate
//! matrix in `docs/design/2026-04-20-contributor-studio/02-skill-dojo-and-evals.md` §8.3.
//!
//! ```text
//!  Draft ──► Personal ──► TeamShared ──► Benchmarked ──► MeshCandidate ──► Promoted
//!                                                                             │
//!                                                                             ▼
//!                                                                          Retired
//! ```
//!
//! Retired is terminal (BC19 invariant 2). TeamShared gate needs a passing baseline
//! benchmark. MeshCandidate gate needs ≥3 benchmarks over 30 days plus a broker
//! review marker (BC19 spec §8.3, PRD-003 §14 R12 fanout concerns honoured separately
//! by [`crate::actors::skill_compatibility_scanner`]).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillLifecycleState {
    Draft,
    Personal,
    TeamShared,
    Benchmarked,
    MeshCandidate,
    Promoted,
    Retired,
}

impl SkillLifecycleState {
    /// Is this state the aggregate terminal? Retirement is the only terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, SkillLifecycleState::Retired)
    }

    /// Return the next state in the forward progression (None for terminal).
    pub fn next(&self) -> Option<SkillLifecycleState> {
        use SkillLifecycleState::*;
        match self {
            Draft => Some(Personal),
            Personal => Some(TeamShared),
            TeamShared => Some(Benchmarked),
            Benchmarked => Some(MeshCandidate),
            MeshCandidate => Some(Promoted),
            Promoted => None, // May only go to Retired via explicit retirement action
            Retired => None,
        }
    }
}

/// Context presented to the guard when an aggregate asks to transition.
///
/// The guard is pure: it reads the context + current state, and returns
/// `Ok(new_state)` or a typed error describing why the gate refused.
#[derive(Debug, Clone, Default)]
pub struct TransitionContext {
    /// Passing eval benchmarks attached to the current version, newest-first.
    pub passing_benchmarks: Vec<BenchmarkSummary>,
    /// True once the Judgment Broker (BC11, ADR-041) has green-lit a Team→Mesh move.
    pub broker_review_ok: bool,
    /// Successor `skill_id` for `Retired` transitions. BC19 invariant 2.
    pub successor_ref: Option<String>,
    /// Whether the retirement reason is `BaseModelAbsorbed`. Substitute for a successor.
    pub base_model_absorbed: bool,
    /// Evaluation wall clock, injected so tests are deterministic.
    pub now: DateTime<Utc>,
}

/// Compact benchmark summary for gate checks. Full record lives in [`crate::domain::skills::SkillBenchmark`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub benchmark_id: String,
    pub run_at: DateTime<Utc>,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LifecycleTransitionError {
    #[error("illegal transition from {from:?} to {to:?}")]
    IllegalTransition { from: SkillLifecycleState, to: SkillLifecycleState },

    #[error("TeamShared requires at least one passing benchmark within the last 30 days")]
    TeamSharedNeedsPassingBaseline,

    #[error("MeshCandidate requires ≥3 passing benchmarks within the last 30 days")]
    MeshCandidateNeedsBenchmarks,

    #[error("MeshCandidate requires a broker review marker (BC11/ADR-041)")]
    MeshCandidateNeedsBrokerReview,

    #[error("Retirement requires either a successor_ref or BaseModelAbsorbed rationale (BC19 inv. 2)")]
    RetirementNeedsJustification,

    #[error("Retired is terminal; no outbound transition possible")]
    TerminalState,
}

/// The whole state machine as one pure function. The aggregate calls this,
/// inspects the `Result`, and only mutates itself on `Ok`.
pub fn transition(
    from: SkillLifecycleState,
    to: SkillLifecycleState,
    ctx: &TransitionContext,
) -> Result<SkillLifecycleState, LifecycleTransitionError> {
    use SkillLifecycleState::*;

    if from == Retired {
        return Err(LifecycleTransitionError::TerminalState);
    }

    // Retirement is reachable from any non-terminal state (early abandon),
    // but always via a justified rationale.
    if to == Retired {
        if ctx.successor_ref.is_some() || ctx.base_model_absorbed {
            return Ok(Retired);
        }
        return Err(LifecycleTransitionError::RetirementNeedsJustification);
    }

    // Otherwise, transitions are strictly monotonic along the enum order.
    let expected = from.next();
    if expected != Some(to) {
        return Err(LifecycleTransitionError::IllegalTransition { from, to });
    }

    // Gate-specific guards.
    match to {
        TeamShared => {
            let thirty_days_ago = ctx.now - Duration::days(30);
            let has_recent_pass = ctx
                .passing_benchmarks
                .iter()
                .any(|b| b.passed && b.run_at >= thirty_days_ago);
            if !has_recent_pass {
                return Err(LifecycleTransitionError::TeamSharedNeedsPassingBaseline);
            }
        }
        MeshCandidate => {
            let thirty_days_ago = ctx.now - Duration::days(30);
            let recent_passes: usize = ctx
                .passing_benchmarks
                .iter()
                .filter(|b| b.passed && b.run_at >= thirty_days_ago)
                .count();
            if recent_passes < 3 {
                return Err(LifecycleTransitionError::MeshCandidateNeedsBenchmarks);
            }
            if !ctx.broker_review_ok {
                return Err(LifecycleTransitionError::MeshCandidateNeedsBrokerReview);
            }
        }
        _ => {}
    }

    Ok(to)
}
