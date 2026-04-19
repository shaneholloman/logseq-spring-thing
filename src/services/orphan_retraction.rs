// src/services/orphan_retraction.rs
//! Orphan retraction — scheduled cleanup of stale `WikilinkRef` edges and
//! the private stubs that become unreachable once their incoming references
//! disappear (ADR-051 hygiene lane).
//!
//! Runs on a `tokio::interval` regardless of `BRIDGE_EDGE_ENABLED` — retraction
//! is graph hygiene that must not regress when the promotion pipeline is off.
//!
//! Two-phase sweep per tick:
//! 1. Delete `WikilinkRef` relationships whose `last_seen_run_id` does not
//!    match the current ingest run AND whose `last_seen_at` is older than
//!    7 days. This protects against flapping (one bad ingest does not
//!    retract everything immediately).
//! 2. Delete `:KGNode` rows with `visibility = 'private'` that now have no
//!    inbound `WikilinkRef` or `EDGE` pointing at them. Public rows are never
//!    auto-deleted — they may be authored but unreferenced, which is fine.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use log::{debug, info, warn};
use neo4rs::query;
use tokio::task::JoinHandle;

use crate::adapters::neo4j_adapter::Neo4jAdapter;

/// Default period between retraction sweeps, in seconds.
pub const DEFAULT_PERIOD_SECS: u64 = 15 * 60;

/// Stale-age (days) gate for WikilinkRef retraction. Edges younger than this
/// are kept even if their `last_seen_run_id` does not match the current run —
/// this prevents a single failed ingest from nuking the link graph.
pub const STALE_AGE_DAYS: i64 = 7;

/// Read `ORPHAN_RETRACTION_PERIOD_SECS` env var, falling back to the default.
pub fn period_from_env() -> Duration {
    let secs = std::env::var("ORPHAN_RETRACTION_PERIOD_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_PERIOD_SECS);
    Duration::from_secs(secs.max(1))
}

/// Outcome of a single retraction pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetractionReport {
    pub wikilinks_deleted: u64,
    pub stubs_deleted: u64,
}

/// Scheduled orphan retraction task.
///
/// Call [`OrphanRetractionTask::spawn`] once at startup. The returned
/// [`JoinHandle`] keeps the loop alive; drop it to (eventually) stop.
/// Single-shot invocation via [`OrphanRetractionTask::run_once`] is also
/// available for manual kicks / tests.
pub struct OrphanRetractionTask {
    neo4j: Arc<Neo4jAdapter>,
    current_run_id: Arc<tokio::sync::RwLock<String>>,
}

impl OrphanRetractionTask {
    /// Construct a task bound to the given Neo4j adapter. `initial_run_id`
    /// must match whatever the ingest pipeline stamps onto `WikilinkRef`
    /// edges when it observes them.
    pub fn new(neo4j: Arc<Neo4jAdapter>, initial_run_id: impl Into<String>) -> Self {
        Self {
            neo4j,
            current_run_id: Arc::new(tokio::sync::RwLock::new(initial_run_id.into())),
        }
    }

    /// Update the run id the next sweep should consider "fresh". Typically
    /// called at the start of every ingest pass.
    pub async fn set_current_run_id(&self, run_id: impl Into<String>) {
        *self.current_run_id.write().await = run_id.into();
    }

    /// Spawn the periodic sweep loop. Ticks every [`period_from_env`] seconds.
    /// The first tick fires immediately after one period (not at t=0).
    pub fn spawn(self: Arc<Self>) -> JoinHandle<()> {
        let period = period_from_env();
        info!(
            "OrphanRetractionTask: spawning periodic sweep every {}s",
            period.as_secs()
        );
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(period);
            // Skip the immediate tick — let the system warm up first.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                match self.run_once().await {
                    Ok(report) => {
                        if report.wikilinks_deleted > 0 || report.stubs_deleted > 0 {
                            info!(
                                "orphan_retraction: swept {} wikilinks, {} private stubs",
                                report.wikilinks_deleted, report.stubs_deleted
                            );
                        } else {
                            debug!("orphan_retraction: nothing to sweep");
                        }
                    }
                    Err(e) => warn!("orphan_retraction tick failed: {:#}", e),
                }
            }
        })
    }

    /// Single sweep. Returns counts of deleted entities.
    pub async fn run_once(&self) -> Result<RetractionReport> {
        let run_id = self.current_run_id.read().await.clone();
        let mut report = RetractionReport::default();

        // ── Phase 1: retract stale WikilinkRef edges ──────────────────────
        let wikilink_q = query(
            "MATCH (src:KGNode)-[w:WikilinkRef]->(tgt:KGNode)
             WHERE (w.last_seen_run_id IS NULL OR w.last_seen_run_id <> $run_id)
               AND (
                    w.last_seen_at IS NULL
                    OR datetime(w.last_seen_at) < datetime() - duration({days: $stale_days})
                   )
             WITH w
             DELETE w
             RETURN count(w) AS deleted",
        )
        .param("run_id", run_id.clone())
        .param("stale_days", STALE_AGE_DAYS);

        let mut wres = self
            .neo4j
            .graph()
            .execute(wikilink_q)
            .await
            .with_context(|| "orphan_retraction: retract WikilinkRef edges")?;
        if let Some(row) = wres
            .next()
            .await
            .with_context(|| "orphan_retraction: read wikilink count row")?
        {
            report.wikilinks_deleted = row.get::<i64>("deleted").unwrap_or(0) as u64;
        }

        // ── Phase 2: delete private stubs with zero inbound refs ──────────
        // Only touch `visibility = 'private'` rows. Public nodes are never
        // auto-deleted here — that is the authoring workflow's call.
        let stub_q = query(
            "MATCH (n:KGNode {visibility: 'private'})
             WHERE NOT EXISTS {
               MATCH (other)-[r]->(n)
               WHERE type(r) IN ['WikilinkRef', 'EDGE']
             }
             WITH n
             DETACH DELETE n
             RETURN count(n) AS deleted",
        );

        let mut sres = self
            .neo4j
            .graph()
            .execute(stub_q)
            .await
            .with_context(|| "orphan_retraction: delete private stubs")?;
        if let Some(row) = sres
            .next()
            .await
            .with_context(|| "orphan_retraction: read stub count row")?
        {
            report.stubs_deleted = row.get::<i64>("deleted").unwrap_or(0) as u64;
        }

        Ok(report)
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn period_from_env_defaults() {
        let prev = std::env::var("ORPHAN_RETRACTION_PERIOD_SECS").ok();
        std::env::remove_var("ORPHAN_RETRACTION_PERIOD_SECS");
        assert_eq!(period_from_env(), Duration::from_secs(DEFAULT_PERIOD_SECS));
        if let Some(v) = prev {
            std::env::set_var("ORPHAN_RETRACTION_PERIOD_SECS", v);
        }
    }

    #[test]
    fn period_from_env_overrides() {
        let prev = std::env::var("ORPHAN_RETRACTION_PERIOD_SECS").ok();
        std::env::set_var("ORPHAN_RETRACTION_PERIOD_SECS", "42");
        assert_eq!(period_from_env(), Duration::from_secs(42));
        if let Some(v) = prev {
            std::env::set_var("ORPHAN_RETRACTION_PERIOD_SECS", v);
        } else {
            std::env::remove_var("ORPHAN_RETRACTION_PERIOD_SECS");
        }
    }

    #[test]
    fn period_from_env_clamps_to_minimum_one() {
        let prev = std::env::var("ORPHAN_RETRACTION_PERIOD_SECS").ok();
        std::env::set_var("ORPHAN_RETRACTION_PERIOD_SECS", "0");
        assert_eq!(period_from_env(), Duration::from_secs(1));
        if let Some(v) = prev {
            std::env::set_var("ORPHAN_RETRACTION_PERIOD_SECS", v);
        } else {
            std::env::remove_var("ORPHAN_RETRACTION_PERIOD_SECS");
        }
    }

    #[test]
    fn retraction_report_default_is_zeroed() {
        let r = RetractionReport::default();
        assert_eq!(r.wikilinks_deleted, 0);
        assert_eq!(r.stubs_deleted, 0);
    }
}
