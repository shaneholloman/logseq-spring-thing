//! `SkillEvalSuite` — versioned prompts + assertions + grader reference.
//!
//! Schema mirrors `docs/design/2026-04-20-contributor-studio/02-skill-dojo-and-evals.md` §8.1.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraderKind {
    /// Deterministic structural/string assertions.
    Deterministic,
    /// LLM-judge scoring.
    ModelGraded,
    /// Mix of deterministic and model-graded.
    Hybrid,
    /// Blind A/B against baseline.
    Comparator,
    /// Executor-observed tool-call / latency / token assertions.
    Executor,
    /// Special-case trigger-accuracy grader (must/must-not fire).
    TriggerAccuracy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub prompt: String,
    /// Opaque JSON string of assertions; we do not parse them here — the
    /// `SkillEvaluationActor` does, so the grammar can evolve without
    /// refactoring the aggregate.
    pub assertions_json: String,
    pub grader: GraderKind,
    pub tier_budget: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvalSuite {
    pub suite_id: String,
    pub prompts: Vec<EvalCase>,
    pub baseline_model_tier: u8,
    pub grader_ref: GraderKind,
}

impl SkillEvalSuite {
    pub fn new(suite_id: String, baseline_model_tier: u8, grader_ref: GraderKind) -> Self {
        Self {
            suite_id,
            prompts: Vec::new(),
            baseline_model_tier,
            grader_ref,
        }
    }

    pub fn add_case(&mut self, case: EvalCase) {
        self.prompts.push(case);
    }

    pub fn case_count(&self) -> usize {
        self.prompts.len()
    }
}
