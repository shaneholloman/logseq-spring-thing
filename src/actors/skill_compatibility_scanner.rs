//! `SkillCompatibilityScanner` — drift detector with bounded concurrency.
//!
//! Implements PRD-003 §14 R12: on a model-routing config change or upstream
//! MCP tool version bump, the scanner must re-benchmark every affected skill
//! **without** exceeding the mesh benchmark-runner's capacity. The contract
//! is a `tokio::sync::Semaphore` with a configurable permit count; the default
//! is 8 (PRD-003 §14 R12 default).
//!
//! This scaffold is transport-agnostic: the actual benchmark dispatch plugs in
//! via a `BenchmarkDispatcher` trait. Tests use a counting dispatcher to assert
//! the concurrency cap.

use actix::prelude::*;
use async_trait::async_trait;
use futures::future::join_all;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};

/// How the scanner dispatches a single drift benchmark. Production wires this
/// to the MCP `skill_eval_run` tool; tests wire to an in-memory counter.
#[async_trait]
pub trait BenchmarkDispatcher: Send + Sync + 'static {
    async fn dispatch(&self, skill_id: &str) -> Result<(), String>;
}

/// Default dispatcher: logs and returns Ok. Useful in dev where the MCP
/// runner is not yet wired. Replace via `SkillCompatibilityScanner::with_dispatcher`.
pub struct NoopBenchmarkDispatcher;

#[async_trait]
impl BenchmarkDispatcher for NoopBenchmarkDispatcher {
    async fn dispatch(&self, skill_id: &str) -> Result<(), String> {
        debug!("[NoopBenchmarkDispatcher] would re-benchmark {}", skill_id);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SkillCompatibilityScannerConfig {
    /// PRD-003 §14 R12: default 8 parallel benchmarks.
    pub max_parallel: usize,
    /// Jitter on dispatch to avoid synchronised MCP bursts.
    pub dispatch_jitter_ms: u64,
}

impl Default for SkillCompatibilityScannerConfig {
    fn default() -> Self {
        Self {
            max_parallel: 8,
            dispatch_jitter_ms: 0,
        }
    }
}

pub struct SkillCompatibilityScanner {
    config: SkillCompatibilityScannerConfig,
    semaphore: Arc<Semaphore>,
    dispatcher: Arc<dyn BenchmarkDispatcher>,
    /// High-water mark of concurrently in-flight benchmarks; observability for R12.
    in_flight_peak: Arc<AtomicUsize>,
    /// Running counter so tests can sanity-check that every skill was visited.
    dispatched_total: Arc<AtomicUsize>,
}

impl SkillCompatibilityScanner {
    pub fn new(config: SkillCompatibilityScannerConfig) -> Self {
        let permits = config.max_parallel.max(1);
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
            dispatcher: Arc::new(NoopBenchmarkDispatcher),
            in_flight_peak: Arc::new(AtomicUsize::new(0)),
            dispatched_total: Arc::new(AtomicUsize::new(0)),
            config,
        }
    }

    pub fn with_dispatcher(mut self, dispatcher: Arc<dyn BenchmarkDispatcher>) -> Self {
        self.dispatcher = dispatcher;
        self
    }

    pub fn peek_in_flight_peak(&self) -> usize {
        self.in_flight_peak.load(Ordering::SeqCst)
    }

    pub fn peek_dispatched_total(&self) -> usize {
        self.dispatched_total.load(Ordering::SeqCst)
    }
}

impl Default for SkillCompatibilityScanner {
    fn default() -> Self {
        Self::new(SkillCompatibilityScannerConfig::default())
    }
}

impl Actor for SkillCompatibilityScanner {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!(
            "[SkillCompatibilityScanner] started; max_parallel={}",
            self.config.max_parallel
        );
    }
}

/// Submit a fan-out scan. Returns the number of skills that completed.
#[derive(Message)]
#[rtype(result = "usize")]
pub struct ScanAllInstalled {
    pub skill_ids: Vec<String>,
    pub affected_tiers: Vec<u8>,
}

impl Handler<ScanAllInstalled> for SkillCompatibilityScanner {
    type Result = ResponseFuture<usize>;

    fn handle(&mut self, msg: ScanAllInstalled, _ctx: &mut Self::Context) -> Self::Result {
        let sem = self.semaphore.clone();
        let dispatcher = self.dispatcher.clone();
        let peak = self.in_flight_peak.clone();
        let total = self.dispatched_total.clone();
        let jitter = self.config.dispatch_jitter_ms;

        Box::pin(async move {
            let in_flight = Arc::new(AtomicUsize::new(0));
            let ids = msg.skill_ids;

            debug!(
                "[SkillCompatibilityScanner] fan-out {} skill(s); affected_tiers={:?}",
                ids.len(),
                msg.affected_tiers
            );

            let tasks = ids.into_iter().map(|skill_id| {
                let sem = sem.clone();
                let dispatcher = dispatcher.clone();
                let peak = peak.clone();
                let in_flight = in_flight.clone();
                let total = total.clone();
                async move {
                    let _permit = match sem.acquire().await {
                        Ok(p) => p,
                        Err(_) => {
                            warn!(
                                "[SkillCompatibilityScanner] semaphore closed; aborting {}",
                                skill_id
                            );
                            return 0usize;
                        }
                    };
                    let cur = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    // Update peak atomically via compare_exchange loop.
                    let mut prev_peak = peak.load(Ordering::SeqCst);
                    while cur > prev_peak {
                        match peak.compare_exchange(
                            prev_peak,
                            cur,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        ) {
                            Ok(_) => break,
                            Err(observed) => prev_peak = observed,
                        }
                    }

                    if jitter > 0 {
                        sleep(Duration::from_millis(jitter)).await;
                    }
                    let ok = dispatcher.dispatch(&skill_id).await.is_ok();
                    if ok {
                        total.fetch_add(1, Ordering::SeqCst);
                    }
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                    if ok { 1 } else { 0 }
                }
            });

            join_all(tasks).await.into_iter().sum::<usize>()
        })
    }
}
