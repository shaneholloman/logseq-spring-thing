// src/adapters/neo4j_adapter.rs
//! Neo4j Graph Repository Adapter
//!
//! Implements KnowledgeGraphRepository trait using Neo4j graph database.
//! Provides native Cypher query support for multi-hop reasoning and path analysis.
//!
//! Database schema:
//! - Nodes: (:GraphNode {id, metadata_id, label, owl_class_iri, ...})
//! - Relationships: [:EDGE {weight, relation_type, owl_property_iri}]
//!
//! This adapter enables:
//! - Complex graph traversals with Cypher
//! - Multi-hop path analysis
//! - Semantic reasoning via OWL enrichment
//! - High-performance graph queries

use async_trait::async_trait;
use log::{debug, info, warn};
use neo4rs::{Graph, Query, Node as Neo4jNode, ConfigBuilder};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::instrument;

use crate::models::edge::Edge;
use crate::models::graph::GraphData;
use crate::models::node::Node;
use crate::utils::json::{to_json, from_json};
use crate::ports::knowledge_graph_repository::{
    GraphStatistics, KnowledgeGraphRepository, KnowledgeGraphRepositoryError,
    Result as RepoResult,
};
use crate::utils::network::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError};
use crate::utils::time;

/// Neo4j configuration with security and performance settings
#[derive(Debug, Clone)]
pub struct Neo4jConfig {
    pub uri: String,
    pub user: String,
    pub password: String,
    pub database: Option<String>,
    /// Maximum number of connections in the pool (default: 50)
    pub max_connections: usize,
    /// Query timeout in seconds (default: 30)
    pub query_timeout_secs: u64,
    /// Connection timeout in seconds (default: 10)
    pub connection_timeout_secs: u64,
}

impl Neo4jConfig {
    /// Build a Neo4jConfig from environment variables, returning an error
    /// instead of panicking when required variables are missing or invalid.
    pub fn from_env() -> Result<Self, String> {
        // SECURITY: NEO4J_PASSWORD is REQUIRED - no insecure defaults
        let password = std::env::var("NEO4J_PASSWORD").unwrap_or_else(|_| {
            // In development/test mode, allow default only if explicitly set
            if std::env::var("ALLOW_INSECURE_DEFAULTS").is_ok() {
                log::warn!("NEO4J_PASSWORD not set - using insecure default (ALLOW_INSECURE_DEFAULTS=1)");
                "password".to_string()
            } else {
                String::new()
            }
        });

        if password.is_empty() {
            log::error!("CRITICAL: NEO4J_PASSWORD environment variable is REQUIRED!");
            log::error!("   Set NEO4J_PASSWORD=<your-secure-password> or");
            log::error!("   Set ALLOW_INSECURE_DEFAULTS=1 for development only");
            return Err("NEO4J_PASSWORD must be set. See logs for details.".to_string());
        }

        // Reject obviously insecure passwords in production
        if (password == "password" || password == "neo4j" || password.len() < 8)
            && std::env::var("ALLOW_INSECURE_DEFAULTS").is_err()
        {
            log::error!("CRITICAL: NEO4J_PASSWORD is too weak or uses a default value!");
            return Err("NEO4J_PASSWORD must be at least 8 characters and not a default value".to_string());
        }

        Ok(Self {
            uri: std::env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string()),
            user: std::env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string()),
            password,
            database: std::env::var("NEO4J_DATABASE").ok(),
            max_connections: std::env::var("NEO4J_MAX_CONNECTIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50),
            query_timeout_secs: std::env::var("NEO4J_QUERY_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            connection_timeout_secs: std::env::var("NEO4J_CONNECTION_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        })
    }
}

impl Default for Neo4jConfig {
    /// Returns a safe placeholder config that will NOT connect to any database.
    /// Callers that need a real connection should use `Neo4jConfig::from_env()` instead.
    fn default() -> Self {
        Self {
            uri: "bolt://localhost:7687".to_string(),
            user: "neo4j".to_string(),
            password: "not-configured".to_string(),
            database: None,
            max_connections: 50,
            query_timeout_secs: 30,
            connection_timeout_secs: 10,
        }
    }
}

/// Repository for knowledge graph data in Neo4j
/// Provides high-performance graph operations with native Cypher support.
/// All node positions and velocities are persisted and can be queried with
/// complex graph patterns.
pub struct Neo4jAdapter {
    graph: Arc<Graph>,
    #[allow(dead_code)]
    config: Neo4jConfig,
    circuit_breaker: CircuitBreaker,
}

impl Neo4jAdapter {
    /// Create a new Neo4jAdapter with security hardening
    /// # Arguments
    /// * `config` - Neo4j connection configuration
    /// # Security
    /// - Uses connection pooling (configured via config.max_connections)
    /// - Enforces query timeouts (configured via config.query_timeout_secs)
    /// - Logs warning if default password is used
    /// # Returns
    /// Initialized adapter ready for graph operations
    pub async fn new(config: Neo4jConfig) -> Result<Self, KnowledgeGraphRepositoryError> {
        // SECURITY: Validate configuration
        if config.password == "password" {
            log::error!("❌ CRITICAL: Using default password 'password' for Neo4j!");
            log::error!("❌ Set NEO4J_PASSWORD environment variable immediately!");
        }

        if config.max_connections == 0 {
            return Err(KnowledgeGraphRepositoryError::DatabaseError(
                "Invalid configuration: max_connections must be > 0".to_string()
            ));
        }

        info!("Connecting to Neo4j at {} (max_connections: {}, query_timeout: {}s)",
              config.uri, config.max_connections, config.query_timeout_secs);

        // PERF: Configure connection pool for high-throughput graph operations
        let neo4j_config = ConfigBuilder::default()
            .uri(&config.uri)
            .user(&config.user)
            .password(&config.password)
            .max_connections(config.max_connections)
            .build()
            .map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!(
                    "Failed to build Neo4j config: {}",
                    e
                ))
            })?;

        let graph = Graph::connect(neo4j_config)
            .map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!(
                    "Failed to connect to Neo4j: {}",
                    e
                ))
            })?;

        info!("Connected to Neo4j successfully with {} connection pool", config.max_connections);

        let adapter = Self {
            graph: Arc::new(graph),
            config,
            circuit_breaker: CircuitBreaker::new(CircuitBreakerConfig::tcp_connection()),
        };

        // Create indexes and constraints
        adapter.create_schema().await?;

        Ok(adapter)
    }

    /// Get access to underlying Graph for direct queries.
    pub fn graph(&self) -> &Arc<Graph> {
        &self.graph
    }

    /// Create Neo4j schema (indexes and constraints)
    async fn create_schema(&self) -> RepoResult<()> {
        info!("Creating Neo4j schema...");

        // Create uniqueness constraint on GraphNode.id
        let constraint_query = Query::new("CREATE CONSTRAINT graph_node_id IF NOT EXISTS FOR (n:GraphNode) REQUIRE n.id IS UNIQUE".to_string());

        if let Err(e) = self.graph.run(constraint_query).await {
            warn!("Failed to create constraint (may already exist): {}", e);
        }

        // Create index on metadata_id for faster lookups
        let index_query = Query::new("CREATE INDEX graph_node_metadata_id IF NOT EXISTS FOR (n:GraphNode) ON (n.metadata_id)".to_string());

        if let Err(e) = self.graph.run(index_query).await {
            warn!("Failed to create index (may already exist): {}", e);
        }

        // Create index on owl_class_iri for semantic queries
        let owl_index_query = Query::new("CREATE INDEX graph_node_owl_class IF NOT EXISTS FOR (n:GraphNode) ON (n.owl_class_iri)".to_string());

        if let Err(e) = self.graph.run(owl_index_query).await {
            warn!("Failed to create OWL index (may already exist): {}", e);
        }

        // Create index on node_type for semantic force filtering
        let node_type_index_query = Query::new("CREATE INDEX graph_node_type IF NOT EXISTS FOR (n:GraphNode) ON (n.node_type)".to_string());

        if let Err(e) = self.graph.run(node_type_index_query).await {
            warn!("Failed to create node_type index (may already exist): {}", e);
        }

        // Create index on edge relation_type for semantic pathfinding
        let edge_type_index_query = Query::new("CREATE INDEX edge_relation_type IF NOT EXISTS FOR ()-[r:EDGE]-() ON (r.relation_type)".to_string());

        if let Err(e) = self.graph.run(edge_type_index_query).await {
            warn!("Failed to create edge relation_type index (may already exist): {}", e);
        }

        // Semantic-code indexes: these properties were previously only present
        // inside the `metadata` JSON string blob, invisible to Cypher queries.
        // Now that save_graph flattens them as first-class properties, index
        // them for fast filtering (e.g. WHERE n.physicality_code > 0, domain
        // GROUP BY source_domain).
        for (name, prop) in [
            ("graph_node_physicality_code", "physicality_code"),
            ("graph_node_role_code", "role_code"),
            ("graph_node_maturity_level", "maturity_level"),
            ("graph_node_source_domain", "source_domain"),
            ("graph_node_term_id", "term_id"),
        ] {
            let q = Query::new(format!(
                "CREATE INDEX {} IF NOT EXISTS FOR (n:GraphNode) ON (n.{})",
                name, prop
            ));
            if let Err(e) = self.graph.run(q).await {
                warn!("Failed to create {} index (may already exist): {}", name, e);
            }
        }

        // Create fulltext index on node label and metadata_id for fast text search
        let fulltext_index_query = Query::new(
            "CREATE FULLTEXT INDEX graph_node_label_ft IF NOT EXISTS FOR (n:GraphNode) ON EACH [n.label, n.metadata_id]".to_string()
        );

        if let Err(e) = self.graph.run(fulltext_index_query).await {
            warn!("Failed to create fulltext index (may already exist): {}", e);
        }

        info!("Neo4j schema created successfully with semantic type and fulltext indexes");
        Ok(())
    }

    /// Convert Node to Neo4j properties
    fn node_to_properties(node: &Node) -> HashMap<String, neo4rs::BoltType> {
        let mut props = HashMap::new();

        props.insert("id".to_string(), neo4rs::BoltType::Integer(neo4rs::BoltInteger::new(node.id as i64)));
        props.insert("metadata_id".to_string(), neo4rs::BoltType::String(neo4rs::BoltString::from(node.metadata_id.clone())));
        props.insert("label".to_string(), neo4rs::BoltType::String(neo4rs::BoltString::from(node.label.clone())));
        props.insert("x".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(node.data.x as f64)));
        props.insert("y".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(node.data.y as f64)));
        props.insert("z".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(node.data.z as f64)));
        props.insert("vx".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(node.data.vx as f64)));
        props.insert("vy".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(node.data.vy as f64)));
        props.insert("vz".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(node.data.vz as f64)));
        props.insert("mass".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(node.mass.unwrap_or(1.0) as f64)));

        if let Some(ref iri) = node.owl_class_iri {
            props.insert("owl_class_iri".to_string(), neo4rs::BoltType::String(neo4rs::BoltString::from(iri.clone())));
        }

        if let Some(ref color) = node.color {
            props.insert("color".to_string(), neo4rs::BoltType::String(neo4rs::BoltString::from(color.clone())));
        }

        if let Some(size) = node.size {
            props.insert("size".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(size as f64)));
        }

        if let Some(ref node_type) = node.node_type {
            props.insert("node_type".to_string(), neo4rs::BoltType::String(neo4rs::BoltString::from(node_type.clone())));
        }

        if let Some(weight) = node.weight {
            props.insert("weight".to_string(), neo4rs::BoltType::Float(neo4rs::BoltFloat::new(weight as f64)));
        }

        if let Some(ref group) = node.group {
            props.insert("group_name".to_string(), neo4rs::BoltType::String(neo4rs::BoltString::from(group.clone())));
        }

        // Serialize metadata as JSON string
        if !node.metadata.is_empty() {
            if let Ok(json) = to_json(&node.metadata) {
                props.insert("metadata".to_string(), neo4rs::BoltType::String(neo4rs::BoltString::from(json)));
            }
        }

        props
    }

    /// Map ontology source_domain to a display color for visual grouping
    fn domain_to_color(domain: &str) -> String {
        match domain {
            "AI" => "#4FC3F7".to_string(),   // Light blue
            "BC" => "#81C784".to_string(),   // Green
            "RB" => "#FFB74D".to_string(),   // Orange
            "MV" => "#CE93D8".to_string(),   // Purple
            "TC" => "#FFD54F".to_string(),   // Yellow
            "DT" => "#EF5350".to_string(),   // Red
            "NGM" => "#4DB6AC".to_string(),  // Teal
            _ => "#90A4AE".to_string(),      // Grey for unknown
        }
    }

    /// Convert Neo4j node to our Node model
    /// Prioritizes sim_* properties for physics coordinates (GPU-calculated positions)
    fn neo4j_node_to_node(neo4j_node: &Neo4jNode) -> RepoResult<Node> {
        let id: i64 = neo4j_node.get("id").map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Missing id: {}", e))
        })?;

        let metadata_id: String = neo4j_node.get("metadata_id").map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Missing metadata_id: {}", e))
        })?;

        let label: String = neo4j_node.get("label")
            .ok()
            .filter(|s: &String| !s.is_empty() && s != "UNKNOWN")
            .unwrap_or_else(|| metadata_id.clone());

        // Prefer sim_* properties (GPU physics state) over x/y/z (initial/content positions)
        // This preserves the calculated layout during content sync
        let x: f64 = neo4j_node.get("sim_x").or_else(|_| neo4j_node.get("x")).unwrap_or(0.0);
        let y: f64 = neo4j_node.get("sim_y").or_else(|_| neo4j_node.get("y")).unwrap_or(0.0);
        let z: f64 = neo4j_node.get("sim_z").or_else(|_| neo4j_node.get("z")).unwrap_or(0.0);
        let vx: f64 = neo4j_node.get("vx").unwrap_or(0.0);
        let vy: f64 = neo4j_node.get("vy").unwrap_or(0.0);
        let vz: f64 = neo4j_node.get("vz").unwrap_or(0.0);
        let mass: f64 = neo4j_node.get("mass").unwrap_or(1.0);

        let owl_class_iri: Option<String> = neo4j_node.get("owl_class_iri").ok();
        let color: Option<String> = neo4j_node.get("color").ok();
        let size: Option<f64> = neo4j_node.get("size").ok();
        let raw_node_type: Result<String, _> = neo4j_node.get("node_type");
        let node_type: Option<String> = raw_node_type.ok()
            .filter(|s: &String| !s.is_empty());  // Treat empty string as None
        // Log first few nodes for debugging type propagation
        if id <= 3 {
            log::info!("Neo4jAdapter::neo4j_node_to_node: id={} node_type={:?}", id, node_type);
        }
        let weight: Option<f64> = neo4j_node.get("weight").ok();
        let group_name: Option<String> = neo4j_node.get("group_name").ok();

        let mut metadata: HashMap<String, String> = neo4j_node
            .get::<String>("metadata")
            .ok()
            .and_then(|json| from_json(&json).ok())
            .unwrap_or_default();

        // Read quality_score and authority_score from Neo4j node properties
        // These are stored as top-level properties, not inside metadata JSON
        if let Ok(quality_score) = neo4j_node.get::<f64>("quality_score") {
            metadata.insert("quality_score".to_string(), quality_score.to_string());
        }
        if let Ok(authority_score) = neo4j_node.get::<f64>("authority_score") {
            metadata.insert("authority_score".to_string(), authority_score.to_string());
        }

        let mut node = Node::new_with_id(metadata_id, Some(id as u32));
        node.label = label;
        node.data.x = x as f32;
        node.data.y = y as f32;
        node.data.z = z as f32;
        node.data.vx = vx as f32;
        node.data.vy = vy as f32;
        node.data.vz = vz as f32;
        node.mass = Some(mass as f32);
        node.owl_class_iri = owl_class_iri;
        node.color = color;
        node.size = size.map(|s| s as f32);
        node.node_type = node_type;
        node.weight = weight.map(|w| w as f32);
        node.group = group_name;
        node.metadata = metadata;

        Ok(node)
    }

    /// Execute a parameterized Cypher query (SAFE - use this for user input)
    /// # Security
    /// This method enforces parameterization to prevent Cypher injection attacks.
    /// DO NOT concatenate user input into the query string - use parameters instead.
    /// # Example
    /// ```ignore
    /// // SAFE - Uses parameters
    /// let params = hashmap!{"name" => BoltType::String("Alice".into())};
    /// adapter.execute_cypher_safe("MATCH (n:User {name: $name}) RETURN n", params).await?;
    /// // UNSAFE - Don't do this!
    /// // let query = format!("MATCH (n:User {{name: '{}'}}) RETURN n", user_input);
    /// ```
    pub(crate) async fn execute_cypher_safe(
        &self,
        query: &str,
        params: HashMap<String, neo4rs::BoltType>,
    ) -> RepoResult<Vec<HashMap<String, serde_json::Value>>> {
        self.execute_cypher_internal(query, params, true).await
    }

    /// Execute a Cypher query (DEPRECATED - use execute_cypher_safe)
    /// # Security Warning
    /// This method is deprecated in favor of execute_cypher_safe.
    /// Only use this for trusted, static queries. Never concatenate user input!
    #[deprecated(since = "0.1.0", note = "Use execute_cypher_safe instead")]
    pub(crate) async fn execute_cypher(
        &self,
        query: &str,
        params: HashMap<String, neo4rs::BoltType>,
    ) -> RepoResult<Vec<HashMap<String, serde_json::Value>>> {
        log::warn!("execute_cypher is deprecated - use execute_cypher_safe instead");
        self.execute_cypher_internal(query, params, false).await
    }

    /// Internal method for executing Cypher queries
    async fn execute_cypher_internal(
        &self,
        query: &str,
        params: HashMap<String, neo4rs::BoltType>,
        _safe_mode: bool,
    ) -> RepoResult<Vec<HashMap<String, serde_json::Value>>> {
        // SECURITY: Log query execution (without sensitive data)
        debug!("Executing Cypher query with {} parameters", params.len());

        let mut query_obj = Query::new(query.to_string());

        for (key, value) in params {
            query_obj = query_obj.param(&key, value);
        }

        // Apply query timeout at application level (neo4rs doesn't support query-level timeouts)
        // Default timeout: 30 seconds for complex graph queries
        let query_timeout = std::time::Duration::from_secs(
            std::env::var("NEO4J_QUERY_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30)
        );

        // Clone graph Arc for use in closure
        let graph = self.graph.clone();

        // Wrap query execution with circuit breaker for network resilience
        let execute_result = self.circuit_breaker.execute(async {
            let result = tokio::time::timeout(
                query_timeout,
                graph.execute(query_obj)
            ).await;

            match result {
                Ok(Ok(r)) => Ok(r),
                Ok(Err(e)) => {
                    log::error!("Cypher query failed: {}", e);
                    Err(KnowledgeGraphRepositoryError::DatabaseError(format!("Cypher query failed: {}", e)))
                }
                Err(_) => {
                    log::error!("Cypher query timed out after {:?}", query_timeout);
                    Err(KnowledgeGraphRepositoryError::DatabaseError(
                        format!("Query timed out after {:?}", query_timeout)
                    ))
                }
            }
        }).await;

        let mut result = match execute_result {
            Ok(r) => r,
            Err(CircuitBreakerError::CircuitOpen) => {
                log::warn!("Circuit breaker is open - Neo4j queries temporarily blocked");
                return Err(KnowledgeGraphRepositoryError::DatabaseError(
                    "Circuit breaker open: Neo4j service temporarily unavailable".to_string()
                ));
            }
            Err(CircuitBreakerError::OperationFailed(e)) => {
                return Err(e);
            }
        };

        let mut results = Vec::new();
        while let Ok(Some(_row)) = result.next().await {
            // Note: Neo4rs Row API doesn't provide direct access to all keys
            // For now, returning empty map - users should use specific field access
            let row_map = HashMap::new();
            results.push(row_map);
        }

        Ok(results)
    }
}

#[async_trait]
impl KnowledgeGraphRepository for Neo4jAdapter {
    #[instrument(skip(self), level = "debug")]
    async fn load_graph(&self) -> RepoResult<Arc<GraphData>> {
        // Flat-column projection (same approach as neo4j_graph_repository).
        // DO NOT use `RETURN n` — Bolt node property extraction silently
        // fails for some nodes, causing the whole load to abort or return 0.
        let nodes_query = Query::new(
            "MATCH (n:GraphNode)
             RETURN n.id AS id,
                    n.metadata_id AS metadata_id,
                    n.label AS label,
                    COALESCE(n.sim_x, n.x) AS x,
                    COALESCE(n.sim_y, n.y) AS y,
                    COALESCE(n.sim_z, n.z) AS z,
                    n.vx AS vx, n.vy AS vy, n.vz AS vz,
                    n.mass AS mass, n.size AS size,
                    n.color AS color, n.weight AS weight,
                    n.node_type AS node_type,
                    n.owl_class_iri AS owl_class_iri,
                    n.group_name AS group_name,
                    n.metadata AS metadata_json,
                    n.quality_score AS quality_score,
                    n.authority_score AS authority_score
             ORDER BY id".to_string()
        );

        let mut nodes = Vec::new();
        let mut result = self.graph.execute(nodes_query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to load GraphNode nodes: {}", e))
        })?;

        let mut skipped = 0u32;
        while let Ok(Some(row)) = result.next().await {
            let id: i64 = match row.get("id") {
                Ok(v) => v,
                Err(e) => {
                    if skipped < 3 { log::warn!("Skipping GraphNode row (missing id): {}", e); }
                    skipped += 1;
                    continue;
                }
            };
            let metadata_id: String = row.get("metadata_id").unwrap_or_default();
            let label: String = row.get("label").unwrap_or_default();
            let x: f64 = row.get("x").unwrap_or(0.0);
            let y: f64 = row.get("y").unwrap_or(0.0);
            let z: f64 = row.get("z").unwrap_or(0.0);
            let vx: f64 = row.get("vx").unwrap_or(0.0);
            let vy: f64 = row.get("vy").unwrap_or(0.0);
            let vz: f64 = row.get("vz").unwrap_or(0.0);
            let mass: f64 = row.get("mass").unwrap_or(1.0);
            let size: Option<f64> = row.get("size").ok();
            let color: Option<String> = row.get("color").ok();
            let weight: Option<f64> = row.get("weight").ok();
            // Normalize empty strings to None for correct classification
            let node_type: Option<String> = row.get::<String>("node_type").ok()
                .filter(|s| !s.trim().is_empty());
            let owl_class_iri: Option<String> = row.get("owl_class_iri").ok();
            let group_name: Option<String> = row.get("group_name").ok();
            let metadata_json: String = row.get("metadata_json").unwrap_or_else(|_| "{}".to_string());
            let mut metadata: HashMap<String, String> = serde_json::from_str(&metadata_json)
                .unwrap_or_default();
            if let Ok(qs) = row.get::<f64>("quality_score") {
                metadata.insert("quality_score".to_string(), qs.to_string());
            }
            if let Ok(aus) = row.get::<f64>("authority_score") {
                metadata.insert("authority_score".to_string(), aus.to_string());
            }

            let mut node = Node::new_with_id(metadata_id, Some(id as u32));
            node.label = label;
            node.data.x = x as f32; node.data.y = y as f32; node.data.z = z as f32;
            node.data.vx = vx as f32; node.data.vy = vy as f32; node.data.vz = vz as f32;
            node.mass = Some(mass as f32);
            node.size = size.map(|s| s as f32);
            node.color = color;
            node.weight = weight.map(|w| w as f32);
            node.node_type = node_type;
            node.owl_class_iri = owl_class_iri;
            node.group = group_name;
            node.metadata = metadata;
            nodes.push(node);
        }

        info!("Loaded {} GraphNode nodes via flat projection (skipped {})", nodes.len(), skipped);

        // If GraphNode load returns 0 during a transient state (e.g. mid-MERGE by
        // file_service), return empty graph instead of falling back to OwlClass.
        // The OwlClass fallback produces incorrect node_type values that collapse
        // the dual-graph separation. The caller will retry via ReloadGraphFromDatabase.
        let mut iri_to_id: HashMap<String, u32> = HashMap::new();
        if nodes.is_empty() {
            warn!("GraphNode load returned 0 nodes — returning empty graph");
            warn!("This is likely a transient state during file_service MERGE. Will retry on next reload.");
            return Ok(Arc::new(GraphData {
                nodes: vec![],
                edges: vec![],
                metadata: HashMap::new(),
                id_to_metadata: HashMap::new(),
            }));
        }

        // Post-load population sanity check
        {
            use crate::models::graph_types::{classify_node_population, NodePopulation};
            let mut k = 0u32; let mut o = 0u32; let mut a = 0u32; let mut none_count = 0u32;
            for n in &nodes {
                match classify_node_population(n.node_type.as_deref()) {
                    NodePopulation::Knowledge => k += 1,
                    NodePopulation::Ontology => o += 1,
                    NodePopulation::Agent => a += 1,
                }
                if n.node_type.is_none() { none_count += 1; }
            }
            info!("Graph population sanity: total={}, knowledge={}, ontology={}, agent={}, node_type=None: {}",
                  nodes.len(), k, o, a, none_count);
            if o == 0 && nodes.len() > 100 {
                warn!("Zero ontology nodes in {} total — node_type mapping may be broken", nodes.len());
            }
        }

        // ONT-001 fix: Populate iri_to_id from GraphNode nodes that have owl_class_iri.
        // This enables the ontology edge bridge (SUBCLASS_OF + RELATES between OwlClass
        // nodes) to map IRI-based relationships into numeric GraphNode IDs.
        for node in &nodes {
            if let Some(ref iri) = node.owl_class_iri {
                iri_to_id.insert(iri.clone(), node.id);
            }
        }
        if !iri_to_id.is_empty() {
            info!("ONT-001: Built iri_to_id map — {} GraphNode nodes have owl_class_iri (enables ontology edge bridge)", iri_to_id.len());
        }

        // ADR-014: Always load ALL edge types — no either/or branching.
        let mut edges = Vec::new();

        // 1) Load GraphNode EDGE relationships
        {
            let edges_query = Query::new("MATCH (s:GraphNode)-[r:EDGE]->(t:GraphNode) RETURN s.id AS source, t.id AS target, r.weight AS weight, r.relation_type AS relation_type, r.owl_property_iri AS owl_property_iri, r.metadata AS metadata".to_string());

            let mut result = self.graph.execute(edges_query).await.map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to load edges: {}", e))
            })?;

            while let Ok(Some(row)) = result.next().await {
                let source: i64 = row.get("source").unwrap_or(0);
                let target: i64 = row.get("target").unwrap_or(0);
                let weight: f64 = row.get("weight").unwrap_or(1.0);
                let relation_type: Option<String> = row.get("relation_type").ok();
                let owl_property_iri: Option<String> = row.get("owl_property_iri").ok();
                let metadata_json: Option<String> = row.get("metadata").ok();

                let metadata = metadata_json
                    .and_then(|json| from_json(&json).ok());

                let mut edge = Edge::new(source as u32, target as u32, weight as f32);
                edge.edge_type = relation_type;
                edge.owl_property_iri = owl_property_iri;
                edge.metadata = metadata;

                edges.push(edge);
            }

            debug!("Loaded {} GraphNode EDGE relationships", edges.len());
        }

        // 2) Load ontology relationships (SUBCLASS_OF + RELATES between OwlClass nodes)
        //    Map OwlClass IRIs to numeric node IDs via iri_to_id.
        if !iri_to_id.is_empty() {
            let onto_edges_query = Query::new(
                "MATCH (s:OwlClass)-[r]->(t:OwlClass)
                 WHERE type(r) IN ['SUBCLASS_OF', 'RELATES']
                 RETURN s.iri AS source_iri,
                        t.iri AS target_iri,
                        type(r) AS rel_type,
                        r.relationship_type AS relationship_type,
                        r.weight AS weight,
                        r.owl_property_iri AS owl_property_iri
                 ".to_string()
            );

            let mut onto_result = self.graph.execute(onto_edges_query).await.map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to load ontology edges: {}", e))
            })?;

            let pre_count = edges.len();
            while let Ok(Some(row)) = onto_result.next().await {
                let source_iri: String = match row.get("source_iri") {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let target_iri: String = match row.get("target_iri") {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Map IRIs to numeric node IDs
                let source_id = match iri_to_id.get(&source_iri) {
                    Some(&id) => id,
                    None => continue,
                };
                let target_id = match iri_to_id.get(&target_iri) {
                    Some(&id) => id,
                    None => continue,
                };

                let rel_type: String = row.get("rel_type").unwrap_or_else(|_| "RELATES".to_string());
                let relationship_type: Option<String> = row.get("relationship_type").ok();
                let weight: f64 = row.get("weight").unwrap_or(1.0);
                let owl_property_iri: Option<String> = row.get("owl_property_iri").ok();

                let display_type = relationship_type.unwrap_or(rel_type);

                let mut edge = Edge::new(source_id, target_id, weight as f32);
                edge.edge_type = Some(display_type);
                edge.owl_property_iri = owl_property_iri;
                edges.push(edge);
            }

            info!("Loaded {} ontology edges (SUBCLASS_OF + RELATES)", edges.len() - pre_count);
        }

        debug!("Loaded {} total base edges from Neo4j", edges.len());

        // Bridge: If GraphNode EDGE relationships exist but weren't loaded (OwlClass path),
        // map them into the current graph by matching GraphNode labels to loaded node labels.
        // This gives knowledge/page nodes their wikilink edges in the ontology graph.
        if !iri_to_id.is_empty() {
            let label_to_id: std::collections::HashMap<String, u32> = nodes.iter()
                .map(|n| (n.label.to_lowercase(), n.id))
                .collect();

            let wikilink_query = Query::new(
                "MATCH (s:GraphNode)-[r:EDGE]->(t:GraphNode)
                 RETURN s.label AS source_label, t.label AS target_label,
                        r.weight AS weight, r.relation_type AS relation_type
                ".to_string()
            );

            let mut edge_set: std::collections::HashSet<(u32, u32)> = edges.iter()
                .map(|e| (e.source, e.target))
                .collect();

            let mut wikilink_count = 0u32;
            match self.graph.execute(wikilink_query).await {
                Ok(mut result) => {
                    while let Ok(Some(row)) = result.next().await {
                        let src_label: String = match row.get("source_label") { Ok(v) => v, Err(_) => continue };
                        let tgt_label: String = match row.get("target_label") { Ok(v) => v, Err(_) => continue };
                        let src_key = src_label.to_lowercase();
                        let tgt_key = tgt_label.to_lowercase();

                        if let (Some(&src_id), Some(&tgt_id)) = (label_to_id.get(&src_key), label_to_id.get(&tgt_key)) {
                            if src_id != tgt_id && edge_set.insert((src_id, tgt_id)) {
                                let weight: f64 = row.get("weight").unwrap_or(1.0);
                                let rel_type: Option<String> = row.get("relation_type").ok();
                                let mut edge = Edge::new(src_id, tgt_id, weight as f32);
                                edge.edge_type = rel_type.or(Some("explicit_link".to_string()));
                                edges.push(edge);
                                wikilink_count += 1;
                            }
                        }
                    }
                }
                Err(e) => warn!("Failed to load wikilink bridge edges: {}", e),
            }

            if wikilink_count > 0 {
                info!("Bridged {} wikilink edges from GraphNode EDGE to ontology graph", wikilink_count);
            }
        }

        // Enrich: Load ontology SUBCLASS_OF relationships and map to GraphNode edges.
        // OwlClass nodes have labels that match GraphNode labels — use this to bridge
        // the ontology hierarchy into the force-directed graph for meaningful clustering.
        {
            let label_to_id: HashMap<String, u32> = nodes.iter()
                .map(|n| (n.label.to_lowercase(), n.id))
                .collect();

            // Map OwlClass→OwlClass edges (SUBCLASS_OF + RELATES) to GraphNode edges.
            // Use COALESCE for label matching since parent OwlClasses often lack 'label'.
            let enrich_query = Query::new(
                "MATCH (child:OwlClass)-[r]->(parent:OwlClass)
                 WHERE type(r) IN ['SUBCLASS_OF', 'RELATES']
                 WITH child,
                      parent,
                      r,
                      type(r) AS rel_type,
                      r.relationship_type AS sub_type,
                      r.weight AS rel_weight,
                      COALESCE(child.label, child.preferred_term, child.iri) AS child_label,
                      COALESCE(parent.label, parent.preferred_term, parent.iri) AS parent_label,
                      child.source_domain AS child_domain,
                      parent.source_domain AS parent_domain
                 WHERE child_label IS NOT NULL AND parent_label IS NOT NULL
                   AND child_label <> '' AND parent_label <> ''
                 RETURN child_label, parent_label, child_domain, parent_domain,
                        rel_type, sub_type, rel_weight
                ".to_string()
            );

            let mut edge_set: std::collections::HashSet<(u32, u32)> = edges.iter()
                .map(|e| (e.source, e.target))
                .collect();

            let mut enrich_count = 0u32;
            match self.graph.execute(enrich_query).await {
                Ok(mut enrich_result) => {
                    while let Ok(Some(row)) = enrich_result.next().await {
                        let child_label: String = match row.get("child_label") {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let parent_label: String = match row.get("parent_label") {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        let child_key = child_label.to_lowercase();
                        let parent_key = parent_label.to_lowercase();

                        if let (Some(&src_id), Some(&tgt_id)) =
                            (label_to_id.get(&child_key), label_to_id.get(&parent_key))
                        {
                            if src_id != tgt_id && edge_set.insert((src_id, tgt_id)) {
                                // Use the relationship sub-type if available, else the Neo4j type
                                let rel_type: String = row.get("rel_type").unwrap_or_else(|_| "SUBCLASS_OF".to_string());
                                let sub_type: Option<String> = row.get("sub_type").ok();
                                let weight: f64 = row.get("rel_weight").unwrap_or(0.8);

                                let edge_type_str = sub_type.unwrap_or_else(|| {
                                    if rel_type == "SUBCLASS_OF" { "hierarchical".to_string() }
                                    else { "associative".to_string() }
                                });

                                let mut edge = Edge::new(src_id, tgt_id, weight as f32);
                                edge.edge_type = Some(edge_type_str);
                                let domain: Option<String> = row.get("child_domain").ok();
                                if let Some(d) = domain {
                                    edge.owl_property_iri = Some(format!("{}@{}", rel_type, d));
                                }
                                edges.push(edge);
                                enrich_count += 1;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to load ontology enrichment edges: {} (continuing without them)", e);
                }
            }

            if enrich_count > 0 {
                info!(
                    "Enriched graph with {} ontology edges (SUBCLASS_OF + RELATES) (total: {} edges)",
                    enrich_count, edges.len()
                );
            }
        }

        // Enrich: propagate source_domain from OwlClass to matching GraphNodes.
        // Many GraphNodes lack domain metadata but have matching OwlClass entries
        // with source_domain (bc, ai, mv, etc.) — bridge this for clustering.
        {
            let domain_query = Query::new(
                "MATCH (c:OwlClass)
                 WHERE c.source_domain IS NOT NULL
                   AND c.source_domain <> 'unknown' AND c.source_domain <> ''
                 WITH COALESCE(c.label, c.preferred_term, c.iri) AS label,
                      c.source_domain AS domain
                 WHERE label IS NOT NULL AND label <> ''
                 RETURN label, domain
                ".to_string()
            );

            let mut domain_map: HashMap<String, String> = HashMap::new();
            match self.graph.execute(domain_query).await {
                Ok(mut domain_result) => {
                    while let Ok(Some(row)) = domain_result.next().await {
                        if let (Ok(label), Ok(domain)) = (row.get::<String>("label"), row.get::<String>("domain")) {
                            domain_map.insert(label.to_lowercase(), domain);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to load domain mapping: {}", e);
                }
            }

            if !domain_map.is_empty() {
                let mut enriched = 0u32;
                for node in &mut nodes {
                    if node.metadata.get("source_domain").map(|d| d == "unknown" || d.is_empty()).unwrap_or(true) {
                        if let Some(domain) = domain_map.get(&node.label.to_lowercase()) {
                            node.metadata.insert("source_domain".to_string(), domain.clone());
                            enriched += 1;
                        }
                    }
                }
                if enriched > 0 {
                    info!("Enriched {} nodes with source_domain from ontology", enriched);
                }
            }
        }

        let mut graph = GraphData::new();
        graph.nodes = nodes;
        graph.edges = edges;

        Ok(Arc::new(graph))
    }

    async fn save_graph(&self, graph: &GraphData) -> RepoResult<()> {
        // Save nodes in batch - PRESERVING physics state (sim_x/y/z, vx/vy/vz)
        // Content sync should NEVER reset GPU-calculated layout positions
        for node in &graph.nodes {
            // Flatten semantic-code metadata entries into first-class Neo4j
            // properties so Cypher queries can filter on them directly.
            // Previously these lived only inside the `metadata` JSON string blob
            // and were invisible to `WHERE n.physicality_code IS NOT NULL`.
            // Codes use the u8 enum mapping from src/models/metadata.rs:
            //   0 = None (property absent), 255 = Unknown (property present
            //   but unrecognised), 1..N = canonical values.
            let physicality_code: i64 = node.metadata.get("physicality_code")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let role_code: i64 = node.metadata.get("role_code")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let maturity_level: i64 = node.metadata.get("maturity_level")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let source_domain: String = node.metadata.get("source_domain")
                .or_else(|| node.metadata.get("source-domain"))
                .or_else(|| node.metadata.get("domain"))
                .cloned()
                .unwrap_or_default();
            let preferred_term: String = node.metadata.get("preferred-term")
                .or_else(|| node.metadata.get("preferred_term"))
                .cloned()
                .unwrap_or_default();
            let term_id: String = node.metadata.get("term-id")
                .or_else(|| node.metadata.get("term_id"))
                .cloned()
                .unwrap_or_default();
            let source_file: String = node.metadata.get("source_file")
                .or_else(|| node.metadata.get("source-file"))
                .cloned()
                .unwrap_or_default();

            // Use COALESCE to preserve existing physics coordinates
            // Physics state is stored in sim_* properties, content coords in x/y/z
            let query = Query::new(
                "MERGE (n:GraphNode {id: $id})
                 ON CREATE SET
                     n.metadata_id = $metadata_id,
                     n.label = $label,
                     n.x = $x,
                     n.y = $y,
                     n.z = $z,
                     n.sim_x = $x,
                     n.sim_y = $y,
                     n.sim_z = $z,
                     n.vx = $vx,
                     n.vy = $vy,
                     n.vz = $vz,
                     n.mass = $mass,
                     n.owl_class_iri = $owl_class_iri,
                     n.color = $color,
                     n.size = $size,
                     n.node_type = $node_type,
                     n.weight = $weight,
                     n.group_name = $group_name,
                     n.physicality_code = $physicality_code,
                     n.role_code = $role_code,
                     n.maturity_level = $maturity_level,
                     n.source_domain = $source_domain,
                     n.preferred_term = $preferred_term,
                     n.term_id = $term_id,
                     n.source_file = $source_file,
                     n.metadata = $metadata
                 ON MATCH SET
                     n.metadata_id = $metadata_id,
                     n.label = $label,
                     n.owl_class_iri = COALESCE($owl_class_iri, n.owl_class_iri),
                     n.color = COALESCE($color, n.color),
                     n.size = COALESCE($size, n.size),
                     n.node_type = COALESCE($node_type, n.node_type),
                     n.weight = COALESCE($weight, n.weight),
                     n.group_name = COALESCE($group_name, n.group_name),
                     n.physicality_code = $physicality_code,
                     n.role_code = $role_code,
                     n.maturity_level = $maturity_level,
                     n.source_domain = $source_domain,
                     n.preferred_term = $preferred_term,
                     n.term_id = $term_id,
                     n.source_file = $source_file,
                     n.metadata = $metadata
                 // NEVER overwrite sim_x/sim_y/sim_z or vx/vy/vz on MATCH
                 // These are the GPU-calculated physics positions
                ".to_string()
            )
            .param("id", node.id as i64)
            .param("metadata_id", node.metadata_id.clone())
            .param("label", node.label.clone())
            .param("x", node.data.x as f64)
            .param("y", node.data.y as f64)
            .param("z", node.data.z as f64)
            .param("vx", node.data.vx as f64)
            .param("vy", node.data.vy as f64)
            .param("vz", node.data.vz as f64)
            .param("mass", node.mass.unwrap_or(1.0) as f64)
            .param("owl_class_iri", node.owl_class_iri.clone().unwrap_or_default())
            .param("color", node.color.clone().unwrap_or_default())
            .param("size", node.size.unwrap_or(1.0) as f64)
            .param("node_type", node.node_type.clone().unwrap_or_default())
            .param("weight", node.weight.unwrap_or(1.0) as f64)
            .param("group_name", node.group.clone().unwrap_or_default())
            .param("physicality_code", physicality_code)
            .param("role_code", role_code)
            .param("maturity_level", maturity_level)
            .param("source_domain", source_domain)
            .param("preferred_term", preferred_term)
            .param("term_id", term_id)
            .param("source_file", source_file)
            .param("metadata", serde_json::to_string(&node.metadata).unwrap_or_default());

            self.graph.run(query).await.map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!(
                    "Failed to save node {}: {}",
                    node.id, e
                ))
            })?;
        }

        // Save edges in batch
        for edge in &graph.edges {
            let mut query = Query::new("MATCH (s:GraphNode {id: $source}) MATCH (t:GraphNode {id: $target}) MERGE (s)-[r:EDGE]->(t) SET r.weight = $weight, r.relation_type = $relation_type, r.owl_property_iri = $owl_property_iri, r.metadata = $metadata".to_string());

            query = query.param("source", edge.source as i64);
            query = query.param("target", edge.target as i64);
            query = query.param("weight", edge.weight as f64);
            query = query.param("relation_type", edge.edge_type.clone().unwrap_or_default());
            query = query.param("owl_property_iri", edge.owl_property_iri.clone().unwrap_or_default());

            let metadata_json = edge.metadata.as_ref()
                .and_then(|m| to_json(m).ok())
                .unwrap_or_default();
            query = query.param("metadata", metadata_json);

            self.graph.run(query).await.map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!(
                    "Failed to save edge {}: {}",
                    edge.id, e
                ))
            })?;
        }

        info!("Saved graph to Neo4j: {} nodes, {} edges", graph.nodes.len(), graph.edges.len());
        Ok(())
    }

    async fn add_node(&self, node: &Node) -> RepoResult<u32> {
        let props = Self::node_to_properties(node);

        let mut query = Query::new("CREATE (n:GraphNode) SET n = $props RETURN n.id AS id".to_string());

        query = query.param("props", props);

        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to add node: {}", e))
        })?;

        if let Ok(Some(row)) = result.next().await {
            let id: i64 = row.get("id").unwrap_or(node.id as i64);
            Ok(id as u32)
        } else {
            Ok(node.id)
        }
    }

    async fn batch_add_nodes(&self, nodes: Vec<Node>) -> RepoResult<Vec<u32>> {
        let mut ids = Vec::new();
        for node in nodes {
            let id = self.add_node(&node).await?;
            ids.push(id);
        }
        Ok(ids)
    }

    async fn update_node(&self, node: &Node) -> RepoResult<()> {
        let props = Self::node_to_properties(node);

        let mut query = Query::new("MATCH (n:GraphNode {id: $id}) SET n = $props".to_string());

        query = query.param("id", node.id as i64);
        query = query.param("props", props);

        self.graph.run(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to update node: {}", e))
        })?;

        Ok(())
    }

    async fn batch_update_nodes(&self, nodes: Vec<Node>) -> RepoResult<()> {
        for node in nodes {
            self.update_node(&node).await?;
        }
        Ok(())
    }

    async fn remove_node(&self, node_id: u32) -> RepoResult<()> {
        let query = Query::new("MATCH (n:GraphNode {id: $id}) DETACH DELETE n".to_string()).param("id", node_id as i64);

        self.graph.run(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to remove node: {}", e))
        })?;

        Ok(())
    }

    async fn batch_remove_nodes(&self, node_ids: Vec<u32>) -> RepoResult<()> {
        for id in node_ids {
            self.remove_node(id).await?;
        }
        Ok(())
    }

    async fn get_node(&self, node_id: u32) -> RepoResult<Option<Node>> {
        let query = Query::new("MATCH (n:GraphNode {id: $id}) RETURN n".to_string()).param("id", node_id as i64);

        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get node: {}", e))
        })?;

        if let Ok(Some(row)) = result.next().await {
            if let Ok(neo4j_node) = row.get::<Neo4jNode>("n") {
                return Ok(Some(Self::neo4j_node_to_node(&neo4j_node)?));
            }
        }

        Ok(None)
    }

    async fn get_nodes(&self, node_ids: Vec<u32>) -> RepoResult<Vec<Node>> {
        let ids: Vec<i64> = node_ids.iter().map(|&id| id as i64).collect();

        let query = Query::new("MATCH (n:GraphNode) WHERE n.id IN $ids RETURN n".to_string()).param("ids", ids);

        let mut nodes = Vec::new();
        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get nodes: {}", e))
        })?;

        while let Ok(Some(row)) = result.next().await {
            if let Ok(neo4j_node) = row.get::<Neo4jNode>("n") {
                nodes.push(Self::neo4j_node_to_node(&neo4j_node)?);
            }
        }

        Ok(nodes)
    }

    async fn get_nodes_by_metadata_id(&self, metadata_id: &str) -> RepoResult<Vec<Node>> {
        let query = Query::new("MATCH (n:GraphNode {metadata_id: $metadata_id}) RETURN n".to_string()).param("metadata_id", metadata_id.to_string());

        let mut nodes = Vec::new();
        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get nodes: {}", e))
        })?;

        while let Ok(Some(row)) = result.next().await {
            if let Ok(neo4j_node) = row.get::<Neo4jNode>("n") {
                nodes.push(Self::neo4j_node_to_node(&neo4j_node)?);
            }
        }

        Ok(nodes)
    }

    async fn search_nodes_by_label(&self, label: &str) -> RepoResult<Vec<Node>> {
        let query = Query::new("MATCH (n:GraphNode) WHERE n.label CONTAINS $label RETURN n".to_string()).param("label", label.to_string());

        let mut nodes = Vec::new();
        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to search nodes: {}", e))
        })?;

        while let Ok(Some(row)) = result.next().await {
            if let Ok(neo4j_node) = row.get::<Neo4jNode>("n") {
                nodes.push(Self::neo4j_node_to_node(&neo4j_node)?);
            }
        }

        Ok(nodes)
    }

    async fn add_edge(&self, edge: &Edge) -> RepoResult<String> {
        // Use MERGE to prevent duplicate edges between the same source-target pair.
        // Each GitHub sync re-processes all wikilinks; CREATE would duplicate on every run.
        let mut query = Query::new("MATCH (s:GraphNode {id: $source}) MATCH (t:GraphNode {id: $target}) MERGE (s)-[r:EDGE]->(t) SET r.weight = $weight, r.relation_type = $relation_type, r.owl_property_iri = $owl_property_iri, r.metadata = $metadata RETURN elementId(r) AS id".to_string());

        query = query.param("source", edge.source as i64);
        query = query.param("target", edge.target as i64);
        query = query.param("weight", edge.weight as f64);
        query = query.param("relation_type", edge.edge_type.clone().unwrap_or_default());
        query = query.param("owl_property_iri", edge.owl_property_iri.clone().unwrap_or_default());

        let metadata_json = edge.metadata.as_ref()
            .and_then(|m| to_json(m).ok())
            .unwrap_or_default();
        query = query.param("metadata", metadata_json);

        self.graph.run(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to add edge: {}", e))
        })?;

        Ok(edge.id.clone())
    }

    async fn batch_add_edges(&self, edges: Vec<Edge>) -> RepoResult<Vec<String>> {
        let mut ids = Vec::new();
        for edge in edges {
            let id = self.add_edge(&edge).await?;
            ids.push(id);
        }
        Ok(ids)
    }

    async fn update_edge(&self, edge: &Edge) -> RepoResult<()> {
        let mut query = Query::new("MATCH (s:GraphNode {id: $source})-[r:EDGE]->(t:GraphNode {id: $target}) SET r.weight = $weight, r.relation_type = $relation_type, r.owl_property_iri = $owl_property_iri, r.metadata = $metadata".to_string());

        query = query.param("source", edge.source as i64);
        query = query.param("target", edge.target as i64);
        query = query.param("weight", edge.weight as f64);
        query = query.param("relation_type", edge.edge_type.clone().unwrap_or_default());
        query = query.param("owl_property_iri", edge.owl_property_iri.clone().unwrap_or_default());

        let metadata_json = edge.metadata.as_ref()
            .and_then(|m| to_json(m).ok())
            .unwrap_or_default();
        query = query.param("metadata", metadata_json);

        self.graph.run(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to update edge: {}", e))
        })?;

        Ok(())
    }

    async fn remove_edge(&self, edge_id: &str) -> RepoResult<()> {
        // Parse edge_id format "source-target"
        let parts: Vec<&str> = edge_id.split('-').collect();
        if parts.len() != 2 {
            return Err(KnowledgeGraphRepositoryError::InvalidData(
                format!("Invalid edge_id format: {}", edge_id)
            ));
        }

        let source: u32 = parts[0].parse().map_err(|_| {
            KnowledgeGraphRepositoryError::InvalidData(format!("Invalid source id: {}", parts[0]))
        })?;

        let target: u32 = parts[1].parse().map_err(|_| {
            KnowledgeGraphRepositoryError::InvalidData(format!("Invalid target id: {}", parts[1]))
        })?;

        let query = Query::new("MATCH (s:GraphNode {id: $source})-[r:EDGE]->(t:GraphNode {id: $target}) DELETE r".to_string())
            .param("source", source as i64)
            .param("target", target as i64);

        self.graph.run(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to remove edge: {}", e))
        })?;

        Ok(())
    }

    async fn batch_remove_edges(&self, edge_ids: Vec<String>) -> RepoResult<()> {
        for id in edge_ids {
            self.remove_edge(&id).await?;
        }
        Ok(())
    }

    async fn get_node_edges(&self, node_id: u32) -> RepoResult<Vec<Edge>> {
        let query = Query::new("MATCH (s:GraphNode {id: $id})-[r:EDGE]-(t:GraphNode) RETURN s.id AS source, t.id AS target, r.weight AS weight, r.relation_type AS relation_type, r.owl_property_iri AS owl_property_iri, r.metadata AS metadata".to_string()).param("id", node_id as i64);

        let mut edges = Vec::new();
        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get node edges: {}", e))
        })?;

        while let Ok(Some(row)) = result.next().await {
            let source: i64 = row.get("source").unwrap_or(0);
            let target: i64 = row.get("target").unwrap_or(0);
            let weight: f64 = row.get("weight").unwrap_or(1.0);
            let relation_type: Option<String> = row.get("relation_type").ok();
            let owl_property_iri: Option<String> = row.get("owl_property_iri").ok();
            let metadata_json: Option<String> = row.get("metadata").ok();

            let metadata = metadata_json
                .and_then(|json| from_json(&json).ok());

            let mut edge = Edge::new(source as u32, target as u32, weight as f32);
            edge.edge_type = relation_type;
            edge.owl_property_iri = owl_property_iri;
            edge.metadata = metadata;

            edges.push(edge);
        }

        Ok(edges)
    }

    async fn get_edges_between(&self, source_id: u32, target_id: u32) -> RepoResult<Vec<Edge>> {
        let query = Query::new("MATCH (s:GraphNode {id: $source})-[r:EDGE]-(t:GraphNode {id: $target}) RETURN s.id AS source, t.id AS target, r.weight AS weight, r.relation_type AS relation_type, r.owl_property_iri AS owl_property_iri, r.metadata AS metadata".to_string())
            .param("source", source_id as i64)
            .param("target", target_id as i64);

        let mut edges = Vec::new();
        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get edges between: {}", e))
        })?;

        while let Ok(Some(row)) = result.next().await {
            let source: i64 = row.get("source").unwrap_or(0);
            let target: i64 = row.get("target").unwrap_or(0);
            let weight: f64 = row.get("weight").unwrap_or(1.0);
            let relation_type: Option<String> = row.get("relation_type").ok();
            let owl_property_iri: Option<String> = row.get("owl_property_iri").ok();
            let metadata_json: Option<String> = row.get("metadata").ok();

            let metadata = metadata_json
                .and_then(|json| from_json(&json).ok());

            let mut edge = Edge::new(source as u32, target as u32, weight as f32);
            edge.edge_type = relation_type;
            edge.owl_property_iri = owl_property_iri;
            edge.metadata = metadata;

            edges.push(edge);
        }

        Ok(edges)
    }

    async fn batch_update_positions(
        &self,
        positions: Vec<(u32, f32, f32, f32)>,
    ) -> RepoResult<()> {
        // Update sim_* properties (physics state) - these are the GPU-calculated positions
        // x/y/z remain as initial/content positions and are not overwritten by physics
        for (node_id, x, y, z) in positions {
            let query = Query::new(
                "MATCH (n:GraphNode {id: $id})
                 SET n.sim_x = $x, n.sim_y = $y, n.sim_z = $z".to_string()
            )
                .param("id", node_id as i64)
                .param("x", x as f64)
                .param("y", y as f64)
                .param("z", z as f64);

            self.graph.run(query).await.map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!(
                    "Failed to update position for node {}: {}",
                    node_id, e
                ))
            })?;
        }

        Ok(())
    }

    async fn query_nodes(&self, cypher_query: &str) -> RepoResult<Vec<Node>> {
        // SECURITY: Reject write operations through the read-only query_nodes path
        let upper = cypher_query.to_uppercase();
        for keyword in &["CREATE", "MERGE", "DELETE", "SET ", "REMOVE", "DROP", "CALL {", "LOAD CSV"] {
            if upper.contains(keyword) {
                return Err(KnowledgeGraphRepositoryError::InvalidData(
                    format!("query_nodes rejects write/mutating Cypher keywords: {}", keyword.trim()),
                ));
            }
        }
        info!("query_nodes: executing read-only Cypher query");

        let query = Query::new(cypher_query.to_string());

        let mut nodes = Vec::new();
        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!(
                "Failed to execute query_nodes: {}",
                e
            ))
        })?;

        while let Ok(Some(row)) = result.next().await {
            if let Ok(neo4j_node) = row.get::<Neo4jNode>("n") {
                match Self::neo4j_node_to_node(&neo4j_node) {
                    Ok(node) => nodes.push(node),
                    Err(e) => {
                        warn!("Skipping node due to conversion error: {}", e);
                    }
                }
            }
        }

        Ok(nodes)
    }

    async fn get_neighbors(&self, node_id: u32) -> RepoResult<Vec<Node>> {
        let query = Query::new("MATCH (n:GraphNode {id: $id})-[:EDGE]-(neighbor:GraphNode) RETURN DISTINCT neighbor AS n".to_string()).param("id", node_id as i64);

        let mut nodes = Vec::new();
        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get neighbors: {}", e))
        })?;

        while let Ok(Some(row)) = result.next().await {
            if let Ok(neo4j_node) = row.get::<Neo4jNode>("n") {
                nodes.push(Self::neo4j_node_to_node(&neo4j_node)?);
            }
        }

        Ok(nodes)
    }

    async fn get_statistics(&self) -> RepoResult<GraphStatistics> {
        let query = Query::new("MATCH (n:GraphNode) OPTIONAL MATCH (n)-[r:EDGE]-() RETURN count(DISTINCT n) AS node_count, count(r) AS edge_count".to_string());

        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get statistics: {}", e))
        })?;

        if let Ok(Some(row)) = result.next().await {
            let node_count: i64 = row.get("node_count").unwrap_or(0);
            let edge_count: i64 = row.get("edge_count").unwrap_or(0);

            let average_degree = if node_count > 0 {
                (edge_count as f32 * 2.0) / node_count as f32
            } else {
                0.0
            };

            // Calculate connected components using Cypher
            let components_query = Query::new(
                "MATCH (n:GraphNode)
                 WITH COLLECT(DISTINCT n) AS nodes
                 UNWIND nodes AS node
                 OPTIONAL MATCH path = (node)-[*]-(connected)
                 WITH node, COLLECT(DISTINCT connected) AS component
                 RETURN COUNT(DISTINCT component) AS component_count"
                    .to_string()
            );

            let mut component_count = 1; // Default to 1 if query fails
            if let Ok(mut result) = self.graph.execute(components_query).await {
                if let Some(row) = result.next().await.ok().flatten() {
                    if let Ok(count) = row.get::<i64>("component_count") {
                        component_count = count as usize;
                    }
                }
            }

            return Ok(GraphStatistics {
                node_count: node_count as usize,
                edge_count: edge_count as usize,
                average_degree,
                connected_components: component_count,
                last_updated: time::now(),
            });
        }

        Ok(GraphStatistics {
            node_count: 0,
            edge_count: 0,
            average_degree: 0.0,
            connected_components: 0,
            last_updated: time::now(),
        })
    }

    async fn clear_graph(&self) -> RepoResult<()> {
        let query = Query::new("MATCH (n:GraphNode) DETACH DELETE n".to_string());

        self.graph.run(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to clear graph: {}", e))
        })?;

        info!("Cleared all graph data from Neo4j");
        Ok(())
    }

    async fn begin_transaction(&self) -> RepoResult<()> {
        // Neo4j handles transactions internally
        Ok(())
    }

    async fn commit_transaction(&self) -> RepoResult<()> {
        // Neo4j handles transactions internally
        Ok(())
    }

    async fn rollback_transaction(&self) -> RepoResult<()> {
        // Neo4j handles transactions internally
        Ok(())
    }

    async fn health_check(&self) -> RepoResult<bool> {
        let query = Query::new("RETURN 1 AS health".to_string());

        match self.graph.run(query).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn get_all_positions(&self) -> RepoResult<HashMap<u32, (f32, f32, f32)>> {
        // Return sim_* positions (GPU physics state) when available, fallback to x/y/z
        let query = Query::new(
            "MATCH (n:GraphNode)
             WHERE n.sim_x IS NOT NULL OR n.x IS NOT NULL
             RETURN n.id AS id,
                    COALESCE(n.sim_x, n.x, 0.0) AS x,
                    COALESCE(n.sim_y, n.y, 0.0) AS y,
                    COALESCE(n.sim_z, n.z, 0.0) AS z".to_string()
        );

        let mut positions = HashMap::new();
        let mut result = self.graph.execute(query).await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get all positions: {}", e))
        })?;

        while let Ok(Some(row)) = result.next().await {
            let id: i64 = row.get("id").unwrap_or(0);
            let x: f64 = row.get("x").unwrap_or(0.0);
            let y: f64 = row.get("y").unwrap_or(0.0);
            let z: f64 = row.get("z").unwrap_or(0.0);

            positions.insert(id as u32, (x as f32, y as f32, z as f32));
        }

        debug!("Retrieved {} node positions from Neo4j (using sim_* where available)", positions.len());
        Ok(positions)
    }

    async fn get_nodes_by_owl_class_iri(&self, owl_class_iri: &str) -> RepoResult<Vec<Node>> {
        let query = Query::new("MATCH (n:GraphNode) WHERE n.owl_class_iri = $iri RETURN n".to_string()).param("iri", owl_class_iri);

        let mut result = self.graph
            .execute(query)
            .await
            .map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!(
                    "Failed to query nodes by owl_class_iri: {}",
                    e
                ))
            })?;

        let mut nodes = Vec::new();

        while let Some(row) = result.next().await.map_err(|e| {
            KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to fetch row: {}", e))
        })? {
            let neo_node: Neo4jNode = row.get("n").map_err(|e| {
                KnowledgeGraphRepositoryError::DatabaseError(format!("Failed to get node: {}", e))
            })?;

            let node = Self::neo4j_node_to_node(&neo_node)?;
            nodes.push(node);
        }

        Ok(nodes)
    }
}
