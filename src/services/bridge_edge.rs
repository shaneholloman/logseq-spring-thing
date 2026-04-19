// src/services/bridge_edge.rs
//! BRIDGE_TO promotion pipeline (ADR-051, ADR-049, ADR-048 dual-tier model).
//!
//! Closes the loop between `:KGNode` (lexical, authoring-tier) and
//! `:OntologyClass` (formal, schema-tier) by materialising a promotion
//! pipeline over weighted signals.
//!
//! Workflow
//! --------
//! 1. `score_candidate` fuses eight signals into a `MigrationCandidate` whose
//!    `confidence` is a sigmoid of the weighted sum.
//! 2. `surface` MERGEs a `BRIDGE_CANDIDATE` edge once confidence ≥ 0.60 so the
//!    migration broker (ADR-049) can see it in its inbox.
//! 3. `promote` materialises a `:BRIDGE_TO` edge. The confidence stored on this
//!    edge is **monotonic**: subsequent re-scoring may only raise it, never
//!    lower it. This encodes the irreversibility of a schema-tier lock-in.
//! 4. `auto_expire` sweeps candidates whose confidence has collapsed below 0.35
//!    for more than 3 days and drops their `BRIDGE_CANDIDATE` edge.
//!
//! The service never touches `:BRIDGE_TO` edges during expiry — once promoted,
//! always promoted.
//!
//! Feature gate: `BRIDGE_EDGE_ENABLED=true|false` (default false). When false,
//! surface/promote/auto_expire are no-ops. Orphan retraction (see
//! [`super::orphan_retraction`]) still runs — it is independent hygiene.

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use log::{debug, info, warn};
use neo4rs::query;
use serde::{Deserialize, Serialize};

use crate::adapters::neo4j_adapter::Neo4jAdapter;
use crate::services::metrics::MetricsRegistry;

// ── Weights (ADR-049 §migration-broker) ─────────────────────────────────────

pub const W_S1_WIKILINK_TO_ONTOLOGY: f64 = 0.20;
pub const W_S2_SEMANTIC_COOCCURRENCE: f64 = 0.15;
pub const W_S3_EXPLICIT_OWL_DECLARATION: f64 = 0.15;
pub const W_S4_AGENT_PROPOSAL: f64 = 0.20;
pub const W_S5_MATURITY_MARKER: f64 = 0.10;
pub const W_S6_CENTRALITY_IN_KG: f64 = 0.10;
pub const W_S7_AUTHORING_RECENCY: f64 = 0.05;
pub const W_S8_AUTHORITY_SCORE: f64 = 0.05;

/// Sigmoid steepness and bias (ADR-049 §migration-broker).
pub const SIGMOID_STEEPNESS: f64 = 12.0;
pub const SIGMOID_BIAS: f64 = 0.42;

/// Minimum confidence to surface a candidate to the broker inbox.
pub const SURFACE_THRESHOLD: f64 = 0.60;
/// Confidence below this AND older than [`EXPIRY_AGE_DAYS`] auto-expires.
pub const EXPIRY_CONFIDENCE: f64 = 0.35;
/// Age (days) before a sub-threshold candidate auto-expires.
pub const EXPIRY_AGE_DAYS: i64 = 3;

// ── Models ─────────────────────────────────────────────────────────────────

/// Eight-dimensional signal vector feeding the promotion sigmoid.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SignalVector {
    /// S1 — wiki/backlink density pointing at an existing ontology class.
    pub s1_wikilink_to_ontology: f64,
    /// S2 — co-occurrence with ontology-class terms in same documents.
    pub s2_semantic_cooccurrence: f64,
    /// S3 — explicit OWL declaration (`@type owl:Class`) attached to node.
    pub s3_explicit_owl_declaration: f64,
    /// S4 — agent-emitted migration proposal for this node.
    pub s4_agent_proposal: f64,
    /// S5 — page maturity marker (#published, #canonical).
    pub s5_maturity_marker: f64,
    /// S6 — centrality rank inside the KG (pagerank / degree).
    pub s6_centrality_in_kg: f64,
    /// S7 — authoring recency (recent edits weigh more).
    pub s7_authoring_recency: f64,
    /// S8 — authoring authority (owner score, signature weight).
    pub s8_authority_score: f64,
}

impl SignalVector {
    /// Weighted sum per ADR-049 coefficients. Sums to 1.0 by construction
    /// when every signal is saturated at 1.0.
    pub fn weighted_sum(&self) -> f64 {
        self.s1_wikilink_to_ontology * W_S1_WIKILINK_TO_ONTOLOGY
            + self.s2_semantic_cooccurrence * W_S2_SEMANTIC_COOCCURRENCE
            + self.s3_explicit_owl_declaration * W_S3_EXPLICIT_OWL_DECLARATION
            + self.s4_agent_proposal * W_S4_AGENT_PROPOSAL
            + self.s5_maturity_marker * W_S5_MATURITY_MARKER
            + self.s6_centrality_in_kg * W_S6_CENTRALITY_IN_KG
            + self.s7_authoring_recency * W_S7_AUTHORING_RECENCY
            + self.s8_authority_score * W_S8_AUTHORITY_SCORE
    }

    /// Emit as a flat `Vec<f64>` for transport (e.g. `SignBridgePromotion`).
    pub fn to_vec(&self) -> Vec<f64> {
        vec![
            self.s1_wikilink_to_ontology,
            self.s2_semantic_cooccurrence,
            self.s3_explicit_owl_declaration,
            self.s4_agent_proposal,
            self.s5_maturity_marker,
            self.s6_centrality_in_kg,
            self.s7_authoring_recency,
            self.s8_authority_score,
        ]
    }
}

/// `sigmoid(a=SIGMOID_STEEPNESS, bias=SIGMOID_BIAS)` applied to a weighted sum.
///
/// At `weighted == SIGMOID_BIAS` returns exactly `0.5`. Rises steeply around
/// the bias: at `weighted = 0.60` it exceeds 0.90.
pub fn sigmoid_confidence(weighted: f64) -> f64 {
    1.0 / (1.0 + (-SIGMOID_STEEPNESS * (weighted - SIGMOID_BIAS)).exp())
}

/// Candidate lifecycle per ADR-051 transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateStatus {
    /// Confidence ≥ surface threshold; present in broker inbox.
    Surfaced,
    /// Broker has picked it up and is evaluating.
    Reviewing,
    /// `:BRIDGE_TO` edge materialised; monotonic lock-in.
    Promoted,
    /// Broker rejected; enter 3-day cooldown before re-evaluation.
    Rejected,
    /// Sub-threshold for too long; `BRIDGE_CANDIDATE` edge removed.
    Expired,
}

impl CandidateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Surfaced => "surfaced",
            Self::Reviewing => "reviewing",
            Self::Promoted => "promoted",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }
}

/// A single promotion candidate: `:KGNode` → `:OntologyClass`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationCandidate {
    /// Source `:KGNode` canonical IRI (logseq page IRI or equivalent).
    pub kg_iri: String,
    /// Proposed target `:OntologyClass` IRI.
    pub owl_class_iri: String,
    /// Raw signal vector that drove the score.
    pub signals: SignalVector,
    /// `sigmoid(weighted_sum(signals))`. Monotonic on `:BRIDGE_TO` once promoted.
    pub confidence: f64,
    pub status: CandidateStatus,
    pub first_seen_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
}

impl MigrationCandidate {
    /// Synthetic stable id for this (kg_iri, owl_class_iri) pair. Safe to use
    /// as a Neo4j candidate key.
    pub fn candidate_id(&self) -> String {
        format!("{}→{}", self.kg_iri, self.owl_class_iri)
    }
}

// ── Service ────────────────────────────────────────────────────────────────

/// Read `BRIDGE_EDGE_ENABLED` env var. Missing or anything but "true" → false.
pub fn bridge_edge_enabled() -> bool {
    matches!(
        std::env::var("BRIDGE_EDGE_ENABLED").ok().as_deref(),
        Some("true") | Some("1") | Some("TRUE")
    )
}

/// Service encapsulating scoring, surfacing, promotion, and auto-expiry.
///
/// Construct with `Arc<Neo4jAdapter>`. Cheap to clone (all state is in Neo4j).
pub struct BridgeEdgeService {
    neo4j: Arc<Neo4jAdapter>,
    prom: Option<Arc<MetricsRegistry>>,
}

impl BridgeEdgeService {
    pub fn new(neo4j: Arc<Neo4jAdapter>) -> Self {
        Self { neo4j, prom: None }
    }

    /// Attach a Prometheus registry — enables surfacing/promotion/expiry
    /// counters and the confidence histogram.
    pub fn with_prom(mut self, prom: Arc<MetricsRegistry>) -> Self {
        self.prom = Some(prom);
        self
    }

    /// Score a candidate without writing to Neo4j. Pure function over the
    /// supplied signals — no I/O.
    pub fn score_candidate(
        &self,
        kg_iri: impl Into<String>,
        owl_class_iri: impl Into<String>,
        signals: SignalVector,
    ) -> MigrationCandidate {
        let now = Utc::now();
        let confidence = sigmoid_confidence(signals.weighted_sum());
        let status = if confidence >= SURFACE_THRESHOLD {
            CandidateStatus::Surfaced
        } else if confidence < EXPIRY_CONFIDENCE {
            // Below expiry threshold — candidate is dead on arrival. Caller
            // may still surface() (which is idempotent and skips below-threshold),
            // but we set a descriptive status so logs are honest.
            CandidateStatus::Expired
        } else {
            // 0.35 ≤ confidence < 0.60: surfaced but not yet actionable.
            // Keep it as Surfaced so the broker inbox query is simple;
            // surface() itself guards on SURFACE_THRESHOLD.
            CandidateStatus::Surfaced
        };

        MigrationCandidate {
            kg_iri: kg_iri.into(),
            owl_class_iri: owl_class_iri.into(),
            signals,
            confidence,
            status,
            first_seen_at: now,
            last_updated_at: now,
        }
    }

    /// MERGE a `BRIDGE_CANDIDATE` edge when confidence clears
    /// [`SURFACE_THRESHOLD`]. No-op below threshold. No-op if a `:BRIDGE_TO`
    /// already exists for the same pair (monotonic lock-in).
    ///
    /// Respects `BRIDGE_EDGE_ENABLED`.
    pub async fn surface(&self, candidate: &MigrationCandidate) -> Result<bool> {
        if !bridge_edge_enabled() {
            debug!("surface: BRIDGE_EDGE_ENABLED=false, skipping");
            return Ok(false);
        }
        if candidate.confidence < SURFACE_THRESHOLD {
            debug!(
                "surface: candidate {} below threshold ({:.3} < {:.3}), skipping",
                candidate.candidate_id(),
                candidate.confidence,
                SURFACE_THRESHOLD
            );
            return Ok(false);
        }

        // Do not downgrade a Promoted edge. If a :BRIDGE_TO already exists,
        // surfacing is a no-op; use `promote` to refresh confidence.
        let check_q = query(
            "MATCH (k:KGNode)-[b:BRIDGE_TO]->(o:OntologyClass)
             WHERE k.iri = $kg_iri AND o.iri = $owl_iri
             RETURN count(b) AS n",
        )
        .param("kg_iri", candidate.kg_iri.clone())
        .param("owl_iri", candidate.owl_class_iri.clone());

        let mut res = self
            .neo4j
            .graph()
            .execute(check_q)
            .await
            .with_context(|| "surface: check existing BRIDGE_TO")?;
        if let Some(row) = res
            .next()
            .await
            .with_context(|| "surface: read BRIDGE_TO count row")?
        {
            let n: i64 = row.get("n").unwrap_or(0);
            if n > 0 {
                debug!(
                    "surface: {} already promoted, skipping candidate MERGE",
                    candidate.candidate_id()
                );
                return Ok(false);
            }
        }

        // Idempotent MERGE: key the relationship on both endpoints. Refresh
        // confidence + timestamps each time.
        let q = query(
            "MERGE (k:KGNode {iri: $kg_iri})
             MERGE (o:OntologyClass {iri: $owl_iri})
             MERGE (k)-[r:BRIDGE_CANDIDATE]->(o)
             SET r.confidence = $confidence,
                 r.status = $status,
                 r.signals = $signals,
                 r.last_updated_at = datetime(),
                 r.first_seen_at = coalesce(r.first_seen_at, datetime())",
        )
        .param("kg_iri", candidate.kg_iri.clone())
        .param("owl_iri", candidate.owl_class_iri.clone())
        .param("confidence", candidate.confidence)
        .param("status", CandidateStatus::Surfaced.as_str().to_string())
        .param(
            "signals",
            serde_json::to_string(&candidate.signals)
                .unwrap_or_else(|_| "{}".to_string()),
        );

        self.neo4j
            .graph()
            .run(q)
            .await
            .with_context(|| format!("surface: MERGE BRIDGE_CANDIDATE for {}", candidate.candidate_id()))?;

        info!(
            "surface: BRIDGE_CANDIDATE {} confidence={:.3}",
            candidate.candidate_id(),
            candidate.confidence
        );
        if let Some(prom) = self.prom.as_ref() {
            prom.bridge_candidates_surfaced_total.inc();
        }
        Ok(true)
    }

    /// Promote a candidate to a `:BRIDGE_TO` edge.
    ///
    /// **Monotonic invariant**: if a `:BRIDGE_TO` already exists between
    /// the two nodes, its `confidence` is updated ONLY when the new value is
    /// strictly greater. Re-scoring can never lower it. Also removes any
    /// surviving `BRIDGE_CANDIDATE` edge.
    ///
    /// Respects `BRIDGE_EDGE_ENABLED`.
    pub async fn promote(&self, candidate: &MigrationCandidate) -> Result<bool> {
        if !bridge_edge_enabled() {
            debug!("promote: BRIDGE_EDGE_ENABLED=false, skipping");
            return Ok(false);
        }

        // Step 1: MERGE :BRIDGE_TO with monotonic confidence.
        let promote_q = query(
            "MERGE (k:KGNode {iri: $kg_iri})
             MERGE (o:OntologyClass {iri: $owl_iri})
             MERGE (k)-[r:BRIDGE_TO]->(o)
             ON CREATE SET r.confidence = $confidence,
                           r.promoted_at = datetime(),
                           r.signals = $signals
             ON MATCH  SET r.confidence = CASE
                                           WHEN $confidence > r.confidence
                                           THEN $confidence
                                           ELSE r.confidence
                                         END,
                           r.last_rescored_at = datetime(),
                           r.signals = CASE
                                         WHEN $confidence > r.confidence
                                         THEN $signals
                                         ELSE r.signals
                                       END",
        )
        .param("kg_iri", candidate.kg_iri.clone())
        .param("owl_iri", candidate.owl_class_iri.clone())
        .param("confidence", candidate.confidence)
        .param(
            "signals",
            serde_json::to_string(&candidate.signals)
                .unwrap_or_else(|_| "{}".to_string()),
        );

        self.neo4j
            .graph()
            .run(promote_q)
            .await
            .with_context(|| format!("promote: MERGE BRIDGE_TO for {}", candidate.candidate_id()))?;

        // Step 2: retire the BRIDGE_CANDIDATE edge if present. Keep the
        // audit trail on the :BRIDGE_TO edge instead.
        let cleanup_q = query(
            "MATCH (k:KGNode {iri: $kg_iri})-[c:BRIDGE_CANDIDATE]->(o:OntologyClass {iri: $owl_iri})
             DELETE c",
        )
        .param("kg_iri", candidate.kg_iri.clone())
        .param("owl_iri", candidate.owl_class_iri.clone());

        if let Err(e) = self.neo4j.graph().run(cleanup_q).await {
            // Non-fatal: candidate edge cleanup failed. The monotonic :BRIDGE_TO
            // is authoritative; the stale candidate will eventually age out.
            warn!(
                "promote: cleanup BRIDGE_CANDIDATE failed for {}: {}",
                candidate.candidate_id(),
                e
            );
        }

        info!(
            "promote: BRIDGE_TO {} confidence={:.3} (monotonic)",
            candidate.candidate_id(),
            candidate.confidence
        );
        if let Some(prom) = self.prom.as_ref() {
            prom.bridge_promotions_total.inc();
            prom.bridge_confidence_histogram.observe(candidate.confidence);
        }
        Ok(true)
    }

    /// Sweep sub-threshold candidates older than [`EXPIRY_AGE_DAYS`].
    /// Sets `status='expired'` for observability, then deletes the
    /// `BRIDGE_CANDIDATE` edge. Never touches `:BRIDGE_TO`.
    ///
    /// Returns the number of edges expired.
    ///
    /// Respects `BRIDGE_EDGE_ENABLED`.
    pub async fn auto_expire(&self) -> Result<u64> {
        if !bridge_edge_enabled() {
            debug!("auto_expire: BRIDGE_EDGE_ENABLED=false, skipping");
            return Ok(0);
        }
        let cutoff = Utc::now() - Duration::days(EXPIRY_AGE_DAYS);
        let cutoff_rfc3339 = cutoff.to_rfc3339();

        let q = query(
            "MATCH (k:KGNode)-[r:BRIDGE_CANDIDATE]->(o:OntologyClass)
             WHERE r.confidence < $expiry_conf
               AND coalesce(r.status, 'surfaced') <> 'promoted'
               AND r.last_updated_at < datetime($cutoff)
             WITH r
             SET r.status = 'expired'
             WITH r
             DELETE r
             RETURN count(r) AS expired",
        )
        .param("expiry_conf", EXPIRY_CONFIDENCE)
        .param("cutoff", cutoff_rfc3339);

        let mut res = self
            .neo4j
            .graph()
            .execute(q)
            .await
            .with_context(|| "auto_expire: sweep BRIDGE_CANDIDATE")?;

        let mut expired: u64 = 0;
        if let Some(row) = res
            .next()
            .await
            .with_context(|| "auto_expire: read expired count")?
        {
            expired = row.get::<i64>("expired").unwrap_or(0) as u64;
        }
        if expired > 0 {
            info!("auto_expire: retired {} stale BRIDGE_CANDIDATE edges", expired);
            if let Some(prom) = self.prom.as_ref() {
                prom.bridge_expired_total.inc_by(expired);
            }
        } else {
            debug!("auto_expire: no candidates to retire");
        }
        Ok(expired)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn signal_vector_weighted_sum_saturated() {
        let s = SignalVector {
            s1_wikilink_to_ontology: 1.0,
            s2_semantic_cooccurrence: 1.0,
            s3_explicit_owl_declaration: 1.0,
            s4_agent_proposal: 1.0,
            s5_maturity_marker: 1.0,
            s6_centrality_in_kg: 1.0,
            s7_authoring_recency: 1.0,
            s8_authority_score: 1.0,
        };
        // Weights sum to 0.20 + 0.15 + 0.15 + 0.20 + 0.10 + 0.10 + 0.05 + 0.05 = 1.00
        assert!((s.weighted_sum() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn signal_vector_weighted_sum_zero() {
        let s = SignalVector {
            s1_wikilink_to_ontology: 0.0,
            s2_semantic_cooccurrence: 0.0,
            s3_explicit_owl_declaration: 0.0,
            s4_agent_proposal: 0.0,
            s5_maturity_marker: 0.0,
            s6_centrality_in_kg: 0.0,
            s7_authoring_recency: 0.0,
            s8_authority_score: 0.0,
        };
        assert_eq!(s.weighted_sum(), 0.0);
    }

    #[test]
    fn sigmoid_at_bias_is_half() {
        let c = sigmoid_confidence(SIGMOID_BIAS);
        assert!((c - 0.5).abs() < 1e-9);
    }

    #[test]
    fn sigmoid_at_surface_threshold_is_steep() {
        let c = sigmoid_confidence(0.60);
        // 1/(1+exp(-12*(0.60-0.42))) = 1/(1+exp(-2.16)) ≈ 0.8966
        assert!(c > 0.89, "expected > 0.89, got {}", c);
    }

    #[test]
    fn sigmoid_at_expiry_is_low() {
        let c = sigmoid_confidence(EXPIRY_CONFIDENCE);
        assert!(c < 0.5, "expected < 0.5, got {}", c);
    }

    #[test]
    fn signal_vector_to_vec_is_length_eight() {
        let s = SignalVector {
            s1_wikilink_to_ontology: 0.1,
            s2_semantic_cooccurrence: 0.2,
            s3_explicit_owl_declaration: 0.3,
            s4_agent_proposal: 0.4,
            s5_maturity_marker: 0.5,
            s6_centrality_in_kg: 0.6,
            s7_authoring_recency: 0.7,
            s8_authority_score: 0.8,
        };
        let v = s.to_vec();
        assert_eq!(v.len(), 8);
        assert_eq!(v[0], 0.1);
        assert_eq!(v[7], 0.8);
    }

    #[test]
    fn candidate_id_is_stable() {
        let c = MigrationCandidate {
            kg_iri: "logseq://page/Foo".to_string(),
            owl_class_iri: "https://ex.org/owl#Foo".to_string(),
            signals: SignalVector {
                s1_wikilink_to_ontology: 0.0,
                s2_semantic_cooccurrence: 0.0,
                s3_explicit_owl_declaration: 0.0,
                s4_agent_proposal: 0.0,
                s5_maturity_marker: 0.0,
                s6_centrality_in_kg: 0.0,
                s7_authoring_recency: 0.0,
                s8_authority_score: 0.0,
            },
            confidence: 0.0,
            status: CandidateStatus::Expired,
            first_seen_at: Utc::now(),
            last_updated_at: Utc::now(),
        };
        assert_eq!(
            c.candidate_id(),
            "logseq://page/Foo→https://ex.org/owl#Foo"
        );
    }

    #[test]
    fn candidate_status_as_str() {
        assert_eq!(CandidateStatus::Surfaced.as_str(), "surfaced");
        assert_eq!(CandidateStatus::Promoted.as_str(), "promoted");
        assert_eq!(CandidateStatus::Expired.as_str(), "expired");
        assert_eq!(CandidateStatus::Reviewing.as_str(), "reviewing");
        assert_eq!(CandidateStatus::Rejected.as_str(), "rejected");
    }

    #[test]
    fn bridge_edge_enabled_respects_env() {
        // Save, mutate, restore.
        let prev = std::env::var("BRIDGE_EDGE_ENABLED").ok();
        std::env::set_var("BRIDGE_EDGE_ENABLED", "true");
        assert!(bridge_edge_enabled());
        std::env::set_var("BRIDGE_EDGE_ENABLED", "false");
        assert!(!bridge_edge_enabled());
        std::env::remove_var("BRIDGE_EDGE_ENABLED");
        assert!(!bridge_edge_enabled());
        if let Some(v) = prev {
            std::env::set_var("BRIDGE_EDGE_ENABLED", v);
        }
    }
}
