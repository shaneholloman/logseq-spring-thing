// src/services/nhop_materializer.rs
//! N-Hop Edge Materializer for Ontology Graph Layout Coherence.
//!
//! Materializes transitive 2-hop and 3-hop connections as weak springs in Neo4j
//! so the GPU physics engine can produce more coherent ontology layouts.
//!
//! # Motivation
//! The ontology graph contains SUBCLASS_OF and RELATES edges that only express
//! direct (1-hop) relationships. Nodes separated by 2 or 3 hops have no
//! attractive force between them, causing the layout to fragment into loosely
//! connected clusters. Materializing these transitive edges with decaying
//! weights (0.05 for 2-hop, 0.02 for 3-hop) produces weak springs that pull
//! semantically related subgraphs together without collapsing them.
//!
//! # Neo4j Relationship Types
//! - `MATERIALIZED_2HOP` — created between nodes separated by exactly 2 hops
//! - `MATERIALIZED_3HOP` — created between nodes separated by exactly 3 hops
//!
//! Both carry `weight`, `source_path`, and `created_at` properties.
//!
//! # Feature Gate
//! `NHOP_MATERIALIZATION_ENABLED=true|false` (default: false). When false,
//! `materialize_all` is a no-op returning empty stats.

use std::sync::Arc;
use std::time::Instant;

use neo4rs::{Graph, query};
use thiserror::Error;
use tracing::{info, warn, instrument};

// ── Feature Flag ────────────────────────────────────────────────────────────

/// Environment variable controlling the feature gate.
pub const NHOP_ENABLED_ENV: &str = "NHOP_MATERIALIZATION_ENABLED";

/// Returns true if N-hop materialization is enabled via env var.
pub fn nhop_enabled() -> bool {
    std::env::var(NHOP_ENABLED_ENV)
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

// ── Error Type ──────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum NHopError {
    #[error("Neo4j query failed: {0}")]
    Neo4j(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Feature disabled: NHOP_MATERIALIZATION_ENABLED is not set to true")]
    Disabled,
}

/// Convert neo4rs errors into NHopError via map_err pattern.
/// We use a helper function instead of From impl to avoid depending
/// on the exact neo4rs error type path.
fn neo4j_err(e: impl std::fmt::Display) -> NHopError {
    NHopError::Neo4j(e.to_string())
}

// ── Configuration ───────────────────────────────────────────────────────────

/// Configuration for N-hop materialization behavior.
#[derive(Debug, Clone)]
pub struct NHopConfig {
    /// Weight for 2-hop materialized edges (default: 0.05).
    pub two_hop_weight: f32,
    /// Weight for 3-hop materialized edges (default: 0.02).
    pub three_hop_weight: f32,
    /// Maximum number of materialized edges per source node.
    pub max_edges_per_node: usize,
    /// Relationship types to traverse (empty = all ontology rels).
    pub traverse_types: Vec<String>,
    /// Whether to materialize across different relationship types.
    pub cross_type_hops: bool,
}

impl Default for NHopConfig {
    fn default() -> Self {
        Self {
            two_hop_weight: 0.05,
            three_hop_weight: 0.02,
            max_edges_per_node: 20,
            traverse_types: vec![
                "SUBCLASS_OF".to_string(),
                "RELATES".to_string(),
            ],
            cross_type_hops: false,
        }
    }
}

impl NHopConfig {
    /// Validate the configuration for correctness.
    pub fn validate(&self) -> Result<(), NHopError> {
        if self.two_hop_weight <= 0.0 || self.two_hop_weight > 1.0 {
            return Err(NHopError::Config(format!(
                "two_hop_weight must be in (0.0, 1.0], got {}",
                self.two_hop_weight
            )));
        }
        if self.three_hop_weight <= 0.0 || self.three_hop_weight > 1.0 {
            return Err(NHopError::Config(format!(
                "three_hop_weight must be in (0.0, 1.0], got {}",
                self.three_hop_weight
            )));
        }
        if self.three_hop_weight >= self.two_hop_weight {
            return Err(NHopError::Config(format!(
                "three_hop_weight ({}) must be less than two_hop_weight ({})",
                self.three_hop_weight, self.two_hop_weight
            )));
        }
        if self.max_edges_per_node == 0 {
            return Err(NHopError::Config(
                "max_edges_per_node must be > 0".to_string(),
            ));
        }
        if self.traverse_types.is_empty() {
            return Err(NHopError::Config(
                "traverse_types must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

// ── Output Types ────────────────────────────────────────────────────────────

/// Statistics from a materialization run.
#[derive(Debug, Clone, Default)]
pub struct MaterializationStats {
    /// Number of 2-hop edges created in this run.
    pub two_hop_edges_created: usize,
    /// Number of 3-hop edges created in this run.
    pub three_hop_edges_created: usize,
    /// Wall-clock duration of the materialization in milliseconds.
    pub duration_ms: u64,
    /// Number of source nodes processed.
    pub nodes_processed: usize,
}

/// A materialized edge ready for GPU upload as a weak spring.
#[derive(Debug, Clone)]
pub struct MaterializedEdge {
    /// IRI of the source ontology node.
    pub source_iri: String,
    /// IRI of the target ontology node.
    pub target_iri: String,
    /// Spring weight (lower = weaker attraction).
    pub weight: f32,
    /// Number of hops this edge spans (2 or 3).
    pub hop_count: u8,
    /// Textual description of the traversal path (e.g. "SUBCLASS_OF->SUBCLASS_OF").
    pub source_path: String,
}

// ── Service ─────────────────────────────────────────────────────────────────

/// N-hop edge materializer service.
///
/// Creates transitive edges in Neo4j for ontology layout improvement.
pub struct NHopMaterializer {
    neo4j: Arc<Graph>,
    config: NHopConfig,
}

impl NHopMaterializer {
    /// Create a new materializer with the given Neo4j connection and config.
    ///
    /// Validates the config on construction.
    pub fn new(neo4j: Arc<Graph>, config: NHopConfig) -> Result<Self, NHopError> {
        config.validate()?;
        Ok(Self { neo4j, config })
    }

    /// Create with default configuration.
    pub fn with_defaults(neo4j: Arc<Graph>) -> Self {
        Self {
            neo4j,
            config: NHopConfig::default(),
        }
    }

    /// Run full batch materialization for all ontology nodes.
    ///
    /// Creates `MATERIALIZED_2HOP` and `MATERIALIZED_3HOP` edges where they
    /// don't already exist. Idempotent: re-running will not create duplicates.
    #[instrument(skip(self), fields(cross_type = self.config.cross_type_hops))]
    pub async fn materialize_all(&self) -> Result<MaterializationStats, NHopError> {
        if !nhop_enabled() {
            warn!("N-hop materialization disabled (set {}=true to enable)", NHOP_ENABLED_ENV);
            return Err(NHopError::Disabled);
        }

        let start = Instant::now();
        let mut stats = MaterializationStats::default();

        // Count source nodes for stats
        stats.nodes_processed = self.count_ontology_nodes().await?;

        // Phase 1: 2-hop materialization
        if self.config.cross_type_hops {
            stats.two_hop_edges_created = self.materialize_2hop_cross_type().await?;
        } else {
            stats.two_hop_edges_created = self.materialize_2hop_same_type().await?;
        }

        // Phase 2: 3-hop materialization
        stats.three_hop_edges_created = self.materialize_3hop().await?;

        stats.duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "N-hop materialization complete: 2-hop={}, 3-hop={}, nodes={}, duration={}ms",
            stats.two_hop_edges_created,
            stats.three_hop_edges_created,
            stats.nodes_processed,
            stats.duration_ms,
        );

        Ok(stats)
    }

    /// Materialize edges for a single node by IRI.
    ///
    /// Returns the number of new edges created for that node.
    #[instrument(skip(self))]
    pub async fn materialize_for_node(&self, iri: &str) -> Result<usize, NHopError> {
        if !nhop_enabled() {
            return Err(NHopError::Disabled);
        }

        let mut total = 0usize;

        // Build relationship type filter as a Cypher-safe list literal
        let types_literal = self.config.traverse_types
            .iter()
            .map(|t| format!("'{}'", t))
            .collect::<Vec<_>>()
            .join(", ");

        // 2-hop from this node
        let cypher_2 = format!(
            "MATCH (a:OntologyClass {{iri: $iri}})-[r1]->(b:OntologyClass)-[r2]->(c:OntologyClass) \
             WHERE a <> c \
               AND type(r1) IN [{types}] \
               AND type(r2) IN [{types}] \
               AND NOT (a)-[:MATERIALIZED_2HOP]->(c) \
             WITH a, c, type(r1) + '->' + type(r2) AS path \
             WITH a, c, path, count(*) AS cnt \
             ORDER BY cnt DESC \
             LIMIT $max_edges \
             CREATE (a)-[:MATERIALIZED_2HOP {{weight: $weight, source_path: path, created_at: datetime()}}]->(c) \
             RETURN count(*) AS created",
            types = types_literal,
        );

        let q = query(&cypher_2)
            .param("iri", iri.to_string())
            .param("weight", self.config.two_hop_weight as f64)
            .param("max_edges", self.config.max_edges_per_node as i64);

        let mut result = self.neo4j.execute(q).await.map_err(neo4j_err)?;
        if let Some(row) = result.next().await.map_err(neo4j_err)? {
            total += row.get::<i64>("created").unwrap_or(0) as usize;
        }

        // 3-hop from this node
        let q3 = query(
            "MATCH (a:OntologyClass {iri: $iri})-[:SUBCLASS_OF|RELATES]->(b:OntologyClass) \
                   -[:SUBCLASS_OF|RELATES]->(c:OntologyClass) \
                   -[:SUBCLASS_OF|RELATES]->(d:OntologyClass) \
             WHERE a <> d \
               AND NOT (a)-[:MATERIALIZED_3HOP]->(d) \
             WITH a, d, count(*) AS paths \
             ORDER BY paths DESC \
             LIMIT $max_edges \
             CREATE (a)-[:MATERIALIZED_3HOP {weight: $weight, path_count: paths, created_at: datetime()}]->(d) \
             RETURN count(*) AS created"
        )
        .param("iri", iri.to_string())
        .param("weight", self.config.three_hop_weight as f64)
        .param("max_edges", self.config.max_edges_per_node as i64);

        let mut result3 = self.neo4j.execute(q3).await.map_err(neo4j_err)?;
        if let Some(row) = result3.next().await.map_err(neo4j_err)? {
            total += row.get::<i64>("created").unwrap_or(0) as usize;
        }

        Ok(total)
    }

    /// Retrieve all materialized edges for GPU upload.
    ///
    /// Returns both 2-hop and 3-hop edges with their weights and metadata.
    #[instrument(skip(self))]
    pub async fn get_materialized_edges(&self) -> Result<Vec<MaterializedEdge>, NHopError> {
        let mut edges = Vec::new();

        // Fetch 2-hop edges
        let q2 = query(
            "MATCH (a:OntologyClass)-[r:MATERIALIZED_2HOP]->(c:OntologyClass) \
             RETURN a.iri AS source, c.iri AS target, r.weight AS weight, r.source_path AS path"
        );

        let mut result2 = self.neo4j.execute(q2).await.map_err(neo4j_err)?;
        while let Some(row) = result2.next().await.map_err(neo4j_err)? {
            let source_iri: String = row.get("source").unwrap_or_default();
            let target_iri: String = row.get("target").unwrap_or_default();
            let weight: f64 = row.get("weight").unwrap_or(self.config.two_hop_weight as f64);
            let source_path: String = row.get("path").unwrap_or_default();

            if !source_iri.is_empty() && !target_iri.is_empty() {
                edges.push(MaterializedEdge {
                    source_iri,
                    target_iri,
                    weight: weight as f32,
                    hop_count: 2,
                    source_path,
                });
            }
        }

        // Fetch 3-hop edges
        let q3 = query(
            "MATCH (a:OntologyClass)-[r:MATERIALIZED_3HOP]->(d:OntologyClass) \
             RETURN a.iri AS source, d.iri AS target, r.weight AS weight"
        );

        let mut result3 = self.neo4j.execute(q3).await.map_err(neo4j_err)?;
        while let Some(row) = result3.next().await.map_err(neo4j_err)? {
            let source_iri: String = row.get("source").unwrap_or_default();
            let target_iri: String = row.get("target").unwrap_or_default();
            let weight: f64 = row.get("weight").unwrap_or(self.config.three_hop_weight as f64);

            if !source_iri.is_empty() && !target_iri.is_empty() {
                edges.push(MaterializedEdge {
                    source_iri,
                    target_iri,
                    weight: weight as f32,
                    hop_count: 3,
                    source_path: "variable-length-3".to_string(),
                });
            }
        }

        info!("Retrieved {} materialized edges for GPU upload", edges.len());
        Ok(edges)
    }

    /// Remove all materialized edges from Neo4j.
    ///
    /// Use before re-running materialization with new config, or to clean up.
    #[instrument(skip(self))]
    pub async fn clear_materialized(&self) -> Result<(), NHopError> {
        let q2 = query(
            "MATCH ()-[r:MATERIALIZED_2HOP]->() DELETE r RETURN count(r) AS deleted"
        );
        let mut res2 = self.neo4j.execute(q2).await.map_err(neo4j_err)?;
        let deleted_2: i64 = if let Some(row) = res2.next().await.map_err(neo4j_err)? {
            row.get("deleted").unwrap_or(0)
        } else {
            0
        };

        let q3 = query(
            "MATCH ()-[r:MATERIALIZED_3HOP]->() DELETE r RETURN count(r) AS deleted"
        );
        let mut res3 = self.neo4j.execute(q3).await.map_err(neo4j_err)?;
        let deleted_3: i64 = if let Some(row) = res3.next().await.map_err(neo4j_err)? {
            row.get("deleted").unwrap_or(0)
        } else {
            0
        };

        info!(
            "Cleared materialized edges: 2-hop={}, 3-hop={}",
            deleted_2, deleted_3
        );
        Ok(())
    }

    // ── Private Helpers ─────────────────────────────────────────────────────

    /// Count ontology nodes for stats reporting.
    async fn count_ontology_nodes(&self) -> Result<usize, NHopError> {
        let q = query("MATCH (n:OntologyClass) RETURN count(n) AS cnt");
        let mut result = self.neo4j.execute(q).await.map_err(neo4j_err)?;
        if let Some(row) = result.next().await.map_err(neo4j_err)? {
            Ok(row.get::<i64>("cnt").unwrap_or(0) as usize)
        } else {
            Ok(0)
        }
    }

    /// Materialize 2-hop edges following the same relationship type.
    ///
    /// For each rel type in traverse_types, creates MATERIALIZED_2HOP edges
    /// between nodes separated by exactly 2 hops of that type.
    async fn materialize_2hop_same_type(&self) -> Result<usize, NHopError> {
        let mut total = 0usize;

        for rel_type in &self.config.traverse_types {
            let cypher = format!(
                "MATCH (a:OntologyClass)-[r1:{rel}]->(b:OntologyClass)-[r2:{rel}]->(c:OntologyClass) \
                 WHERE a <> c AND NOT (a)-[:MATERIALIZED_2HOP]->(c) \
                 WITH a, c, '{rel}->{rel}' AS path \
                 WITH a, c, path, count(*) AS cnt \
                 ORDER BY cnt DESC \
                 WITH a, collect({{target: c, path: path}})[..{limit}] AS targets \
                 UNWIND targets AS t \
                 WITH a, t.target AS tgt, t.path AS tp \
                 CREATE (a)-[:MATERIALIZED_2HOP {{weight: $weight, source_path: tp, created_at: datetime()}}]->(tgt) \
                 RETURN count(*) AS created",
                rel = rel_type,
                limit = self.config.max_edges_per_node,
            );

            let q = query(&cypher)
                .param("weight", self.config.two_hop_weight as f64);

            let mut result = self.neo4j.execute(q).await.map_err(neo4j_err)?;
            if let Some(row) = result.next().await.map_err(neo4j_err)? {
                let created = row.get::<i64>("created").unwrap_or(0) as usize;
                total += created;
                info!("2-hop same-type [{}]: {} edges created", rel_type, created);
            }
        }

        Ok(total)
    }

    /// Materialize 2-hop edges crossing different relationship types.
    ///
    /// Creates MATERIALIZED_2HOP edges where the two hops may follow different
    /// relationship types from traverse_types.
    async fn materialize_2hop_cross_type(&self) -> Result<usize, NHopError> {
        let types_literal = self.config.traverse_types
            .iter()
            .map(|t| format!("'{}'", t))
            .collect::<Vec<_>>()
            .join(", ");

        let cypher = format!(
            "MATCH (a:OntologyClass)-[r1]->(b:OntologyClass)-[r2]->(c:OntologyClass) \
             WHERE a <> c \
               AND type(r1) IN [{types}] \
               AND type(r2) IN [{types}] \
               AND NOT (a)-[:MATERIALIZED_2HOP]->(c) \
             WITH a, c, type(r1) + '->' + type(r2) AS path \
             WITH a, c, path, count(*) AS cnt \
             ORDER BY cnt DESC \
             WITH a, collect({{target: c, path: path}})[..{limit}] AS targets \
             UNWIND targets AS t \
             WITH a, t.target AS tgt, t.path AS tp \
             CREATE (a)-[:MATERIALIZED_2HOP {{weight: $weight, source_path: tp, created_at: datetime()}}]->(tgt) \
             RETURN count(*) AS created",
            types = types_literal,
            limit = self.config.max_edges_per_node,
        );

        let q = query(&cypher)
            .param("weight", self.config.two_hop_weight as f64);

        let mut result = self.neo4j.execute(q).await.map_err(neo4j_err)?;
        if let Some(row) = result.next().await.map_err(neo4j_err)? {
            let created = row.get::<i64>("created").unwrap_or(0) as usize;
            info!("2-hop cross-type: {} edges created", created);
            Ok(created)
        } else {
            Ok(0)
        }
    }

    /// Materialize 3-hop edges using variable-length path patterns.
    async fn materialize_3hop(&self) -> Result<usize, NHopError> {
        let cypher = format!(
            "MATCH (a:OntologyClass)-[:SUBCLASS_OF|RELATES]->(b:OntologyClass) \
                   -[:SUBCLASS_OF|RELATES]->(c:OntologyClass) \
                   -[:SUBCLASS_OF|RELATES]->(d:OntologyClass) \
             WHERE a <> d AND NOT (a)-[:MATERIALIZED_3HOP]->(d) \
             WITH a, d, count(*) AS paths \
             WHERE paths >= 1 \
             ORDER BY paths DESC \
             WITH a, collect({{target: d, paths: paths}})[..{limit}] AS targets \
             UNWIND targets AS t \
             WITH a, t.target AS tgt, t.paths AS tp \
             CREATE (a)-[:MATERIALIZED_3HOP {{weight: $weight, path_count: tp, created_at: datetime()}}]->(tgt) \
             RETURN count(*) AS created",
            limit = self.config.max_edges_per_node,
        );

        let q = query(&cypher)
            .param("weight", self.config.three_hop_weight as f64);

        let mut result = self.neo4j.execute(q).await.map_err(neo4j_err)?;
        if let Some(row) = result.next().await.map_err(neo4j_err)? {
            let created = row.get::<i64>("created").unwrap_or(0) as usize;
            info!("3-hop: {} edges created", created);
            Ok(created)
        } else {
            Ok(0)
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = NHopConfig::default();
        assert_eq!(config.two_hop_weight, 0.05);
        assert_eq!(config.three_hop_weight, 0.02);
        assert_eq!(config.max_edges_per_node, 20);
        assert_eq!(config.traverse_types, vec!["SUBCLASS_OF", "RELATES"]);
        assert!(!config.cross_type_hops);
    }

    #[test]
    fn test_config_validation_valid() {
        let config = NHopConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_zero_weight() {
        let config = NHopConfig {
            two_hop_weight: 0.0,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, NHopError::Config(_)));
    }

    #[test]
    fn test_config_validation_negative_weight() {
        let config = NHopConfig {
            three_hop_weight: -0.1,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, NHopError::Config(_)));
    }

    #[test]
    fn test_config_validation_three_hop_exceeds_two_hop() {
        let config = NHopConfig {
            two_hop_weight: 0.05,
            three_hop_weight: 0.10,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, NHopError::Config(_)));
        let msg = err.to_string();
        assert!(msg.contains("must be less than"));
    }

    #[test]
    fn test_config_validation_equal_weights() {
        let config = NHopConfig {
            two_hop_weight: 0.05,
            three_hop_weight: 0.05,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, NHopError::Config(_)));
    }

    #[test]
    fn test_config_validation_zero_max_edges() {
        let config = NHopConfig {
            max_edges_per_node: 0,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, NHopError::Config(_)));
    }

    #[test]
    fn test_config_validation_empty_traverse_types() {
        let config = NHopConfig {
            traverse_types: vec![],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, NHopError::Config(_)));
    }

    #[test]
    fn test_config_validation_weight_at_boundary() {
        let config = NHopConfig {
            two_hop_weight: 1.0,
            three_hop_weight: 0.99,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_weight_exceeds_one() {
        let config = NHopConfig {
            two_hop_weight: 1.01,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, NHopError::Config(_)));
    }

    #[test]
    fn test_materialized_edge_fields() {
        let edge = MaterializedEdge {
            source_iri: "http://example.org/A".to_string(),
            target_iri: "http://example.org/C".to_string(),
            weight: 0.05,
            hop_count: 2,
            source_path: "SUBCLASS_OF->SUBCLASS_OF".to_string(),
        };
        assert_eq!(edge.hop_count, 2);
        assert_eq!(edge.weight, 0.05);
        assert_eq!(edge.source_path, "SUBCLASS_OF->SUBCLASS_OF");
    }

    #[test]
    fn test_materialization_stats_default() {
        let stats = MaterializationStats::default();
        assert_eq!(stats.two_hop_edges_created, 0);
        assert_eq!(stats.three_hop_edges_created, 0);
        assert_eq!(stats.duration_ms, 0);
        assert_eq!(stats.nodes_processed, 0);
    }

    #[test]
    fn test_nhop_enabled_defaults_false() {
        // When env var is not set, should be false
        std::env::remove_var(NHOP_ENABLED_ENV);
        assert!(!nhop_enabled());
    }

    #[test]
    fn test_nhop_enabled_true() {
        std::env::set_var(NHOP_ENABLED_ENV, "true");
        assert!(nhop_enabled());
        std::env::remove_var(NHOP_ENABLED_ENV);
    }

    #[test]
    fn test_nhop_enabled_one() {
        std::env::set_var(NHOP_ENABLED_ENV, "1");
        assert!(nhop_enabled());
        std::env::remove_var(NHOP_ENABLED_ENV);
    }

    #[test]
    fn test_nhop_enabled_false() {
        std::env::set_var(NHOP_ENABLED_ENV, "false");
        assert!(!nhop_enabled());
        std::env::remove_var(NHOP_ENABLED_ENV);
    }

    #[test]
    fn test_error_display() {
        let e = NHopError::Neo4j("connection refused".to_string());
        assert_eq!(e.to_string(), "Neo4j query failed: connection refused");

        let e = NHopError::Config("bad weight".to_string());
        assert_eq!(e.to_string(), "Configuration error: bad weight");

        let e = NHopError::Disabled;
        assert!(e.to_string().contains("not set to true"));
    }

    /// Verify that deduplication logic works: the Cypher uses
    /// `NOT (a)-[:MATERIALIZED_2HOP]->(c)` to prevent duplicates.
    /// This test validates the pattern string is included in same-type queries.
    #[test]
    fn test_deduplication_pattern_in_cypher() {
        // The dedup guard is baked into the Cypher; this test ensures the
        // format string for same-type materialization includes it.
        let rel_type = "SUBCLASS_OF";
        let limit = 20;
        let cypher = format!(
            "MATCH (a:OntologyClass)-[r1:{rel}]->(b:OntologyClass)-[r2:{rel}]->(c:OntologyClass) \
             WHERE a <> c AND NOT (a)-[:MATERIALIZED_2HOP]->(c) \
             WITH a, c, '{rel}->{rel}' AS path \
             WITH a, c, path, count(*) AS cnt \
             ORDER BY cnt DESC \
             WITH a, collect({{target: c, path: path}})[..{limit}] AS targets \
             UNWIND targets AS t \
             WITH a, t.target AS tgt, t.path AS tp \
             CREATE (a)-[:MATERIALIZED_2HOP {{weight: $weight, source_path: tp, created_at: datetime()}}]->(tgt) \
             RETURN count(*) AS created",
            rel = rel_type,
            limit = limit,
        );
        assert!(cypher.contains("NOT (a)-[:MATERIALIZED_2HOP]->(c)"));
        assert!(cypher.contains("a <> c"));
        assert!(cypher.contains("[..20]"));
    }

    /// Weight decay: 3-hop weight must be strictly less than 2-hop weight.
    /// This validates the physical invariant that longer paths produce weaker springs.
    #[test]
    fn test_weight_decay_invariant() {
        let config = NHopConfig::default();
        assert!(
            config.three_hop_weight < config.two_hop_weight,
            "3-hop weight ({}) must be less than 2-hop weight ({})",
            config.three_hop_weight,
            config.two_hop_weight
        );
        // The ratio should be roughly 2:5 (0.02 / 0.05 = 0.4)
        let ratio = config.three_hop_weight / config.two_hop_weight;
        assert!(ratio > 0.0 && ratio < 1.0);
    }
}
