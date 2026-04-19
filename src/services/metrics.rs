//! Prometheus / OpenMetrics registry for VisionClaw's sovereign-mesh plane.
//!
//! Exposes 24 first-class counters/gauges/histograms covering:
//!   * NIP-98 authentication paths (ADR-028-ext)
//!   * Pod-first ingest saga (ADR-051)
//!   * Ingest parser visibility partition (ADR-051)
//!   * BRIDGE_TO promotion pipeline (ADR-051 §bridge)
//!   * Orphan retraction hygiene
//!   * Pod client I/O (PUT/MOVE)
//!   * Server-Nostr signed events (ADR-050 §server-identity)
//!   * solid-pod-rs WAC decisions (ADR-053)
//!
//! Call-sites update metrics directly by retrieving the shared
//! `web::Data<Arc<MetricsRegistry>>` from `HttpRequest::app_data` or by holding
//! a clone of the `Arc`. All metrics are initialised at construction — the
//! registry is registration-complete after `MetricsRegistry::new()` returns.
//!
//! The `METRICS_ENABLED=false` env var inhibits the `render_text()` endpoint
//! output (returns an empty body with a "# metrics disabled" comment). The
//! counters themselves are cheap and always track — the flag is intended as a
//! kill-switch for the `/metrics` endpoint, not an instrumentation gate.

use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;

// ── Label sets ─────────────────────────────────────────────────────────────

/// Label set for saga outcome counter (`complete|pending|failed`).
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct SagaOutcomeLabels {
    pub outcome: SagaOutcomeLabel,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum SagaOutcomeLabel {
    Complete,
    Pending,
    Failed,
}

/// Label set for pod PUT counter (`public|private|config`).
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct PodContainerLabels {
    pub container: PodContainer,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum PodContainer {
    Public,
    Private,
    Config,
}

/// Label set for server-nostr kind counter.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct NostrKindLabels {
    pub kind: NostrKind,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum NostrKind {
    K30023,
    K30100,
    K30200,
    K30300,
}

// ── Registry ───────────────────────────────────────────────────────────────

/// Central metric registry for VisionClaw's sovereign-mesh observability.
///
/// All counters/gauges/histograms are lazily-incrementable via `Arc<Self>`;
/// the underlying `prometheus_client` types use `AtomicU64` / `AtomicI64`
/// so updates are lock-free.
pub struct MetricsRegistry {
    /// Underlying OpenMetrics registry. Exposed for advanced integrations
    /// (e.g. spawning additional collectors), but the common path is to
    /// call [`MetricsRegistry::render_text`].
    pub registry: Registry,

    // ── Auth (ADR-028-ext) ───────────────────────────────────────────
    pub auth_nip98_success_total: Counter,
    pub auth_nip98_failure_total: Counter,
    pub auth_legacy_fallback_total: Counter,
    pub auth_anonymous_total: Counter,

    // ── Pod saga (ADR-051) ───────────────────────────────────────────
    pub ingest_saga_total: Family<SagaOutcomeLabels, Counter>,
    pub ingest_saga_pending_nodes: Gauge,
    pub ingest_saga_retry_total: Counter,
    pub ingest_saga_duration_seconds: Histogram,

    // ── Ingest parser (ADR-051) ──────────────────────────────────────
    pub ingest_nodes_public_total: Counter,
    pub ingest_nodes_private_total: Counter,
    pub ingest_wikilink_stubs_total: Counter,

    // ── Bridge edge (ADR-051 §bridge) ────────────────────────────────
    pub bridge_candidates_surfaced_total: Counter,
    pub bridge_promotions_total: Counter,
    pub bridge_expired_total: Counter,
    pub bridge_confidence_histogram: Histogram,

    // ── Orphan retraction ────────────────────────────────────────────
    pub orphan_wikilinkref_removed_total: Counter,
    pub orphan_stubs_removed_total: Counter,

    // ── Pod client ───────────────────────────────────────────────────
    pub pod_put_total: Family<PodContainerLabels, Counter>,
    pub pod_put_errors_total: Counter,
    pub pod_move_total: Counter,

    // ── Server-Nostr (ADR-050 §server-identity) ──────────────────────
    pub server_nostr_signed_total: Family<NostrKindLabels, Counter>,
    pub server_nostr_broadcast_errors_total: Counter,

    // ── solid-pod-rs (ADR-053) ───────────────────────────────────────
    pub solid_pod_rs_requests_total: Counter,
    pub solid_pod_rs_wac_denied_total: Counter,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    /// Build every metric with its help string and register it against the
    /// shared OpenMetrics registry. Idempotent construction — new instances
    /// are independent.
    pub fn new() -> Self {
        let mut registry = Registry::with_prefix("visionclaw");

        // Auth
        let auth_nip98_success_total = Counter::default();
        let auth_nip98_failure_total = Counter::default();
        let auth_legacy_fallback_total = Counter::default();
        let auth_anonymous_total = Counter::default();

        registry.register(
            "auth_nip98_success_total",
            "Count of NIP-98 Schnorr auth validations that passed",
            auth_nip98_success_total.clone(),
        );
        registry.register(
            "auth_nip98_failure_total",
            "Count of NIP-98 Schnorr auth validations that failed (malformed header or invalid signature)",
            auth_nip98_failure_total.clone(),
        );
        registry.register(
            "auth_legacy_fallback_total",
            "Count of requests that fell back to the legacy X-Nostr-Pubkey+X-Nostr-Token path (dev only)",
            auth_legacy_fallback_total.clone(),
        );
        registry.register(
            "auth_anonymous_total",
            "Count of anonymous pass-through requests on RequireAuth::optional() scopes",
            auth_anonymous_total.clone(),
        );

        // Saga
        let ingest_saga_total: Family<SagaOutcomeLabels, Counter> = Family::default();
        let ingest_saga_pending_nodes = Gauge::default();
        let ingest_saga_retry_total = Counter::default();
        let ingest_saga_duration_seconds = Histogram::new(
            // Buckets from 1ms → ~16s, doubling. Covers single-node sagas
            // (tens of ms) through batch commits (multi-second).
            exponential_buckets(0.001, 2.0, 14),
        );

        registry.register(
            "ingest_saga_total",
            "Pod-first ingest saga outcomes, partitioned by terminal state",
            ingest_saga_total.clone(),
        );
        registry.register(
            "ingest_saga_pending_nodes",
            "Current number of KGNodes carrying the saga_pending marker",
            ingest_saga_pending_nodes.clone(),
        );
        registry.register(
            "ingest_saga_retry_total",
            "Count of resumption-task retries for pending saga nodes",
            ingest_saga_retry_total.clone(),
        );
        registry.register(
            "ingest_saga_duration_seconds",
            "Wall-clock duration of a saga execute_batch call",
            ingest_saga_duration_seconds.clone(),
        );

        // Parser
        let ingest_nodes_public_total = Counter::default();
        let ingest_nodes_private_total = Counter::default();
        let ingest_wikilink_stubs_total = Counter::default();

        registry.register(
            "ingest_nodes_public_total",
            "Count of KGNodes ingested with visibility=public",
            ingest_nodes_public_total.clone(),
        );
        registry.register(
            "ingest_nodes_private_total",
            "Count of KGNodes ingested with visibility=private",
            ingest_nodes_private_total.clone(),
        );
        registry.register(
            "ingest_wikilink_stubs_total",
            "Count of private-stub KGNodes materialised to satisfy a WikilinkRef edge",
            ingest_wikilink_stubs_total.clone(),
        );

        // Bridge
        let bridge_candidates_surfaced_total = Counter::default();
        let bridge_promotions_total = Counter::default();
        let bridge_expired_total = Counter::default();
        let bridge_confidence_histogram = Histogram::new(
            // Confidence ∈ [0, 1]; 10 linearly-spaced buckets.
            [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0].into_iter(),
        );

        registry.register(
            "bridge_candidates_surfaced_total",
            "Count of MigrationCandidates surfaced via BRIDGE_CANDIDATE MERGE",
            bridge_candidates_surfaced_total.clone(),
        );
        registry.register(
            "bridge_promotions_total",
            "Count of BRIDGE_TO edges promoted (monotonic)",
            bridge_promotions_total.clone(),
        );
        registry.register(
            "bridge_expired_total",
            "Count of BRIDGE_CANDIDATE edges auto-expired due to sub-threshold staleness",
            bridge_expired_total.clone(),
        );
        registry.register(
            "bridge_confidence_histogram",
            "Distribution of BRIDGE_TO promotion confidence scores",
            bridge_confidence_histogram.clone(),
        );

        // Orphan retraction
        let orphan_wikilinkref_removed_total = Counter::default();
        let orphan_stubs_removed_total = Counter::default();

        registry.register(
            "orphan_wikilinkref_removed_total",
            "Count of stale WikilinkRef edges retracted by the orphan sweeper",
            orphan_wikilinkref_removed_total.clone(),
        );
        registry.register(
            "orphan_stubs_removed_total",
            "Count of private-stub KGNodes deleted after losing all inbound refs",
            orphan_stubs_removed_total.clone(),
        );

        // Pod client
        let pod_put_total: Family<PodContainerLabels, Counter> = Family::default();
        let pod_put_errors_total = Counter::default();
        let pod_move_total = Counter::default();

        registry.register(
            "pod_put_total",
            "Count of PUT requests issued to the Solid Pod, partitioned by target container",
            pod_put_total.clone(),
        );
        registry.register(
            "pod_put_errors_total",
            "Count of Pod PUT operations that returned a client error",
            pod_put_errors_total.clone(),
        );
        registry.register(
            "pod_move_total",
            "Count of Pod MOVE (container re-parenting) operations",
            pod_move_total.clone(),
        );

        // Server-Nostr
        let server_nostr_signed_total: Family<NostrKindLabels, Counter> = Family::default();
        let server_nostr_broadcast_errors_total = Counter::default();

        registry.register(
            "server_nostr_signed_total",
            "Count of server-signed Nostr events, partitioned by kind",
            server_nostr_signed_total.clone(),
        );
        registry.register(
            "server_nostr_broadcast_errors_total",
            "Count of server-signed Nostr events that failed to broadcast to configured relays",
            server_nostr_broadcast_errors_total.clone(),
        );

        // solid-pod-rs
        let solid_pod_rs_requests_total = Counter::default();
        let solid_pod_rs_wac_denied_total = Counter::default();

        registry.register(
            "solid_pod_rs_requests_total",
            "Count of requests served by the embedded solid-pod-rs subsystem",
            solid_pod_rs_requests_total.clone(),
        );
        registry.register(
            "solid_pod_rs_wac_denied_total",
            "Count of requests denied by solid-pod-rs WAC evaluation",
            solid_pod_rs_wac_denied_total.clone(),
        );

        Self {
            registry,
            auth_nip98_success_total,
            auth_nip98_failure_total,
            auth_legacy_fallback_total,
            auth_anonymous_total,
            ingest_saga_total,
            ingest_saga_pending_nodes,
            ingest_saga_retry_total,
            ingest_saga_duration_seconds,
            ingest_nodes_public_total,
            ingest_nodes_private_total,
            ingest_wikilink_stubs_total,
            bridge_candidates_surfaced_total,
            bridge_promotions_total,
            bridge_expired_total,
            bridge_confidence_histogram,
            orphan_wikilinkref_removed_total,
            orphan_stubs_removed_total,
            pod_put_total,
            pod_put_errors_total,
            pod_move_total,
            server_nostr_signed_total,
            server_nostr_broadcast_errors_total,
            solid_pod_rs_requests_total,
            solid_pod_rs_wac_denied_total,
        }
    }

    /// Render the current metric snapshot in Prometheus text exposition
    /// format (compatible with OpenMetrics). Honours `METRICS_ENABLED=false`
    /// by short-circuiting to a disabled-marker comment so scrapers receive a
    /// valid but inert payload.
    pub fn render_text(&self) -> String {
        if !metrics_enabled() {
            return "# metrics disabled via METRICS_ENABLED=false\n".to_string();
        }
        let mut buf = String::new();
        // encode() is infallible on String (all writes succeed); fall back to
        // empty body defensively.
        if encode(&mut buf, &self.registry).is_err() {
            return "# metrics encoding error\n".to_string();
        }
        buf
    }
}

/// Returns true when `METRICS_ENABLED` is unset or anything other than
/// "false" / "0" (metrics are on by default).
pub fn metrics_enabled() -> bool {
    match std::env::var("METRICS_ENABLED") {
        Ok(v) => !matches!(v.to_ascii_lowercase().as_str(), "false" | "0" | "off" | "no"),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_and_renders_nonempty_text() {
        let r = MetricsRegistry::new();
        // Touch a counter so at least one non-zero sample exists.
        r.auth_nip98_success_total.inc();
        let body = r.render_text();
        assert!(
            body.contains("visionclaw_auth_nip98_success_total"),
            "expected the counter HELP/line to be present; got: {}",
            body
        );
    }

    #[test]
    fn saga_outcome_labels_partition() {
        let r = MetricsRegistry::new();
        r.ingest_saga_total
            .get_or_create(&SagaOutcomeLabels {
                outcome: SagaOutcomeLabel::Complete,
            })
            .inc();
        r.ingest_saga_total
            .get_or_create(&SagaOutcomeLabels {
                outcome: SagaOutcomeLabel::Failed,
            })
            .inc_by(3);
        let body = r.render_text();
        assert!(body.contains("ingest_saga_total"));
    }

    #[test]
    fn metrics_enabled_defaults_true() {
        std::env::remove_var("METRICS_ENABLED");
        assert!(metrics_enabled());
        std::env::set_var("METRICS_ENABLED", "false");
        assert!(!metrics_enabled());
        std::env::set_var("METRICS_ENABLED", "true");
        assert!(metrics_enabled());
        std::env::remove_var("METRICS_ENABLED");
    }

    #[test]
    fn disabled_render_returns_marker() {
        std::env::set_var("METRICS_ENABLED", "false");
        let r = MetricsRegistry::new();
        let body = r.render_text();
        assert!(body.starts_with("# metrics disabled"));
        std::env::remove_var("METRICS_ENABLED");
    }
}
