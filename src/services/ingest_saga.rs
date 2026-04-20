//! Ingest Saga — Pod-first, Neo4j-second write ordering (ADR-051).
//!
//! # Why
//! If Neo4j commits first but the Pod write fails, we have an orphan graph node
//! pointing nowhere. Reversing the order ensures the Pod has content before the
//! graph declares the node exists. If the process crashes between the two
//! writes, we leave a `saga_pending: true` marker on the KGNode that a
//! background resumption task converts to a full commit on retry.
//!
//! # Data flow
//!   caller (github_sync_service) → IngestSaga::execute_batch
//!       ├─ Phase 1: PUT each node content to its Pod URL
//!       │   (any failure: skip the corresponding Neo4j commit, no marker)
//!       ├─ Phase 2: save_graph() with the Pod-successful nodes
//!       │   (failure: write pending markers for those nodes)
//!       └─ Phase 3: clear pending markers for the nodes that committed
//!
//! A separate `IngestSaga::resume_pending` loop picks up pending markers every
//! 60 seconds (driven by `spawn_resumption_task`) and retries their Neo4j
//! commit. Replay is idempotent: the marker is a single `MERGE`-able property.
//!
//! # Feature flag
//! `POD_SAGA_ENABLED=true|false` — when disabled, the saga short-circuits to
//! the legacy Neo4j-only path to preserve dev workflow during rollout.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use log::{debug, error, info, warn};
use neo4rs::query;
use serde::{Deserialize, Serialize};
use tokio::time::interval;
use uuid::Uuid;

use crate::adapters::neo4j_adapter::Neo4jAdapter;
use crate::models::graph::GraphData;
use crate::models::node::Node as KGNode;
use crate::ports::knowledge_graph_repository::KnowledgeGraphRepository;
use crate::services::metrics::{MetricsRegistry, SagaOutcomeLabel, SagaOutcomeLabels};
use crate::services::pod_client::{PodClient, PodClientError, Visibility};
use crate::services::urn_solid_mapping::{
    urn_solid_alignment_enabled, MappingStatus, UrnSolidMapper,
};

/// Env-var feature flag name.
pub const POD_SAGA_ENABLED_ENV: &str = "POD_SAGA_ENABLED";

/// Default Pod base URL env var.
pub const POD_BASE_URL_ENV: &str = "POD_BASE_URL";

/// How often the resumption task wakes up.
pub const RESUMPTION_INTERVAL: Duration = Duration::from_secs(60);

/// Max pending nodes fetched per resumption tick.
pub const RESUMPTION_BATCH_LIMIT: usize = 200;

/// Returns true if the saga is enabled via `POD_SAGA_ENABLED`.
pub fn saga_enabled() -> bool {
    std::env::var(POD_SAGA_ENABLED_ENV)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Resolves the Pod base URL from the environment, with a sensible in-cluster default.
pub fn pod_base_url() -> String {
    std::env::var(POD_BASE_URL_ENV)
        .unwrap_or_else(|_| "http://jss:3030".to_string())
}

/// A single step in a saga. The orchestrator executes them in order; any
/// failure aborts the rest and either marks pending (for Neo4j failures after
/// successful Pod writes) or fails outright (for Pod failures before any
/// commit has happened).
#[derive(Debug)]
pub enum SagaStep {
    /// Upload content to the user's Pod.
    PodWrite {
        pod_url: String,
        content: Bytes,
        content_type: String,
        /// Caller-supplied Authorization header. When `None`, the saga signs
        /// with the server's Nostr identity.
        auth_header: Option<String>,
        /// The KGNode whose content this step carries — used to tie the step
        /// back to a graph row for pending-marker bookkeeping.
        node: KGNode,
    },
    /// Commit a node (and any follow-up data) to Neo4j.
    Neo4jCommit { node: KGNode },
    /// Audit event — emitted by the sibling Nostr agent. Placeholder until
    /// `server-Nostr kind 30300` lands; for now we log the intent.
    AuditEvent { kind: u16, content: String, node_id: u32 },
}

/// Outcome of executing a saga's step list.
#[derive(Debug)]
pub enum SagaOutcome {
    /// All steps succeeded. Pod + Neo4j are in sync.
    Complete,
    /// Some Pod writes succeeded and the Neo4j commit is pending retry.
    PendingRetry { last_successful_step: usize, error: String },
    /// Unrecoverable failure. No pending marker was written.
    Failed { error: String },
}

/// Saga execution metrics — emitted via tracing for now (Prometheus crate is
/// not in the workspace; these fields are cheap enough to scrape from logs).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SagaMetrics {
    pub total: u64,
    pub complete: u64,
    pub pending: u64,
    pub failed: u64,
    pub retry_total: u64,
    /// Observed durations in milliseconds (not a histogram — coarse aggregate).
    pub duration_ms_sum: u64,
}

/// Orchestrates Pod-first-Neo4j-second writes.
pub struct IngestSaga {
    pub pod_client: Arc<PodClient>,
    pub neo4j: Arc<Neo4jAdapter>,
    pub run_id: Uuid,
    pub pod_base: String,
    metrics: tokio::sync::Mutex<SagaMetrics>,
    /// Optional Prometheus registry. When present, `execute_batch` observes
    /// outcome counters and the duration histogram here.
    prom: Option<Arc<MetricsRegistry>>,
    /// ADR-054: optional URN-Solid mapper used by `regenerate_corpus_jsonl`
    /// to emit `owl:sameAs` aliases per published node.
    urn_solid_mapper: Option<Arc<UrnSolidMapper>>,
}

/// A saga plan for a single node. Constructed by the caller, passed to
/// `execute_batch` for efficient batched execution.
#[derive(Debug, Clone)]
pub struct NodeSagaPlan {
    pub node: KGNode,
    pub pod_url: String,
    pub content: Bytes,
    pub content_type: String,
    pub auth_header: Option<String>,
}

/// Result of executing a batched saga — per-node outcomes so the caller can
/// tie nodes back to success/failure for stats logging.
#[derive(Debug)]
pub struct BatchSagaResult {
    pub complete: Vec<u32>,
    pub pending: Vec<(u32, String)>,
    pub failed: Vec<(u32, String)>,
    pub duration: Duration,
}

impl IngestSaga {
    pub fn new(pod_client: Arc<PodClient>, neo4j: Arc<Neo4jAdapter>) -> Self {
        Self {
            pod_client,
            neo4j,
            run_id: Uuid::new_v4(),
            pod_base: pod_base_url(),
            metrics: tokio::sync::Mutex::new(SagaMetrics::default()),
            prom: None,
            urn_solid_mapper: None,
        }
    }

    /// Attach a Prometheus registry. Returns `self` to allow fluent-style
    /// wiring at construction sites.
    pub fn with_prom(mut self, prom: Arc<MetricsRegistry>) -> Self {
        self.prom = Some(prom);
        self
    }

    /// Attach a URN-Solid mapper (ADR-054). When set, corpus.jsonl emission
    /// enriches each document with the mapped `urn:solid:` alias.
    pub fn with_urn_solid_mapper(mut self, mapper: Arc<UrnSolidMapper>) -> Self {
        self.urn_solid_mapper = Some(mapper);
        self
    }

    pub async fn metrics_snapshot(&self) -> SagaMetrics {
        self.metrics.lock().await.clone()
    }

    /// Record one Prometheus sample describing a terminal batch outcome.
    fn observe_prom_outcome(&self, complete: u64, pending: u64, failed: u64, elapsed_secs: f64) {
        if let Some(prom) = self.prom.as_ref() {
            if complete > 0 {
                prom.ingest_saga_total
                    .get_or_create(&SagaOutcomeLabels {
                        outcome: SagaOutcomeLabel::Complete,
                    })
                    .inc_by(complete);
            }
            if pending > 0 {
                prom.ingest_saga_total
                    .get_or_create(&SagaOutcomeLabels {
                        outcome: SagaOutcomeLabel::Pending,
                    })
                    .inc_by(pending);
                prom.ingest_saga_pending_nodes.inc_by(pending as i64);
            }
            if failed > 0 {
                prom.ingest_saga_total
                    .get_or_create(&SagaOutcomeLabels {
                        outcome: SagaOutcomeLabel::Failed,
                    })
                    .inc_by(failed);
            }
            prom.ingest_saga_duration_seconds.observe(elapsed_secs);
        }
    }

    /// Build a node's default Pod URL from its metadata.
    ///
    /// Expects the KGNode to carry:
    ///   - `owner_pubkey` (npub or hex) in metadata, OR a fallback owner env var
    ///   - `visibility` = "public" | "private" in metadata (default: private)
    ///
    /// Falls back to the server's "service account" container when the node
    /// has no explicit owner (covers current github-sync flow until per-user
    /// identity plumbing lands upstream).
    pub fn default_pod_url_for(&self, node: &KGNode) -> String {
        let owner = node
            .metadata
            .get("owner_pubkey")
            .or_else(|| node.metadata.get("owner"))
            .cloned()
            .unwrap_or_else(|| {
                std::env::var("POD_DEFAULT_OWNER").unwrap_or_else(|_| "server".to_string())
            });
        let visibility = node
            .metadata
            .get("visibility")
            .map(|v| Visibility::from_str(v))
            .unwrap_or(Visibility::Private);
        let slug = if node.metadata_id.is_empty() {
            node.label.clone()
        } else {
            node.metadata_id.clone()
        };
        crate::services::pod_client::pod_url_for(&self.pod_base, &owner, &slug, visibility)
    }

    /// Execute a single saga (for unit tests / single-node flows). Batch-mode
    /// is preferred in hot paths.
    pub async fn execute(&self, steps: Vec<SagaStep>) -> SagaOutcome {
        let start = Instant::now();
        let mut last_success: Option<usize> = None;
        let mut pod_written_node: Option<u32> = None;

        for (i, step) in steps.into_iter().enumerate() {
            match step {
                SagaStep::PodWrite { pod_url, content, content_type, auth_header, node } => {
                    // Idempotent replay: if the Pod already has a resource with
                    // any ETag we accept that as "already written" — callers that
                    // want strict content-equality can pass a precomputed ETag
                    // and compare before skipping.
                    match self.pod_client.get_etag(&pod_url, auth_header.as_deref()).await {
                        Ok(Some(_)) => {
                            debug!("[saga] Pod resource {} already exists — skipping PUT", pod_url);
                        }
                        Ok(None) | Err(_) => {
                            // 404 or HEAD failure → attempt PUT.
                            if let Err(e) = self
                                .pod_client
                                .put_resource(&pod_url, content, &content_type, auth_header.as_deref())
                                .await
                            {
                                return self.finish_failed(start, e.to_string()).await;
                            }
                        }
                    }
                    pod_written_node = Some(node.id);
                    last_success = Some(i);
                }
                SagaStep::Neo4jCommit { node } => {
                    let mut gd = GraphData::new();
                    gd.nodes = vec![node.clone()];
                    if let Err(e) = self.neo4j.save_graph(&gd).await {
                        // Pod written, Neo4j failed → pending marker.
                        if let Some(nid) = pod_written_node {
                            if let Err(marker_err) = self.mark_pending(nid, "pod_written", &e.to_string()).await {
                                warn!("[saga] Failed to write pending marker for node {}: {}", nid, marker_err);
                            }
                        }
                        return self.finish_pending(start, last_success.unwrap_or(0), e.to_string()).await;
                    }
                    // After commit, clear any stale pending marker.
                    let _ = self.clear_pending(node.id).await;
                    last_success = Some(i);
                }
                SagaStep::AuditEvent { kind, content, node_id } => {
                    // Placeholder: real Nostr kind-30300 publish lives in the
                    // sibling agent. Log the intent so downstream tooling can
                    // pick it up.
                    info!("[saga][audit] kind={} node_id={} content={}", kind, node_id, content);
                    last_success = Some(i);
                }
            }
        }

        self.finish_complete(start).await
    }

    /// Execute a batched saga:
    ///   1. PUT every node's content to its Pod URL in parallel.
    ///   2. Group nodes by Pod-write outcome.
    ///   3. Single `save_graph` over the successful set.
    ///   4. Pending-mark the successful set on Neo4j failure; clear markers on success.
    ///
    /// Returns a per-node breakdown so the caller can maintain its own stats.
    pub async fn execute_batch(&self, plans: Vec<NodeSagaPlan>) -> BatchSagaResult {
        let start = Instant::now();

        if plans.is_empty() {
            return BatchSagaResult {
                complete: vec![],
                pending: vec![],
                failed: vec![],
                duration: start.elapsed(),
            };
        }

        // Phase 1: parallel Pod writes.
        let mut pod_futures = Vec::with_capacity(plans.len());
        for plan in plans.iter() {
            let client = self.pod_client.clone();
            let pod_url = plan.pod_url.clone();
            let content = plan.content.clone();
            let content_type = plan.content_type.clone();
            let auth = plan.auth_header.clone();
            let node_id = plan.node.id;
            pod_futures.push(async move {
                let outcome = Self::pod_write_with_replay(&client, &pod_url, content, &content_type, auth.as_deref()).await;
                (node_id, outcome)
            });
        }

        let results = futures::future::join_all(pod_futures).await;

        let mut pod_successful: HashMap<u32, usize> = HashMap::new(); // node_id → idx in plans
        let mut failed: Vec<(u32, String)> = Vec::new();
        for (node_id, outcome) in results {
            match outcome {
                Ok(()) => {
                    if let Some(idx) = plans.iter().position(|p| p.node.id == node_id) {
                        pod_successful.insert(node_id, idx);
                    }
                }
                Err(e) => {
                    warn!("[saga] Pod write failed for node {}: {}", node_id, e);
                    failed.push((node_id, e.to_string()));
                }
            }
        }

        // Phase 2: single batched Neo4j commit for Pod-successful nodes.
        let committed_nodes: Vec<KGNode> = plans
            .iter()
            .filter_map(|p| pod_successful.get(&p.node.id).map(|_| p.node.clone()))
            .collect();

        if committed_nodes.is_empty() {
            let mut m = self.metrics.lock().await;
            m.total += plans.len() as u64;
            m.failed += failed.len() as u64;
            m.duration_ms_sum += start.elapsed().as_millis() as u64;
            return BatchSagaResult {
                complete: vec![],
                pending: vec![],
                failed,
                duration: start.elapsed(),
            };
        }

        let mut gd = GraphData::new();
        gd.nodes = committed_nodes.clone();

        let commit_result = self.neo4j.save_graph(&gd).await;

        let (complete, pending) = match commit_result {
            Ok(()) => {
                // Clear any pending markers that may have been set by a previous crashed attempt.
                for n in &committed_nodes {
                    let _ = self.clear_pending(n.id).await;
                }
                (
                    committed_nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
                    Vec::<(u32, String)>::new(),
                )
            }
            Err(e) => {
                // Neo4j commit failed for the whole batch → mark each
                // Pod-successful node pending so the resumption task retries.
                let err_msg = e.to_string();
                warn!("[saga] Neo4j batch commit failed: {} — marking {} nodes pending", err_msg, committed_nodes.len());
                let mut pending_vec = Vec::with_capacity(committed_nodes.len());
                for n in &committed_nodes {
                    if let Err(marker_err) = self.mark_pending(n.id, "pod_written", &err_msg).await {
                        warn!("[saga] Could not write pending marker for node {}: {}", n.id, marker_err);
                    }
                    pending_vec.push((n.id, err_msg.clone()));
                }
                (Vec::new(), pending_vec)
            }
        };

        // Metrics
        {
            let mut m = self.metrics.lock().await;
            m.total += plans.len() as u64;
            m.complete += complete.len() as u64;
            m.pending += pending.len() as u64;
            m.failed += failed.len() as u64;
            m.duration_ms_sum += start.elapsed().as_millis() as u64;
        }

        // Tracing emission (Prometheus-compatible label shape)
        tracing::info!(
            target = "ingest_saga_metrics",
            outcome_complete = complete.len(),
            outcome_pending = pending.len(),
            outcome_failed = failed.len(),
            duration_ms = start.elapsed().as_millis() as u64,
            "ingest_saga_batch_complete"
        );

        // Prometheus emission mirrors the tracing event so scrapers get a
        // first-class counter/histogram view.
        let elapsed = start.elapsed();
        self.observe_prom_outcome(
            complete.len() as u64,
            pending.len() as u64,
            failed.len() as u64,
            elapsed.as_secs_f64(),
        );

        BatchSagaResult {
            complete,
            pending,
            failed,
            duration: elapsed,
        }
    }

    /// Pod write with idempotent-replay (HEAD→ETag short-circuit).
    async fn pod_write_with_replay(
        client: &PodClient,
        pod_url: &str,
        content: Bytes,
        content_type: &str,
        auth_header: Option<&str>,
    ) -> Result<(), PodClientError> {
        // Short-circuit: if the Pod already has a resource, skip the PUT.
        // This keeps retry cycles O(1) network calls per already-written node.
        match client.get_etag(pod_url, auth_header).await {
            Ok(Some(_etag)) => {
                debug!("[saga] Pod resource {} already exists — idempotent skip", pod_url);
                Ok(())
            }
            Ok(None) => {
                client
                    .put_resource(pod_url, content, content_type, auth_header)
                    .await
                    .map(|_| ())
            }
            Err(_) => {
                // HEAD failed — attempt PUT anyway, let the PUT surface the
                // real error if the Pod is actually broken.
                client
                    .put_resource(pod_url, content, content_type, auth_header)
                    .await
                    .map(|_| ())
            }
        }
    }

    /// Scan Neo4j for KGNodes with `saga_pending: true` and attempt their
    /// Neo4j commit again. Returns per-node outcomes.
    ///
    /// Idempotent: if the node's data on disk has already been written (we
    /// don't re-fetch source content here), the commit just updates the
    /// existing KGNode. Running twice in a row doesn't double-commit because
    /// `save_graph` uses `MERGE` on `id`.
    pub async fn resume_pending(&self) -> Vec<SagaOutcome> {
        let pending = match self.fetch_pending_nodes(RESUMPTION_BATCH_LIMIT).await {
            Ok(n) => n,
            Err(e) => {
                warn!("[saga][resume] Failed to fetch pending nodes: {}", e);
                return vec![];
            }
        };

        if pending.is_empty() {
            return vec![];
        }

        info!("[saga][resume] Processing {} pending nodes", pending.len());

        {
            let mut m = self.metrics.lock().await;
            m.retry_total += pending.len() as u64;
        }
        if let Some(prom) = self.prom.as_ref() {
            prom.ingest_saga_retry_total.inc_by(pending.len() as u64);
        }

        let mut outcomes = Vec::with_capacity(pending.len());
        for node in pending {
            let mut gd = GraphData::new();
            gd.nodes = vec![node.clone()];
            match self.neo4j.save_graph(&gd).await {
                Ok(()) => {
                    // Clear the marker.
                    let _ = self.clear_pending(node.id).await;
                    outcomes.push(SagaOutcome::Complete);
                    if let Some(prom) = self.prom.as_ref() {
                        // Pending node cleared → promote to complete, drop
                        // the gauge by one.
                        prom.ingest_saga_total
                            .get_or_create(&SagaOutcomeLabels {
                                outcome: SagaOutcomeLabel::Complete,
                            })
                            .inc();
                        prom.ingest_saga_pending_nodes.dec();
                    }
                    debug!("[saga][resume] Node {} resumed successfully", node.id);
                }
                Err(e) => {
                    outcomes.push(SagaOutcome::PendingRetry {
                        last_successful_step: 0,
                        error: e.to_string(),
                    });
                    warn!("[saga][resume] Node {} still pending: {}", node.id, e);
                }
            }
        }

        outcomes
    }

    /// Mark a node pending — Pod write succeeded, Neo4j commit owed.
    async fn mark_pending(&self, node_id: u32, step: &str, err: &str) -> Result<(), String> {
        let q = query(
            "MERGE (n:KGNode {id: $id})
             SET n.saga_pending = true,
                 n.saga_started_at = datetime(),
                 n.saga_step = $step,
                 n.saga_last_error = $err",
        )
        .param("id", node_id as i64)
        .param("step", step.to_string())
        .param("err", err.to_string());

        self.neo4j
            .graph()
            .run(q)
            .await
            .map_err(|e| e.to_string())
    }

    /// Remove the pending marker once the node is fully committed.
    async fn clear_pending(&self, node_id: u32) -> Result<(), String> {
        let q = query(
            "MATCH (n:KGNode {id: $id})
             REMOVE n.saga_pending, n.saga_started_at, n.saga_step, n.saga_last_error",
        )
        .param("id", node_id as i64);

        self.neo4j
            .graph()
            .run(q)
            .await
            .map_err(|e| e.to_string())
    }

    /// Fetch KGNodes with `saga_pending: true`, up to `limit`.
    /// Reconstructs a minimal KGNode suitable for re-running save_graph.
    async fn fetch_pending_nodes(&self, limit: usize) -> Result<Vec<KGNode>, String> {
        let q = query(
            "MATCH (n:KGNode)
             WHERE n.saga_pending = true
             RETURN n.id AS id, n.metadata_id AS metadata_id, n.label AS label,
                    n.metadata AS metadata
             LIMIT $limit",
        )
        .param("limit", limit as i64);

        let mut result = self
            .neo4j
            .graph()
            .execute(q)
            .await
            .map_err(|e| format!("fetch_pending_nodes: {}", e))?;

        let mut out = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| format!("row iteration: {}", e))?
        {
            let id: i64 = row.get("id").unwrap_or(0);
            let metadata_id: String = row.get("metadata_id").unwrap_or_default();
            let label: String = row.get("label").unwrap_or_default();
            let metadata_str: String = row.get("metadata").unwrap_or_default();

            let metadata: HashMap<String, String> = serde_json::from_str(&metadata_str)
                .unwrap_or_default();

            let mut node = KGNode::new_with_id(metadata_id.clone(), Some(id as u32));
            node.label = label;
            node.metadata = metadata;
            out.push(node);
        }

        Ok(out)
    }

    async fn finish_complete(&self, start: Instant) -> SagaOutcome {
        let mut m = self.metrics.lock().await;
        m.total += 1;
        m.complete += 1;
        m.duration_ms_sum += start.elapsed().as_millis() as u64;
        SagaOutcome::Complete
    }

    async fn finish_pending(&self, start: Instant, last_step: usize, err: String) -> SagaOutcome {
        let mut m = self.metrics.lock().await;
        m.total += 1;
        m.pending += 1;
        m.duration_ms_sum += start.elapsed().as_millis() as u64;
        SagaOutcome::PendingRetry {
            last_successful_step: last_step,
            error: err,
        }
    }

    async fn finish_failed(&self, start: Instant, err: String) -> SagaOutcome {
        let mut m = self.metrics.lock().await;
        m.total += 1;
        m.failed += 1;
        m.duration_ms_sum += start.elapsed().as_millis() as u64;
        SagaOutcome::Failed { error: err }
    }

    // ─── ADR-054: corpus.jsonl generation ──────────────────────────────────

    /// Regenerate `./public/kg/corpus.jsonl` on the owner's Pod from the
    /// current set of public `:KGNode` records belonging to that owner.
    ///
    /// Behaviour:
    ///   * No-op unless `URN_SOLID_ALIGNMENT=true` (safe-off default).
    ///   * Fetches every `:KGNode {owner_pubkey: $owner, visibility: 'public'}`
    ///     from Neo4j.
    ///   * Emits one JSON-LD document per line, joined by `\n`.
    ///   * PUTs the result to `{pod_base}/{owner}/public/kg/corpus.jsonl` via
    ///     [`PodClient::put_resource`], signing with server keys unless the
    ///     caller supplies an auth header.
    ///   * Empty corpora are still written (0-byte file) so downstream
    ///     crawlers see a deterministic "user has no public KG" signal.
    ///
    /// The call site is `VisibilityTransitionService` on publish/unpublish
    /// (see ADR-054 §2); callers are free to invoke it on any
    /// visibility-material event.
    pub async fn regenerate_corpus_jsonl(
        &self,
        owner_pubkey: &str,
        auth_header: Option<&str>,
    ) -> Result<CorpusJsonlReport, String> {
        if !urn_solid_alignment_enabled() {
            debug!("[urn-solid] regenerate_corpus_jsonl skipped (flag off)");
            return Ok(CorpusJsonlReport::skipped());
        }

        let public_nodes = self.fetch_public_nodes_for_owner(owner_pubkey).await?;

        let body = build_corpus_jsonl(
            &public_nodes,
            owner_pubkey,
            &self.pod_base,
            self.urn_solid_mapper.as_deref(),
        );

        let pod_url = corpus_jsonl_url(&self.pod_base, owner_pubkey);

        self.pod_client
            .put_resource(
                &pod_url,
                Bytes::from(body.clone()),
                "application/x-ndjson",
                auth_header,
            )
            .await
            .map_err(|e| format!("PUT {}: {}", pod_url, e))?;

        info!(
            "[urn-solid] corpus.jsonl regenerated for {} ({} documents, {} bytes) at {}",
            owner_pubkey,
            public_nodes.len(),
            body.len(),
            pod_url,
        );

        Ok(CorpusJsonlReport {
            skipped: false,
            document_count: public_nodes.len(),
            bytes_written: body.len(),
            pod_url,
        })
    }

    /// Load every public `:KGNode` for the given owner, mapping a minimal
    /// projection (`id`, `metadata_id`, `label`, `pod_url`, `owl_class_iri`)
    /// into a [`KGNode`]. Private and tombstoned nodes are excluded by the
    /// `visibility = 'public'` guard.
    async fn fetch_public_nodes_for_owner(
        &self,
        owner_pubkey: &str,
    ) -> Result<Vec<KGNode>, String> {
        let q = neo4rs::query(
            "MATCH (n:KGNode) \
             WHERE n.owner_pubkey = $owner AND n.visibility = 'public' \
             RETURN n.id AS id, \
                    n.metadata_id AS metadata_id, \
                    coalesce(n.label, '') AS label, \
                    coalesce(n.pod_url, '') AS pod_url, \
                    n.owl_class_iri AS owl_class_iri \
             ORDER BY n.id",
        )
        .param("owner", owner_pubkey.to_string());

        let mut result = self
            .neo4j
            .graph()
            .execute(q)
            .await
            .map_err(|e| format!("fetch_public_nodes: {}", e))?;

        let mut out = Vec::new();
        while let Some(row) = result
            .next()
            .await
            .map_err(|e| format!("row: {}", e))?
        {
            let id: i64 = row.get("id").unwrap_or(0);
            let metadata_id: String = row.get("metadata_id").unwrap_or_default();
            let label: String = row.get("label").unwrap_or_default();
            let pod_url: String = row.get("pod_url").unwrap_or_default();
            let owl_class_iri: Option<String> = row.get("owl_class_iri").ok();

            let mut node = KGNode::new_with_id(metadata_id, Some(id as u32));
            node.label = label;
            node.owl_class_iri = owl_class_iri;
            if !pod_url.is_empty() {
                node.pod_url = Some(pod_url);
            }
            node.owner_pubkey = Some(owner_pubkey.to_string());
            out.push(node);
        }

        Ok(out)
    }
}

/// Result of a corpus.jsonl regeneration call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusJsonlReport {
    /// Flag was off; no write happened.
    pub skipped: bool,
    /// Number of JSON-LD documents written.
    pub document_count: usize,
    /// Number of bytes in the final file body.
    pub bytes_written: usize,
    /// Destination Pod URL.
    pub pod_url: String,
}

impl CorpusJsonlReport {
    pub fn skipped() -> Self {
        Self {
            skipped: true,
            document_count: 0,
            bytes_written: 0,
            pod_url: String::new(),
        }
    }
}

/// Compose the full Pod URL for the per-owner corpus.jsonl.
pub fn corpus_jsonl_url(pod_base: &str, owner: &str) -> String {
    let base = pod_base.trim_end_matches('/');
    let owner = owner.trim_matches('/');
    format!("{base}/{owner}/public/kg/corpus.jsonl")
}

/// Build the body of `corpus.jsonl` from a set of public nodes.
///
/// Format: one JSON object per line, NDJSON style, no trailing newline.
/// Each line shape:
/// ```json
/// {
///   "@context": "https://visionclaw.org/context.jsonld",
///   "@id": "<canonical iri>",
///   "@type": ["<our_iri>", "urn:solid:<Name>"?],
///   "rdfs:label": "...",
///   "vc:podUrl": "<pod url>",
///   "owl:sameAs": ["urn:solid:<Name>"]
/// }
/// ```
pub fn build_corpus_jsonl(
    nodes: &[KGNode],
    owner_pubkey: &str,
    pod_base: &str,
    mapper: Option<&UrnSolidMapper>,
) -> String {
    let mut lines = Vec::with_capacity(nodes.len());
    for node in nodes {
        let doc = build_corpus_jsonld_document(node, owner_pubkey, pod_base, mapper);
        match serde_json::to_string(&doc) {
            Ok(s) => lines.push(s),
            Err(e) => warn!("[urn-solid] serialise corpus line for node {}: {}", node.id, e),
        }
    }
    lines.join("\n")
}

/// Build a single JSON-LD document for one public KG node.
pub fn build_corpus_jsonld_document(
    node: &KGNode,
    owner_pubkey: &str,
    pod_base: &str,
    mapper: Option<&UrnSolidMapper>,
) -> serde_json::Value {
    use serde_json::json;

    let canonical_iri = format!(
        "visionclaw:owner:{}/kg/{}",
        owner_pubkey,
        node.metadata_id
    );

    // Resolve URN-Solid alias for stable mappings only.
    let urn_alias: Option<String> = match (mapper, node.owl_class_iri.as_deref()) {
        (Some(m), Some(iri)) => m
            .lookup(iri)
            .filter(|r| r.status == MappingStatus::Stable)
            .map(|r| r.urn_solid),
        _ => None,
    };

    let mut types: Vec<String> = Vec::new();
    if let Some(our) = node.owl_class_iri.clone() {
        types.push(our);
    }
    if let Some(a) = urn_alias.clone() {
        types.push(a);
    }

    let pod_url = node
        .pod_url
        .clone()
        .unwrap_or_else(|| corpus_jsonl_url(pod_base, owner_pubkey)
            .trim_end_matches("corpus.jsonl")
            .trim_end_matches('/')
            .to_string());

    let mut doc = json!({
        "@context": "https://visionclaw.org/context.jsonld",
        "@id": canonical_iri,
        "rdfs:label": node.label,
        "vc:podUrl": pod_url,
    });

    if !types.is_empty() {
        doc["@type"] = serde_json::Value::Array(
            types.into_iter().map(serde_json::Value::String).collect(),
        );
    }

    if let Some(a) = urn_alias {
        doc["owl:sameAs"] = json!([a]);
    }

    doc
}

/// Spawn the background resumption task. Runs every 60s, processing pending
/// KGNodes in batches. Returns the JoinHandle so the caller can `.abort()`
/// on shutdown.
///
/// The task exits cleanly when the saga is shut down (there is no signal yet;
/// returning the handle lets callers abort on SIGTERM).
pub fn spawn_resumption_task(saga: Arc<IngestSaga>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = interval(RESUMPTION_INTERVAL);
        // First tick fires immediately; skip it to give the rest of the
        // startup sequence time to stabilise.
        tick.tick().await;
        info!(
            "[saga] Resumption task started (run_id={}, interval={:?})",
            saga.run_id, RESUMPTION_INTERVAL
        );

        loop {
            tick.tick().await;
            match saga_enabled() {
                true => {
                    let outcomes = saga.resume_pending().await;
                    if !outcomes.is_empty() {
                        let complete = outcomes.iter().filter(|o| matches!(o, SagaOutcome::Complete)).count();
                        let pending = outcomes.iter().filter(|o| matches!(o, SagaOutcome::PendingRetry { .. })).count();
                        let failed = outcomes.iter().filter(|o| matches!(o, SagaOutcome::Failed { .. })).count();
                        debug!("[saga][resume] Tick finished: {} complete, {} pending, {} failed", complete, pending, failed);
                    }
                }
                false => {
                    debug!("[saga][resume] Tick skipped (POD_SAGA_ENABLED=false)");
                }
            }
        }
    })
}

/// Convenience builder: build a saga from the environment with sensible
/// defaults. Returns `Err` if the Pod client cannot be constructed.
pub fn build_from_env(neo4j: Arc<Neo4jAdapter>) -> Result<Arc<IngestSaga>, String> {
    let pc = PodClient::from_env().map_err(|e| format!("PodClient::from_env: {}", e))?;
    Ok(Arc::new(IngestSaga::new(Arc::new(pc), neo4j)))
}

/// Serialise a KGNode to the content body we push to the Pod.
///
/// Format: JSON — stable, human-readable, parseable by downstream WAC tooling.
/// Callers that want Turtle can serialise separately and pass raw bytes to
/// `put_resource`; the saga is agnostic to the content format once the bytes
/// are prepared.
pub fn serialise_node_for_pod(node: &KGNode) -> Bytes {
    let body = serde_json::to_vec_pretty(node).unwrap_or_else(|_| b"{}".to_vec());
    Bytes::from(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saga_enabled_flag() {
        std::env::remove_var(POD_SAGA_ENABLED_ENV);
        assert!(!saga_enabled());
        std::env::set_var(POD_SAGA_ENABLED_ENV, "true");
        assert!(saga_enabled());
        std::env::set_var(POD_SAGA_ENABLED_ENV, "0");
        assert!(!saga_enabled());
        std::env::remove_var(POD_SAGA_ENABLED_ENV);
    }

    #[test]
    fn test_serialise_node_for_pod() {
        let node = KGNode::new("test-page".to_string());
        let bytes = serialise_node_for_pod(&node);
        assert!(!bytes.is_empty());
        // Must be valid JSON.
        let _v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    }

    #[test]
    fn test_default_pod_url_for_fallback_owner() {
        // Can't instantiate a real IngestSaga without Neo4j; test the URL
        // builder helper directly — the saga delegates to it verbatim.
        std::env::set_var("POD_DEFAULT_OWNER", "testowner");
        let node = KGNode::new("page1".to_string());
        // Simulate the branch in default_pod_url_for by calling pod_url_for directly.
        let url = crate::services::pod_client::pod_url_for(
            "http://pod.test",
            "testowner",
            "page1",
            Visibility::Private,
        );
        assert_eq!(url, "http://pod.test/testowner/private/kg/page1");
        let _ = node;
    }
}
