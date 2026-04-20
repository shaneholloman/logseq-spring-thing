//! `SkillBenchmark` — one run of an eval suite against a version.
//!
//! BC19 invariant 7: a benchmark result references both the absolute pass rate
//! and a `BaselineComparison` against the previous benchmark.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvalVerdict {
    Pass,
    Fail,
    Regression,
    Inconclusive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineComparison {
    pub baseline_version_ref: String,
    pub delta_pass_rate: f32,
    pub delta_latency_ms: i64,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBenchmark {
    pub benchmark_id: String,
    pub suite_id: String,
    pub version_ref: String,
    pub pass_rate: f32,
    pub verdict: EvalVerdict,
    pub baseline: Option<BaselineComparison>,
    pub drift_score: f32,
    pub run_at: DateTime<Utc>,
    pub model_id: String,
    pub tier: u8,
}

impl SkillBenchmark {
    /// BC19 inv. 7 guard: all non-initial benchmarks require a baseline.
    pub fn validate(&self, has_prior_version: bool) -> Result<(), &'static str> {
        if has_prior_version && self.baseline.is_none() {
            return Err("non-initial benchmark missing BaselineComparison");
        }
        if !(0.0..=1.0).contains(&self.pass_rate) {
            return Err("pass_rate must be within [0,1]");
        }
        Ok(())
    }

    /// Did the benchmark pass? Used by the lifecycle gate guard.
    pub fn passed(&self) -> bool {
        matches!(self.verdict, EvalVerdict::Pass)
    }
}
