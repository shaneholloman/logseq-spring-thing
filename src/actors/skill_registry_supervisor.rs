//! `SkillRegistrySupervisor` — BC19 actor tree root.
//!
//! Spawns and supervises:
//! * [`SkillEvaluationActor`](super::skill_evaluation_actor) — runs eval suites.
//! * [`SkillCompatibilityScanner`](super::skill_compatibility_scanner) — drift detector
//!   with bounded concurrency (PRD-003 §14 R12).
//!
//! `DojoDiscoveryActor` is out of scope for this agent (owned by P2); the
//! supervisor exposes a hook so P2 can plug its child in later.
//!
//! The supervisor holds the canonical in-memory registry of
//! [`SkillPackage`](crate::domain::skills::SkillPackage)s. Writes happen via
//! messages; reads happen via cloned snapshots to keep the Actix mailbox cheap.

use actix::prelude::*;
use log::{debug, info};
use std::collections::HashMap;
use std::sync::Arc;

use crate::actors::skill_compatibility_scanner::{
    ScanAllInstalled, SkillCompatibilityScanner, SkillCompatibilityScannerConfig,
};
use crate::actors::skill_evaluation_actor::{SkillEvaluationActor, SubmitEvalRun};
use crate::domain::skills::{
    SkillBenchmark, SkillEvalSuite, SkillLifecycleState, SkillPackage, SkillPackageError,
};

/// The supervisor singleton. Keep the surface thin; real coordination lives
/// in the actor handlers below.
pub struct SkillRegistrySupervisor {
    packages: HashMap<String, SkillPackage>,
    pub evaluation: Option<Addr<SkillEvaluationActor>>,
    pub scanner: Option<Addr<SkillCompatibilityScanner>>,
    scanner_config: SkillCompatibilityScannerConfig,
}

impl SkillRegistrySupervisor {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            evaluation: None,
            scanner: None,
            scanner_config: SkillCompatibilityScannerConfig::default(),
        }
    }

    pub fn with_scanner_concurrency(mut self, max_parallel: usize) -> Self {
        self.scanner_config.max_parallel = max_parallel;
        self
    }
}

impl Default for SkillRegistrySupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for SkillRegistrySupervisor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        let eval = SkillEvaluationActor::new().start();
        let scanner = SkillCompatibilityScanner::new(self.scanner_config.clone()).start();
        self.evaluation = Some(eval);
        self.scanner = Some(scanner);
        info!(
            "[SkillRegistrySupervisor] started; scanner max_parallel={}",
            self.scanner_config.max_parallel
        );
    }
}

// ───────────── messages ─────────────

#[derive(Message)]
#[rtype(result = "Result<(), SkillPackageError>")]
pub struct RegisterPackage(pub SkillPackage);

#[derive(Message)]
#[rtype(result = "Option<SkillPackage>")]
pub struct GetPackage(pub String);

#[derive(Message)]
#[rtype(result = "Vec<String>")]
pub struct ListInstalledIds;

#[derive(Message)]
#[rtype(result = "Result<SkillLifecycleState, SkillPackageError>")]
pub struct TransitionPackage {
    pub skill_id: String,
    pub target: SkillLifecycleState,
    pub share_intent_present: bool,
}

#[derive(Message)]
#[rtype(result = "Result<(), SkillPackageError>")]
pub struct AttachBenchmark {
    pub skill_id: String,
    pub benchmark: SkillBenchmark,
}

/// Forwards to the compatibility scanner.
#[derive(Message)]
#[rtype(result = "usize")]
pub struct TriggerConfigChangeScan {
    pub affected_tiers: Vec<u8>,
}

/// Evaluation shortcut so the HTTP layer does not need to know about child addrs.
#[derive(Message)]
#[rtype(result = "Result<String, String>")]
pub struct RunSkillEval {
    pub skill_id: String,
    pub suite: Arc<SkillEvalSuite>,
    pub model_tier: u8,
}

// ───────────── handlers ─────────────

impl Handler<RegisterPackage> for SkillRegistrySupervisor {
    type Result = Result<(), SkillPackageError>;

    fn handle(&mut self, msg: RegisterPackage, _ctx: &mut Self::Context) -> Self::Result {
        debug!("[SkillRegistrySupervisor] register {}", msg.0.skill_id);
        self.packages.insert(msg.0.skill_id.clone(), msg.0);
        Ok(())
    }
}

impl Handler<GetPackage> for SkillRegistrySupervisor {
    type Result = Option<SkillPackage>;

    fn handle(&mut self, msg: GetPackage, _ctx: &mut Self::Context) -> Self::Result {
        self.packages.get(&msg.0).cloned()
    }
}

impl Handler<ListInstalledIds> for SkillRegistrySupervisor {
    type Result = Vec<String>;

    fn handle(&mut self, _msg: ListInstalledIds, _ctx: &mut Self::Context) -> Self::Result {
        self.packages.keys().cloned().collect()
    }
}

impl Handler<TransitionPackage> for SkillRegistrySupervisor {
    type Result = Result<SkillLifecycleState, SkillPackageError>;

    fn handle(&mut self, msg: TransitionPackage, _ctx: &mut Self::Context) -> Self::Result {
        let pkg = self
            .packages
            .get_mut(&msg.skill_id)
            .ok_or_else(|| SkillPackageError::UnknownVersion(msg.skill_id.clone()))?;
        pkg.transition(msg.target, msg.share_intent_present)
    }
}

impl Handler<AttachBenchmark> for SkillRegistrySupervisor {
    type Result = Result<(), SkillPackageError>;

    fn handle(&mut self, msg: AttachBenchmark, _ctx: &mut Self::Context) -> Self::Result {
        let pkg = self
            .packages
            .get_mut(&msg.skill_id)
            .ok_or_else(|| SkillPackageError::UnknownVersion(msg.skill_id.clone()))?;
        pkg.attach_benchmark(msg.benchmark).map(|_| ())
    }
}

impl Handler<TriggerConfigChangeScan> for SkillRegistrySupervisor {
    type Result = ResponseFuture<usize>;

    fn handle(
        &mut self,
        msg: TriggerConfigChangeScan,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let ids: Vec<String> = self.packages.keys().cloned().collect();
        let scanner = self.scanner.clone();
        Box::pin(async move {
            let Some(scanner) = scanner else { return 0 };
            scanner
                .send(ScanAllInstalled {
                    skill_ids: ids,
                    affected_tiers: msg.affected_tiers,
                })
                .await
                .unwrap_or(0)
        })
    }
}

impl Handler<RunSkillEval> for SkillRegistrySupervisor {
    type Result = ResponseFuture<Result<String, String>>;

    fn handle(&mut self, msg: RunSkillEval, _ctx: &mut Self::Context) -> Self::Result {
        let eval = self.evaluation.clone();
        Box::pin(async move {
            let Some(eval) = eval else {
                return Err("evaluation actor not started".to_string());
            };
            eval.send(SubmitEvalRun {
                skill_id: msg.skill_id,
                suite: msg.suite,
                model_tier: msg.model_tier,
            })
            .await
            .map_err(|e| format!("mailbox error: {e}"))?
        })
    }
}
