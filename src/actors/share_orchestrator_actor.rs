//! [`ShareOrchestratorActor`] — Actix wrapper around
//! [`ShareOrchestrator`] (spec §7.3). Consumes `ShareIntentCreated`
//! events from BC18, drives the share-state transition pipeline, and
//! writes the share-log.
//!
//! The actor is intentionally thin: all orchestration logic lives in
//! [`crate::services::share_orchestrator::ShareOrchestrator`]; this
//! module is the supervision-tree integration point.

use actix::prelude::*;
use log::{error, info, warn};
use std::sync::Arc;
use std::time::Duration;

use crate::actors::supervisor::{
    RegisterActor, SupervisedActorTrait, SupervisionStrategy, SupervisorActor,
};
use crate::errors::VisionFlowError;
use crate::services::share_orchestrator::{
    ShareContextExtras, ShareOrchestrator, ShareOrchestratorError, ShareOutcome,
};
use crate::services::share_policy::ShareIntent;

/// Request the actor routes a ShareIntent through the orchestrator pipeline.
#[derive(Message)]
#[rtype(result = "Result<ShareOutcome, ShareOrchestratorError>")]
pub struct RouteShareIntent {
    pub intent: ShareIntent,
    pub extras: ShareContextExtras,
}

/// Broker-decision callback — replays the broker's verdict back into the
/// orchestrator so WAC mutations (promote/demote/retract) can fire.
#[derive(Message)]
#[rtype(result = "Result<ShareOutcome, ShareOrchestratorError>")]
pub struct ApplyBrokerDecision {
    pub intent: ShareIntent,
    pub extras: ShareContextExtras,
}

/// Shutdown hook (graceful drain).
#[derive(Message)]
#[rtype(result = "()")]
pub struct Shutdown;

/// Ready-check message used by supervision health probes.
#[derive(Message)]
#[rtype(result = "bool")]
pub struct IsReady;

/// The actor itself. Owns an `Arc<ShareOrchestrator>`; multiple clones of
/// the orchestrator share state because `ShareOrchestrator` is internally
/// immutable and stateful state lives behind `Mutex` / ports.
pub struct ShareOrchestratorActor {
    orchestrator: Arc<ShareOrchestrator>,
    ready: bool,
}

impl ShareOrchestratorActor {
    pub fn new(orchestrator: Arc<ShareOrchestrator>) -> Self {
        Self { orchestrator, ready: true }
    }
}

impl Actor for ShareOrchestratorActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("[ShareOrchestratorActor] started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("[ShareOrchestratorActor] stopped");
    }
}

impl SupervisedActorTrait for ShareOrchestratorActor {
    fn actor_name() -> &'static str { "ShareOrchestratorActor" }
    fn supervision_strategy() -> SupervisionStrategy {
        SupervisionStrategy::RestartWithBackoff {
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
    fn max_restart_count() -> u32 { 5 }
    fn restart_window() -> Duration { Duration::from_secs(300) }
}

impl Handler<RouteShareIntent> for ShareOrchestratorActor {
    type Result = ResponseFuture<Result<ShareOutcome, ShareOrchestratorError>>;

    fn handle(&mut self, msg: RouteShareIntent, _ctx: &mut Self::Context) -> Self::Result {
        let orch = Arc::clone(&self.orchestrator);
        Box::pin(async move {
            match orch.handle_intent(msg.intent.clone(), msg.extras).await {
                Ok(outcome) => Ok(outcome),
                Err(e) => {
                    error!(
                        "[ShareOrchestratorActor] intent {} failed: {}",
                        msg.intent.intent_id, e
                    );
                    Err(e)
                }
            }
        })
    }
}

impl Handler<ApplyBrokerDecision> for ShareOrchestratorActor {
    type Result = ResponseFuture<Result<ShareOutcome, ShareOrchestratorError>>;

    fn handle(&mut self, msg: ApplyBrokerDecision, _ctx: &mut Self::Context) -> Self::Result {
        // Broker-driven replays follow the same pipeline as the initial
        // intent — the transition classifier reads `source_state` +
        // `target_scope` to decide between promote / demote / retract.
        let orch = Arc::clone(&self.orchestrator);
        Box::pin(async move {
            orch.handle_intent(msg.intent, msg.extras).await
        })
    }
}

impl Handler<Shutdown> for ShareOrchestratorActor {
    type Result = ();
    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) {
        self.ready = false;
        info!("[ShareOrchestratorActor] shutdown requested");
        ctx.stop();
    }
}

impl Handler<IsReady> for ShareOrchestratorActor {
    type Result = MessageResult<IsReady>;
    fn handle(&mut self, _msg: IsReady, _ctx: &mut Self::Context) -> Self::Result {
        MessageResult(self.ready)
    }
}

/// Register a live [`ShareOrchestratorActor`] with the supervision tree.
///
/// Callers (app startup) pass the `Arc<ShareOrchestrator>` they construct
/// from shared ports; the factory closure recreates the actor on failure.
pub async fn register_share_orchestrator_actor(
    supervisor: &Addr<SupervisorActor>,
    orchestrator: Arc<ShareOrchestrator>,
) -> Result<Addr<ShareOrchestratorActor>, VisionFlowError> {
    let actor = ShareOrchestratorActor::new(Arc::clone(&orchestrator)).start();

    let factory_orch = Arc::clone(&orchestrator);
    let factory = Arc::new(move || -> Box<dyn std::any::Any + Send> {
        let addr = ShareOrchestratorActor::new(Arc::clone(&factory_orch)).start();
        Box::new(addr)
    });

    let result = supervisor
        .send(RegisterActor {
            actor_name: ShareOrchestratorActor::actor_name().into(),
            strategy: ShareOrchestratorActor::supervision_strategy(),
            max_restart_count: ShareOrchestratorActor::max_restart_count(),
            restart_window: ShareOrchestratorActor::restart_window(),
            actor_factory: Some(factory),
        })
        .await;

    match result {
        Ok(Ok(())) => Ok(actor),
        Ok(Err(e)) => {
            warn!("[ShareOrchestratorActor] supervisor rejected: {}", e);
            Err(e)
        }
        Err(e) => {
            warn!("[ShareOrchestratorActor] supervisor mailbox error: {}", e);
            Err(VisionFlowError::Generic {
                message: format!("supervisor mailbox error: {}", e),
                source: None,
            })
        }
    }
}
