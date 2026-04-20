//! `SkillEvaluationActor` — executes a `SkillEvalSuite` against a `SkillVersion`.
//!
//! Scope for this agent: scaffold the actor + state machine from §8.2 of
//! `docs/design/2026-04-20-contributor-studio/02-skill-dojo-and-evals.md`.
//! The MCP dispatcher glue (tool `skill_eval_run`) is owned by agent X1.
//!
//! State machine mirrors the design doc:
//!
//! ```text
//!   Idle → Allocating → Running → Grading → Analysing → Recording → Idle
//!                 ↘ Failed (no retry in scaffold) ↗
//! ```

use actix::prelude::*;
use log::{debug, info, warn};
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::skills::SkillEvalSuite;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalFsm {
    Idle,
    Allocating,
    Running,
    Grading,
    Analysing,
    Recording,
    Failed,
}

pub struct SkillEvaluationActor {
    state: EvalFsm,
    /// Cheap counter so tests can observe throughput.
    pub completed_runs: u64,
}

impl SkillEvaluationActor {
    pub fn new() -> Self {
        Self {
            state: EvalFsm::Idle,
            completed_runs: 0,
        }
    }

    pub fn state(&self) -> EvalFsm {
        self.state
    }
}

impl Default for SkillEvaluationActor {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for SkillEvaluationActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("[SkillEvaluationActor] started");
    }
}

/// Request an eval run. Returns a benchmark id string once the FSM hits Recording.
#[derive(Message)]
#[rtype(result = "Result<String, String>")]
pub struct SubmitEvalRun {
    pub skill_id: String,
    pub suite: Arc<SkillEvalSuite>,
    pub model_tier: u8,
}

impl Handler<SubmitEvalRun> for SkillEvaluationActor {
    type Result = Result<String, String>;

    fn handle(&mut self, msg: SubmitEvalRun, _ctx: &mut Self::Context) -> Self::Result {
        debug!(
            "[SkillEvaluationActor] accepted run for skill_id={} case_count={} tier={}",
            msg.skill_id,
            msg.suite.case_count(),
            msg.model_tier,
        );
        // Scaffold FSM drive: we move through the states synchronously; the
        // real executor / grader / comparator are owned by the MCP dispatcher.
        self.state = EvalFsm::Allocating;
        self.state = EvalFsm::Running;
        if msg.suite.case_count() == 0 {
            self.state = EvalFsm::Failed;
            warn!("[SkillEvaluationActor] empty suite rejected");
            return Err("empty eval suite".to_string());
        }
        self.state = EvalFsm::Grading;
        self.state = EvalFsm::Analysing;
        self.state = EvalFsm::Recording;
        let benchmark_id = Uuid::new_v4().to_string();
        self.completed_runs += 1;
        self.state = EvalFsm::Idle;
        Ok(benchmark_id)
    }
}

/// Observation helper for tests.
#[derive(Message)]
#[rtype(result = "(EvalFsm, u64)")]
pub struct GetEvaluationStats;

impl Handler<GetEvaluationStats> for SkillEvaluationActor {
    type Result = (EvalFsm, u64);

    fn handle(&mut self, _msg: GetEvaluationStats, _ctx: &mut Self::Context) -> Self::Result {
        (self.state, self.completed_runs)
    }
}
