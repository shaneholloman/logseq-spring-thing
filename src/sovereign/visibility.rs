//! Visibility Transition Service — publish / unpublish saga (ADR-051).
//!
//! Two symmetric flows:
//!
//! * **Publish** (`private → public`): Pod MOVE → Neo4j flip → binary V5
//!   broadcast hint → server-signed audit kind-30300.
//! * **Unpublish** (`public → private`): Pod MOVE → Neo4j flip + tombstone
//!   write → binary V5 broadcast hint → server-signed audit kind-30300.
//!
//! Both obey the Pod-first, Neo4j-second rule from ADR-051 §atomicity:
//!
//! * If the Pod MOVE fails, we **abort early** and never touch Neo4j.
//! * If the Pod MOVE succeeds but the Neo4j flip fails, we stamp a
//!   `saga_pending` marker on the row so the existing [`IngestSaga`] resumer
//!   (`src/services/ingest_saga.rs`) picks it up on its next tick.
//!
//! Broadcast:
//!
//! The V5 binary broadcaster lives in the websocket / graph-service layer and
//! already re-emits nodes when their Neo4j state changes. Per the sprint
//! brief, this module emits a structured `graph.node.published` /
//! `graph.node.unpublished` tracing event so downstream broadcasters can
//! observe without a new message type. Bit 29 of the node id (opaque flag)
//! flips automatically from the Neo4j `visibility` mutation.
//!
//! Audit:
//!
//! A kind-30300 event is signed via the existing [`ServerNostrActor`] using
//! the [`SignAuditRecord`] message — no new actor message is introduced.
//!
//! Tombstone:
//!
//! On unpublish, a `:PodTombstone {path}` node is merged into Neo4j. The
//! `solid_proxy_handler` GET path checks for a tombstone before routing
//! upstream; when found, it returns HTTP 410 Gone with a `Sunset` header.

use std::sync::Arc;

use actix::Addr;
use async_trait::async_trait;
use log::{debug, info, warn};
use neo4rs::{query, Graph};
use serde_json::json;
use thiserror::Error;

use crate::actors::server_nostr_actor::{ServerNostrActor, SignAuditRecord};
use crate::adapters::neo4j_adapter::Neo4jAdapter;
use crate::services::metrics::{MetricsRegistry, SagaOutcomeLabel, SagaOutcomeLabels};
use crate::services::pod_client::{PodClient, PodClientError};

/// Env-var feature flag controlling whether publish/unpublish perform any
/// side-effects. When unset or `false`, the service returns
/// [`VisibilityError::NotEnabled`] at the door.
pub const VISIBILITY_TRANSITIONS_ENV: &str = "VISIBILITY_TRANSITIONS";

/// Returns `true` if the `VISIBILITY_TRANSITIONS` env var is set to a truthy
/// value. Defaults to `false` (safe-off) to preserve existing behaviour.
pub fn visibility_transitions_enabled() -> bool {
    std::env::var(VISIBILITY_TRANSITIONS_ENV)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

/// Errors raised by the transition service.
#[derive(Debug, Error)]
pub enum VisibilityError {
    #[error("visibility transitions disabled (set {VISIBILITY_TRANSITIONS_ENV}=true)")]
    NotEnabled,

    #[error("pod MOVE failed ({from} -> {to}): {source}")]
    PodMove {
        from: String,
        to: String,
        #[source]
        source: PodClientError,
    },

    #[error("neo4j transaction failed: {0}")]
    Neo4j(String),

    #[error("audit emit failed (saga already committed): {0}")]
    AuditEmit(String),
}

pub type VisibilityResult<T> = Result<T, VisibilityError>;

/// Publish a node (private → public). Pod MOVE first, Neo4j flip second.
#[derive(Debug, Clone)]
pub struct PublishRequest {
    /// KGNode numeric id (primary key on `:KGNode`).
    pub node_id: u32,
    /// Nostr hex pubkey of the owner. Threaded into the audit record.
    pub owner_pubkey: String,
    /// Full Pod URL of the resource in its current (private) container.
    pub current_path: String,
    /// Full Pod URL of the destination (public) container.
    pub target_path: String,
    /// The real label to restore on the node once it becomes public again.
    pub real_label: String,
}

/// Unpublish a node (public → private). Pod MOVE first, Neo4j flip +
/// tombstone second.
#[derive(Debug, Clone)]
pub struct UnpublishRequest {
    pub node_id: u32,
    pub owner_pubkey: String,
    /// Full Pod URL of the resource in its current (public) container.
    pub current_path: String,
    /// Full Pod URL of the destination (private) container.
    pub target_path: String,
}

// ──────────────────────────────────────────────────────────────────────────
// Neo4j abstraction
// ──────────────────────────────────────────────────────────────────────────

/// Narrow trait capturing the Neo4j mutations the saga performs. Exists so
/// tests can exercise the orchestrator without a live Neo4j; production code
/// uses the implementation on [`Neo4jAdapter`] below.
#[async_trait]
pub trait VisibilityNeo4jOps: Send + Sync + 'static {
    /// Flip `:KGNode` visibility to `public`, clear `opaque_id`, restore
    /// `label`, swap `pod_url`.
    async fn flip_to_public(
        &self,
        node_id: u32,
        real_label: &str,
        new_pod_url: &str,
    ) -> Result<(), String>;

    /// Flip `:KGNode` visibility to `private`, swap `pod_url` to the private
    /// container. Opacity enforcement (regenerating `opaque_id`, stripping
    /// `label` on-the-wire) is handled by downstream readers at serialization
    /// time, so we only need to record the canonical state change here.
    async fn flip_to_private(&self, node_id: u32, new_pod_url: &str) -> Result<(), String>;

    /// Stamp the saga-pending marker for a node whose Pod write succeeded
    /// but whose Neo4j flip failed. The existing `IngestSaga::resume_pending`
    /// task picks these up.
    async fn mark_saga_pending(
        &self,
        node_id: u32,
        saga_step: &str,
        err: &str,
    ) -> Result<(), String>;

    /// Upsert a `:PodTombstone {path}` row. The proxy handler consults this
    /// before forwarding GETs to JSS.
    async fn write_tombstone(&self, old_public_path: &str, owner_pubkey: &str)
        -> Result<(), String>;

    /// Returns `true` if a `:PodTombstone` exists for the given public path.
    /// Used by the proxy's GET handler.
    async fn is_tombstoned(&self, old_public_path: &str) -> Result<bool, String>;

    /// Returns the `deleted_at` ISO-8601 timestamp for a tombstone, if any.
    async fn tombstone_sunset(&self, old_public_path: &str) -> Result<Option<String>, String>;
}

#[async_trait]
impl VisibilityNeo4jOps for Neo4jAdapter {
    async fn flip_to_public(
        &self,
        node_id: u32,
        real_label: &str,
        new_pod_url: &str,
    ) -> Result<(), String> {
        run_void(
            self.graph(),
            query(
                "MATCH (n:KGNode {id: $id}) \
                 SET n.visibility = 'public', \
                     n.opaque_id = NULL, \
                     n.label = $label, \
                     n.pod_url = $new_url",
            )
            .param("id", node_id as i64)
            .param("label", real_label.to_string())
            .param("new_url", new_pod_url.to_string()),
        )
        .await
    }

    async fn flip_to_private(&self, node_id: u32, new_pod_url: &str) -> Result<(), String> {
        run_void(
            self.graph(),
            query(
                "MATCH (n:KGNode {id: $id}) \
                 SET n.visibility = 'private', \
                     n.pod_url = $new_url",
            )
            .param("id", node_id as i64)
            .param("new_url", new_pod_url.to_string()),
        )
        .await
    }

    async fn mark_saga_pending(
        &self,
        node_id: u32,
        saga_step: &str,
        err: &str,
    ) -> Result<(), String> {
        run_void(
            self.graph(),
            query(
                "MERGE (n:KGNode {id: $id}) \
                 SET n.saga_pending = true, \
                     n.saga_started_at = datetime(), \
                     n.saga_step = $step, \
                     n.saga_last_error = $err",
            )
            .param("id", node_id as i64)
            .param("step", saga_step.to_string())
            .param("err", err.to_string()),
        )
        .await
    }

    async fn write_tombstone(
        &self,
        old_public_path: &str,
        owner_pubkey: &str,
    ) -> Result<(), String> {
        run_void(
            self.graph(),
            query(
                "MERGE (t:PodTombstone {path: $path}) \
                 SET t.deleted_at = datetime(), \
                     t.owner_pubkey = $owner",
            )
            .param("path", old_public_path.to_string())
            .param("owner", owner_pubkey.to_string()),
        )
        .await
    }

    async fn is_tombstoned(&self, old_public_path: &str) -> Result<bool, String> {
        let q = query(
            "MATCH (t:PodTombstone {path: $path}) RETURN count(t) AS c",
        )
        .param("path", old_public_path.to_string());

        let mut result = self
            .graph()
            .execute(q)
            .await
            .map_err(|e| format!("is_tombstoned: {e}"))?;

        if let Some(row) = result
            .next()
            .await
            .map_err(|e| format!("is_tombstoned row: {e}"))?
        {
            let c: i64 = row.get("c").unwrap_or(0);
            return Ok(c > 0);
        }
        Ok(false)
    }

    async fn tombstone_sunset(
        &self,
        old_public_path: &str,
    ) -> Result<Option<String>, String> {
        let q = query(
            "MATCH (t:PodTombstone {path: $path}) \
             RETURN toString(t.deleted_at) AS ts",
        )
        .param("path", old_public_path.to_string());

        let mut result = self
            .graph()
            .execute(q)
            .await
            .map_err(|e| format!("tombstone_sunset: {e}"))?;

        if let Some(row) = result
            .next()
            .await
            .map_err(|e| format!("tombstone_sunset row: {e}"))?
        {
            let ts: String = row.get("ts").unwrap_or_default();
            if ts.is_empty() {
                return Ok(None);
            }
            return Ok(Some(ts));
        }
        Ok(None)
    }
}

/// Small helper: run a parameterised Cypher statement and discard the result.
async fn run_void(graph: &Arc<Graph>, q: neo4rs::Query) -> Result<(), String> {
    graph.run(q).await.map_err(|e| e.to_string())
}

// ──────────────────────────────────────────────────────────────────────────
// Service
// ──────────────────────────────────────────────────────────────────────────

/// Orchestrates publish / unpublish transitions. Construct once at startup
/// and register as `web::Data<Arc<VisibilityTransitionService>>`.
pub struct VisibilityTransitionService {
    pod_client: Arc<PodClient>,
    neo4j: Arc<dyn VisibilityNeo4jOps>,
    server_nostr: Addr<ServerNostrActor>,
    metrics: Option<Arc<MetricsRegistry>>,
}

impl VisibilityTransitionService {
    /// Build a production service that talks to a real Neo4jAdapter.
    pub fn new(
        pod_client: Arc<PodClient>,
        neo4j: Arc<Neo4jAdapter>,
        server_nostr: Addr<ServerNostrActor>,
        metrics: Option<Arc<MetricsRegistry>>,
    ) -> Self {
        Self {
            pod_client,
            neo4j: neo4j as Arc<dyn VisibilityNeo4jOps>,
            server_nostr,
            metrics,
        }
    }

    /// Build a service with an injectable Neo4j trait object. Used by tests.
    pub fn with_ops(
        pod_client: Arc<PodClient>,
        neo4j: Arc<dyn VisibilityNeo4jOps>,
        server_nostr: Addr<ServerNostrActor>,
        metrics: Option<Arc<MetricsRegistry>>,
    ) -> Self {
        Self {
            pod_client,
            neo4j,
            server_nostr,
            metrics,
        }
    }

    /// Access the underlying Neo4j ops handle — used by the solid proxy's
    /// tombstone check to avoid wiring a second `web::Data`.
    pub fn neo4j_ops(&self) -> Arc<dyn VisibilityNeo4jOps> {
        Arc::clone(&self.neo4j)
    }

    // ─── Publish ───────────────────────────────────────────────────────

    /// Publish (private → public). Pod MOVE first; on success flip Neo4j,
    /// emit V5 broadcast hint, sign audit event.
    pub async fn publish(&self, req: PublishRequest) -> VisibilityResult<()> {
        if !visibility_transitions_enabled() {
            return Err(VisibilityError::NotEnabled);
        }

        info!(
            "[visibility] publish node_id={} {} -> {}",
            req.node_id, req.current_path, req.target_path
        );

        // 1. Pod MOVE. If this fails, no Neo4j change, no audit.
        if let Err(e) = self
            .pod_client
            .move_resource(&req.current_path, &req.target_path, None)
            .await
        {
            self.observe_failure();
            return Err(VisibilityError::PodMove {
                from: req.current_path.clone(),
                to: req.target_path.clone(),
                source: e,
            });
        }

        // 2. Neo4j flip.
        match self
            .neo4j
            .flip_to_public(req.node_id, &req.real_label, &req.target_path)
            .await
        {
            Ok(()) => {
                self.observe_complete();
            }
            Err(e) => {
                warn!(
                    "[visibility] Pod MOVE succeeded but Neo4j flip failed for node {}: {} — marking saga_pending",
                    req.node_id, e
                );
                if let Err(marker_err) = self
                    .neo4j
                    .mark_saga_pending(req.node_id, "published_pod", &e)
                    .await
                {
                    warn!(
                        "[visibility] Could not stamp saga_pending for node {}: {}",
                        req.node_id, marker_err
                    );
                }
                self.observe_pending();
                return Err(VisibilityError::Neo4j(e));
            }
        }

        // 3. Binary V5 broadcast hint. The websocket/graph-service layer
        // subscribes to tracing events and re-emits the node record with
        // bit 29 cleared when it sees this target.
        tracing::info!(
            target = "graph.node.published",
            node_id = req.node_id,
            pod_url = %req.target_path,
            owner_pubkey = %req.owner_pubkey,
            "visibility.publish"
        );

        // 4. Server-signed audit kind-30300 via the existing actor path.
        if let Err(e) = self
            .server_nostr
            .send(SignAuditRecord {
                action: "publish".to_string(),
                actor_pubkey: Some(req.owner_pubkey.clone()),
                details: json!({
                    "node_id": req.node_id,
                    "old_pod_url": req.current_path,
                    "new_pod_url": req.target_path,
                    "real_label": req.real_label,
                }),
            })
            .await
        {
            // Saga already committed — report but do not fail the caller.
            warn!(
                "[visibility] Audit event mailbox error for publish of node {}: {}",
                req.node_id, e
            );
            return Err(VisibilityError::AuditEmit(e.to_string()));
        }

        Ok(())
    }

    // ─── Unpublish ─────────────────────────────────────────────────────

    /// Unpublish (public → private). Pod MOVE first; on success flip Neo4j,
    /// write tombstone for 410 Gone, emit V5 broadcast hint, sign audit
    /// event.
    pub async fn unpublish(&self, req: UnpublishRequest) -> VisibilityResult<()> {
        if !visibility_transitions_enabled() {
            return Err(VisibilityError::NotEnabled);
        }

        info!(
            "[visibility] unpublish node_id={} {} -> {}",
            req.node_id, req.current_path, req.target_path
        );

        // 1. Pod MOVE. If this fails, no Neo4j change, no tombstone, no audit.
        if let Err(e) = self
            .pod_client
            .move_resource(&req.current_path, &req.target_path, None)
            .await
        {
            self.observe_failure();
            return Err(VisibilityError::PodMove {
                from: req.current_path.clone(),
                to: req.target_path.clone(),
                source: e,
            });
        }

        // 2. Neo4j flip to private + tombstone merge. On flip failure we mark
        //    saga_pending; on tombstone failure we log but do not abort (the
        //    pod has already moved and Neo4j is consistent; the tombstone is
        //    a downstream 410 optimisation and replay-safe via MERGE).
        match self
            .neo4j
            .flip_to_private(req.node_id, &req.target_path)
            .await
        {
            Ok(()) => {
                if let Err(tomb_err) = self
                    .neo4j
                    .write_tombstone(&req.current_path, &req.owner_pubkey)
                    .await
                {
                    warn!(
                        "[visibility] Tombstone write failed for {}: {} — 410 Gone not active",
                        req.current_path, tomb_err
                    );
                }
                self.observe_complete();
            }
            Err(e) => {
                warn!(
                    "[visibility] Pod MOVE succeeded but Neo4j flip failed for node {}: {} — marking saga_pending",
                    req.node_id, e
                );
                if let Err(marker_err) = self
                    .neo4j
                    .mark_saga_pending(req.node_id, "unpublished_pod", &e)
                    .await
                {
                    warn!(
                        "[visibility] Could not stamp saga_pending for node {}: {}",
                        req.node_id, marker_err
                    );
                }
                self.observe_pending();
                return Err(VisibilityError::Neo4j(e));
            }
        }

        // 3. Binary V5 broadcast hint — bit 29 flips back to opaque.
        tracing::info!(
            target = "graph.node.unpublished",
            node_id = req.node_id,
            old_pod_url = %req.current_path,
            new_pod_url = %req.target_path,
            owner_pubkey = %req.owner_pubkey,
            "visibility.unpublish"
        );

        // 4. Server-signed audit kind-30300.
        if let Err(e) = self
            .server_nostr
            .send(SignAuditRecord {
                action: "unpublish".to_string(),
                actor_pubkey: Some(req.owner_pubkey.clone()),
                details: json!({
                    "node_id": req.node_id,
                    "old_pod_url": req.current_path,
                    "new_pod_url": req.target_path,
                }),
            })
            .await
        {
            warn!(
                "[visibility] Audit event mailbox error for unpublish of node {}: {}",
                req.node_id, e
            );
            return Err(VisibilityError::AuditEmit(e.to_string()));
        }

        Ok(())
    }

    // ─── Metrics helpers ───────────────────────────────────────────────

    fn observe_complete(&self) {
        if let Some(m) = self.metrics.as_ref() {
            m.ingest_saga_total
                .get_or_create(&SagaOutcomeLabels {
                    outcome: SagaOutcomeLabel::Complete,
                })
                .inc();
        }
    }

    fn observe_pending(&self) {
        if let Some(m) = self.metrics.as_ref() {
            m.ingest_saga_total
                .get_or_create(&SagaOutcomeLabels {
                    outcome: SagaOutcomeLabel::Pending,
                })
                .inc();
            m.ingest_saga_pending_nodes.inc();
        }
    }

    fn observe_failure(&self) {
        if let Some(m) = self.metrics.as_ref() {
            m.ingest_saga_total
                .get_or_create(&SagaOutcomeLabels {
                    outcome: SagaOutcomeLabel::Failed,
                })
                .inc();
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Unit tests (trait-level; no live Neo4j, no live Pod)
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_defaults_to_disabled() {
        std::env::remove_var(VISIBILITY_TRANSITIONS_ENV);
        assert!(!visibility_transitions_enabled());
    }

    #[test]
    fn flag_accepts_truthy_values() {
        std::env::set_var(VISIBILITY_TRANSITIONS_ENV, "true");
        assert!(visibility_transitions_enabled());
        std::env::set_var(VISIBILITY_TRANSITIONS_ENV, "1");
        assert!(visibility_transitions_enabled());
        std::env::set_var(VISIBILITY_TRANSITIONS_ENV, "on");
        assert!(visibility_transitions_enabled());
        std::env::set_var(VISIBILITY_TRANSITIONS_ENV, "no");
        assert!(!visibility_transitions_enabled());
        std::env::remove_var(VISIBILITY_TRANSITIONS_ENV);
    }

    #[test]
    fn request_types_are_debug_clone() {
        let p = PublishRequest {
            node_id: 7,
            owner_pubkey: "deadbeef".into(),
            current_path: "http://pod/x/private/kg/a".into(),
            target_path: "http://pod/x/public/kg/a".into(),
            real_label: "A".into(),
        };
        let _ = p.clone();
        assert!(format!("{:?}", p).contains("PublishRequest"));

        let u = UnpublishRequest {
            node_id: 7,
            owner_pubkey: "deadbeef".into(),
            current_path: "http://pod/x/public/kg/a".into(),
            target_path: "http://pod/x/private/kg/a".into(),
        };
        let _ = u.clone();
        assert!(format!("{:?}", u).contains("UnpublishRequest"));
    }
}
