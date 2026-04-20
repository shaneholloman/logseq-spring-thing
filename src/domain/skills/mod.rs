//! BC19 — Skill Lifecycle bounded context.
//!
//! Aggregates, value objects, and lifecycle state machine for portable,
//! versioned, evaluated capability. Spec refs:
//! * `docs/explanation/ddd-contributor-enablement-context.md` §BC19
//! * `docs/design/2026-04-20-contributor-studio/02-skill-dojo-and-evals.md`
//! * `docs/PRD-003-contributor-ai-support-stratum.md` §14 R12
//!
//! Aggregate root: [`SkillPackage`]. Entities:
//! [`SkillVersion`], [`SkillEvalSuite`], [`SkillBenchmark`], [`SkillDistribution`].

pub mod lifecycle;
pub mod skill_benchmark;
pub mod skill_distribution;
pub mod skill_eval_suite;
pub mod skill_package;
pub mod skill_version;

pub use lifecycle::{LifecycleTransitionError, SkillLifecycleState, TransitionContext};
pub use skill_benchmark::{BaselineComparison, EvalVerdict, SkillBenchmark};
pub use skill_distribution::{DistributionScope, SkillDistribution};
pub use skill_eval_suite::{EvalCase, GraderKind, SkillEvalSuite};
pub use skill_package::{SkillPackage, SkillPackageError};
pub use skill_version::{SkillFingerprint, SkillVersion, SkillVersionError, ToolSequenceSpec};
