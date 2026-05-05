// src/handlers/discovery_handler.rs
//! Feature Discovery REST Endpoint
//!
//! Combined content + topology embedding similarity search.
//! Content embeddings: 384-dim MiniLM vectors from text.
//! KGE topology embeddings: 128-dim TransE vectors from graph structure.

use actix_web::{web, HttpResponse};
use neo4rs::Query;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use crate::adapters::neo4j_adapter::Neo4jAdapter;
use crate::AppState;

// ---------------------------------------------------------------------------
// Request/Response types
// ---------------------------------------------------------------------------

/// Query parameters for discovery search
#[derive(Debug, Deserialize)]
pub struct DiscoveryQuery {
    /// Text query to search for
    pub q: String,
    /// Number of results (default: 10)
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Weight for content similarity (0.0-1.0, default: 0.6)
    #[serde(default = "default_content_weight")]
    pub content_weight: f32,
    /// Weight for topology similarity (0.0-1.0, default: 0.4)
    #[serde(default = "default_topology_weight")]
    pub topology_weight: f32,
    /// Filter by node type (optional)
    pub node_type: Option<String>,
    /// Filter by domain (optional)
    pub domain: Option<String>,
}

fn default_top_k() -> usize {
    10
}
fn default_content_weight() -> f32 {
    0.6
}
fn default_topology_weight() -> f32 {
    0.4
}

/// A single discovery result
#[derive(Debug, Serialize)]
pub struct DiscoveryResult {
    pub iri: String,
    pub label: String,
    pub score: f32,
    pub content_score: f32,
    pub topology_score: f32,
    pub node_type: String,
    pub domain: Option<String>,
    pub definition: Option<String>,
}

/// Response for discovery search
#[derive(Debug, Serialize)]
pub struct DiscoveryResponse {
    pub results: Vec<DiscoveryResult>,
    pub query: String,
    pub total_candidates: usize,
    pub weights: WeightConfig,
}

#[derive(Debug, Serialize)]
pub struct WeightConfig {
    pub content: f32,
    pub topology: f32,
}

/// Related nodes response (for hover/expand in UI)
#[derive(Debug, Serialize)]
pub struct RelatedNodesResponse {
    pub source_iri: String,
    pub related: Vec<RelatedNode>,
}

#[derive(Debug, Serialize)]
pub struct RelatedNode {
    pub iri: String,
    pub label: String,
    pub similarity: f32,
    pub relationship: String, // "content_similar", "structurally_similar", "combined"
}

/// Query for related nodes
#[derive(Debug, Deserialize)]
pub struct RelatedQuery {
    #[serde(default = "default_related_k")]
    pub top_k: usize,
}

fn default_related_k() -> usize {
    5
}

/// Query for gap detection
#[derive(Debug, Deserialize)]
pub struct GapsQuery {
    pub domain: Option<String>,
    #[serde(default = "default_min_score")]
    pub min_score: f32,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_min_score() -> f32 {
    0.3
}

/// A detected ontology gap
#[derive(Debug, Serialize)]
pub struct OntologyGap {
    pub node_a_iri: String,
    pub node_a_label: String,
    pub node_b_iri: String,
    pub node_b_label: String,
    pub similarity: f32,
    pub gap_type: String, // "missing_edge", "weak_connection"
}

/// Response for gap detection
#[derive(Debug, Serialize)]
pub struct GapsResponse {
    pub gaps: Vec<OntologyGap>,
    pub total_checked: usize,
    pub domain: Option<String>,
}

/// Batch similarity request body
#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub iris: Vec<String>,
    #[serde(default = "default_related_k")]
    pub top_k: usize,
}

/// Batch similarity response
#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub results: Vec<RelatedNodesResponse>,
}

/// Embedding response from external service
#[derive(Debug, Deserialize)]
struct EmbeddingServiceResponse {
    #[serde(alias = "embedding", alias = "vector")]
    pub embeddings: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Internal node representation fetched from Neo4j
// ---------------------------------------------------------------------------

struct EmbeddedNode {
    iri: String,
    label: String,
    node_type: String,
    domain: Option<String>,
    definition: Option<String>,
    content_embedding: Vec<f32>,
    kge_embedding: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Math utilities
// ---------------------------------------------------------------------------

/// Cosine similarity between two vectors. Returns 0.0 if either is zero-length.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;

    for (x, y) in a.iter().zip(b.iter()) {
        let xf = *x as f64;
        let yf = *y as f64;
        dot += xf * yf;
        norm_a += xf * xf;
        norm_b += yf * yf;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-12 {
        return 0.0;
    }

    (dot / denom) as f32
}

/// Normalize weights so they sum to 1.0. If both are zero, returns (0.5, 0.5).
pub fn normalize_weights(content_weight: f32, topology_weight: f32) -> (f32, f32) {
    let sum = content_weight.abs() + topology_weight.abs();
    if sum < 1e-9 {
        return (0.5, 0.5);
    }
    (content_weight.abs() / sum, topology_weight.abs() / sum)
}

// ---------------------------------------------------------------------------
// Neo4j data fetching
// ---------------------------------------------------------------------------

/// Fetch all nodes that have both embedding properties from Neo4j.
async fn fetch_embedded_nodes(
    neo4j: &Arc<Neo4jAdapter>,
    node_type: Option<&str>,
    domain: Option<&str>,
) -> Result<Vec<EmbeddedNode>, String> {
    let mut cypher = String::from(
        "MATCH (n) WHERE n.content_embedding_384 IS NOT NULL \
         AND n.kge_embedding_128 IS NOT NULL",
    );

    if let Some(nt) = node_type {
        cypher.push_str(&format!(" AND n.node_type = '{}'", nt.replace('\'', "''")));
    }
    if let Some(d) = domain {
        cypher.push_str(&format!(" AND n.domain = '{}'", d.replace('\'', "''")));
    }

    cypher.push_str(
        " RETURN n.iri AS iri, n.label AS label, \
         n.node_type AS node_type, n.domain AS domain, \
         n.definition AS definition, \
         n.content_embedding_384 AS content_embedding, \
         n.kge_embedding_128 AS kge_embedding",
    );

    let graph = neo4j.graph();
    let query = Query::new(cypher);
    let mut stream = graph
        .execute(query)
        .await
        .map_err(|e| format!("Neo4j query failed: {e}"))?;

    let mut nodes = Vec::new();

    while let Some(row) = stream.next().await.map_err(|e| format!("Row fetch error: {e}"))? {
        let iri: String = row.get("iri").unwrap_or_default();
        let label: String = row.get("label").unwrap_or_default();
        let node_type_val: String = row.get("node_type").unwrap_or_else(|_| "unknown".to_string());
        let domain_val: Option<String> = row.get("domain").ok();
        let definition: Option<String> = row.get("definition").ok();

        // Neo4j stores embeddings as lists of floats
        let content_embedding: Vec<f32> = row
            .get::<Vec<f64>>("content_embedding")
            .unwrap_or_default()
            .into_iter()
            .map(|v| v as f32)
            .collect();

        let kge_embedding: Vec<f32> = row
            .get::<Vec<f64>>("kge_embedding")
            .unwrap_or_default()
            .into_iter()
            .map(|v| v as f32)
            .collect();

        if content_embedding.is_empty() || kge_embedding.is_empty() {
            continue;
        }

        nodes.push(EmbeddedNode {
            iri,
            label,
            node_type: node_type_val,
            domain: domain_val,
            definition,
            content_embedding,
            kge_embedding,
        });
    }

    Ok(nodes)
}

/// Fetch a single node's embeddings by IRI.
async fn fetch_node_by_iri(
    neo4j: &Arc<Neo4jAdapter>,
    iri: &str,
) -> Result<Option<EmbeddedNode>, String> {
    let cypher = "MATCH (n {iri: $iri}) \
        WHERE n.content_embedding_384 IS NOT NULL AND n.kge_embedding_128 IS NOT NULL \
        RETURN n.iri AS iri, n.label AS label, \
        n.node_type AS node_type, n.domain AS domain, \
        n.definition AS definition, \
        n.content_embedding_384 AS content_embedding, \
        n.kge_embedding_128 AS kge_embedding \
        LIMIT 1";

    let graph = neo4j.graph();
    let query = Query::new(cypher.to_string()).param("iri", iri.to_string());
    let mut stream = graph
        .execute(query)
        .await
        .map_err(|e| format!("Neo4j query failed: {e}"))?;

    if let Some(row) = stream.next().await.map_err(|e| format!("Row fetch error: {e}"))? {
        let iri_val: String = row.get("iri").unwrap_or_default();
        let label: String = row.get("label").unwrap_or_default();
        let node_type_val: String = row.get("node_type").unwrap_or_else(|_| "unknown".to_string());
        let domain_val: Option<String> = row.get("domain").ok();
        let definition: Option<String> = row.get("definition").ok();

        let content_embedding: Vec<f32> = row
            .get::<Vec<f64>>("content_embedding")
            .unwrap_or_default()
            .into_iter()
            .map(|v| v as f32)
            .collect();

        let kge_embedding: Vec<f32> = row
            .get::<Vec<f64>>("kge_embedding")
            .unwrap_or_default()
            .into_iter()
            .map(|v| v as f32)
            .collect();

        if content_embedding.is_empty() || kge_embedding.is_empty() {
            return Ok(None);
        }

        Ok(Some(EmbeddedNode {
            iri: iri_val,
            label,
            node_type: node_type_val,
            domain: domain_val,
            definition,
            content_embedding,
            kge_embedding,
        }))
    } else {
        Ok(None)
    }
}

/// Check whether a direct edge exists between two nodes.
async fn edge_exists(neo4j: &Arc<Neo4jAdapter>, iri_a: &str, iri_b: &str) -> bool {
    let cypher = "MATCH (a {iri: $a})-[r]-(b {iri: $b}) RETURN count(r) AS cnt LIMIT 1";
    let graph = neo4j.graph();
    let query = Query::new(cypher.to_string())
        .param("a", iri_a.to_string())
        .param("b", iri_b.to_string());

    match graph.execute(query).await {
        Ok(mut stream) => {
            if let Ok(Some(row)) = stream.next().await {
                let cnt: i64 = row.get("cnt").unwrap_or(0);
                cnt > 0
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Embedding service client
// ---------------------------------------------------------------------------

/// Call the external embedding service to vectorize query text.
async fn embed_text(text: &str) -> Result<Vec<f32>, String> {
    let embed_url = std::env::var("EMBEDDING_SERVICE_URL")
        .unwrap_or_else(|_| "http://ruvector-postgres:8080/embed".to_string());

    let client = reqwest::Client::new();

    #[derive(Serialize)]
    struct EmbedRequest<'a> {
        text: &'a str,
    }

    let resp = client
        .post(&embed_url)
        .json(&EmbedRequest { text })
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Embedding service request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Embedding service returned {status}: {body}"
        ));
    }

    let parsed: EmbeddingServiceResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse embedding response: {e}"))?;

    Ok(parsed.embeddings)
}

// ---------------------------------------------------------------------------
// Endpoint handlers
// ---------------------------------------------------------------------------

/// GET /api/discovery/search?q=...&top_k=10&content_weight=0.6&topology_weight=0.4
///
/// Embeds query text, fetches all nodes with both embedding types from Neo4j,
/// computes combined similarity score, and returns top-k results.
pub async fn search(
    app_state: web::Data<AppState>,
    query: web::Query<DiscoveryQuery>,
) -> Result<HttpResponse, actix_web::Error> {
    let q = &query.q;
    if q.trim().is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Query parameter 'q' must not be empty"
        })));
    }

    let (cw, tw) = normalize_weights(query.content_weight, query.topology_weight);

    info!(
        "Discovery search: q='{}', top_k={}, weights=({:.2}, {:.2})",
        q, query.top_k, cw, tw
    );

    // 1. Embed the query text
    let query_embedding = match embed_text(q).await {
        Ok(emb) => emb,
        Err(e) => {
            warn!("Embedding service error: {e}");
            return Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "error": "Embedding service unavailable",
                "details": e
            })));
        }
    };

    // 2. Fetch all candidate nodes from Neo4j
    let nodes = match fetch_embedded_nodes(
        &app_state.neo4j_adapter,
        query.node_type.as_deref(),
        query.domain.as_deref(),
    )
    .await
    {
        Ok(n) => n,
        Err(e) => {
            warn!("Neo4j fetch error: {e}");
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch nodes from graph database",
                "details": e
            })));
        }
    };

    let total_candidates = nodes.len();

    // 3. Compute combined scores
    // For topology: we find the maximum KGE similarity between the query's
    // nearest content-match and all other nodes' KGE embeddings.
    // Since the query has no inherent topology embedding, we use the KGE of
    // the best content-match as proxy for topology context.
    let mut scored: Vec<(f32, f32, f32, &EmbeddedNode)> = Vec::with_capacity(nodes.len());

    // First pass: compute content scores and find the best content match's KGE
    let mut best_content_score = -1.0f32;
    let mut best_content_kge: Option<&[f32]> = None;

    for node in &nodes {
        let cs = cosine_similarity(&query_embedding, &node.content_embedding);
        if cs > best_content_score {
            best_content_score = cs;
            best_content_kge = Some(&node.kge_embedding);
        }
    }

    // Second pass: compute topology scores using the best match's KGE as anchor
    let anchor_kge = best_content_kge.unwrap_or(&[]);

    for node in &nodes {
        let content_score = cosine_similarity(&query_embedding, &node.content_embedding);
        let topology_score = if anchor_kge.is_empty() {
            0.0
        } else {
            cosine_similarity(anchor_kge, &node.kge_embedding)
        };
        let combined = cw * content_score + tw * topology_score;
        scored.push((combined, content_score, topology_score, node));
    }

    // 4. Sort descending by combined score
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // 5. Take top_k
    let results: Vec<DiscoveryResult> = scored
        .into_iter()
        .take(query.top_k)
        .map(|(score, cs, ts, node)| DiscoveryResult {
            iri: node.iri.clone(),
            label: node.label.clone(),
            score,
            content_score: cs,
            topology_score: ts,
            node_type: node.node_type.clone(),
            domain: node.domain.clone(),
            definition: node.definition.clone(),
        })
        .collect();

    Ok(HttpResponse::Ok().json(DiscoveryResponse {
        results,
        query: q.clone(),
        total_candidates,
        weights: WeightConfig {
            content: cw,
            topology: tw,
        },
    }))
}

/// GET /api/discovery/related/{iri}?top_k=5
///
/// Given a node IRI, find nodes with similar combined embeddings.
pub async fn related(
    app_state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<RelatedQuery>,
) -> Result<HttpResponse, actix_web::Error> {
    let iri = path.into_inner();

    let source_node = match fetch_node_by_iri(&app_state.neo4j_adapter, &iri).await {
        Ok(Some(n)) => n,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(serde_json::json!({
                "error": "Node not found or missing embeddings",
                "iri": iri
            })));
        }
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch node",
                "details": e
            })));
        }
    };

    // Fetch all candidates
    let nodes = match fetch_embedded_nodes(&app_state.neo4j_adapter, None, None).await {
        Ok(n) => n,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch candidate nodes",
                "details": e
            })));
        }
    };

    // Compute combined similarity (equal weight content + topology)
    let mut ranked: Vec<(f32, f32, f32, &EmbeddedNode)> = Vec::new();

    for node in &nodes {
        if node.iri == iri {
            continue; // skip self
        }
        let cs = cosine_similarity(&source_node.content_embedding, &node.content_embedding);
        let ts = cosine_similarity(&source_node.kge_embedding, &node.kge_embedding);
        let combined = 0.5 * cs + 0.5 * ts;
        ranked.push((combined, cs, ts, node));
    }

    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let related: Vec<RelatedNode> = ranked
        .into_iter()
        .take(query.top_k)
        .map(|(sim, cs, ts, node)| {
            let relationship = if cs > ts * 1.5 {
                "content_similar"
            } else if ts > cs * 1.5 {
                "structurally_similar"
            } else {
                "combined"
            };
            RelatedNode {
                iri: node.iri.clone(),
                label: node.label.clone(),
                similarity: sim,
                relationship: relationship.to_string(),
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(RelatedNodesResponse {
        source_iri: iri,
        related,
    }))
}

/// GET /api/discovery/gaps?domain=ai&min_score=0.3&top_k=10
///
/// Find ontology gaps: nodes with high content similarity but NO direct edge.
pub async fn gaps(
    app_state: web::Data<AppState>,
    query: web::Query<GapsQuery>,
) -> Result<HttpResponse, actix_web::Error> {
    let nodes = match fetch_embedded_nodes(
        &app_state.neo4j_adapter,
        None,
        query.domain.as_deref(),
    )
    .await
    {
        Ok(n) => n,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch nodes",
                "details": e
            })));
        }
    };

    let total_checked = nodes.len();
    let min_score = query.min_score;

    // O(N^2) pairwise comparison — acceptable for <10k ontology nodes
    let mut candidate_gaps: Vec<(f32, usize, usize)> = Vec::new();

    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let cs = cosine_similarity(
                &nodes[i].content_embedding,
                &nodes[j].content_embedding,
            );
            if cs >= min_score {
                candidate_gaps.push((cs, i, j));
            }
        }
    }

    // Sort by similarity descending
    candidate_gaps.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Check for missing edges (limit checks to avoid O(N^2) Neo4j queries)
    let check_limit = (query.top_k * 3).min(candidate_gaps.len());
    let mut gaps: Vec<OntologyGap> = Vec::new();

    for &(sim, i, j) in candidate_gaps.iter().take(check_limit) {
        if gaps.len() >= query.top_k {
            break;
        }

        let has_edge = edge_exists(&app_state.neo4j_adapter, &nodes[i].iri, &nodes[j].iri).await;
        if !has_edge {
            gaps.push(OntologyGap {
                node_a_iri: nodes[i].iri.clone(),
                node_a_label: nodes[i].label.clone(),
                node_b_iri: nodes[j].iri.clone(),
                node_b_label: nodes[j].label.clone(),
                similarity: sim,
                gap_type: "missing_edge".to_string(),
            });
        }
    }

    Ok(HttpResponse::Ok().json(GapsResponse {
        gaps,
        total_checked,
        domain: query.domain.clone(),
    }))
}

/// POST /api/discovery/batch
///
/// Batch similarity lookup for multiple nodes at once.
pub async fn batch_similar(
    app_state: web::Data<AppState>,
    body: web::Json<BatchRequest>,
) -> Result<HttpResponse, actix_web::Error> {
    if body.iris.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Field 'iris' must contain at least one IRI"
        })));
    }

    if body.iris.len() > 100 {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Batch limited to 100 IRIs per request"
        })));
    }

    // Fetch all candidates once
    let all_nodes = match fetch_embedded_nodes(&app_state.neo4j_adapter, None, None).await {
        Ok(n) => n,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to fetch candidate nodes",
                "details": e
            })));
        }
    };

    let mut results: Vec<RelatedNodesResponse> = Vec::with_capacity(body.iris.len());

    for iri in &body.iris {
        // Find the source node in our loaded set
        let source = all_nodes.iter().find(|n| &n.iri == iri);
        let source = match source {
            Some(s) => s,
            None => {
                // Node not found or missing embeddings — return empty
                results.push(RelatedNodesResponse {
                    source_iri: iri.clone(),
                    related: vec![],
                });
                continue;
            }
        };

        let mut ranked: Vec<(f32, f32, f32, &EmbeddedNode)> = Vec::new();
        for node in &all_nodes {
            if &node.iri == iri {
                continue;
            }
            let cs =
                cosine_similarity(&source.content_embedding, &node.content_embedding);
            let ts = cosine_similarity(&source.kge_embedding, &node.kge_embedding);
            let combined = 0.5 * cs + 0.5 * ts;
            ranked.push((combined, cs, ts, node));
        }

        ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let related: Vec<RelatedNode> = ranked
            .into_iter()
            .take(body.top_k)
            .map(|(sim, cs, ts, node)| {
                let relationship = if cs > ts * 1.5 {
                    "content_similar"
                } else if ts > cs * 1.5 {
                    "structurally_similar"
                } else {
                    "combined"
                };
                RelatedNode {
                    iri: node.iri.clone(),
                    label: node.label.clone(),
                    similarity: sim,
                    relationship: relationship.to_string(),
                }
            })
            .collect();

        results.push(RelatedNodesResponse {
            source_iri: iri.clone(),
            related,
        });
    }

    Ok(HttpResponse::Ok().json(BatchResponse { results }))
}

// ---------------------------------------------------------------------------
// Admin trigger endpoints
// ---------------------------------------------------------------------------

/// POST /api/discovery/index
///
/// Trigger embedding indexing of all OntologyClass nodes via MiniLM-L6 service.
pub async fn trigger_index(
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Admin: triggering embedding indexing");

    let graph = app_state.neo4j_adapter.graph().clone();
    let service = crate::services::embedding_service::EmbeddingService::new(graph);

    match service.index_ontology_nodes().await {
        Ok(stats) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "complete",
            "nodes_processed": stats.nodes_processed,
            "nodes_embedded": stats.nodes_embedded,
            "nodes_skipped": stats.nodes_skipped,
            "batches_sent": stats.batches_sent,
        }))),
        Err(e) => {
            warn!("Embedding indexing failed: {e}");
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Embedding indexing failed",
                "details": e.to_string()
            })))
        }
    }
}

/// POST /api/discovery/train
///
/// Trigger KGE (TransE) training on the full graph and store embeddings.
pub async fn trigger_train(
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Admin: triggering KGE training");

    let graph = app_state.neo4j_adapter.graph().clone();
    let trainer = crate::services::kge_trainer::KGETrainer::new(
        graph,
        crate::services::kge_trainer::KGEConfig::default(),
    );

    match trainer.train_and_store().await {
        Ok(result) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "complete",
            "num_entities": result.stats.num_entities,
            "num_relations": result.stats.num_relations,
            "num_triples": result.stats.num_triples,
            "final_loss": result.stats.final_loss,
            "epochs_completed": result.stats.epochs_completed,
            "duration_ms": result.stats.duration_ms,
        }))),
        Err(e) => {
            warn!("KGE training failed: {e}");
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "KGE training failed",
                "details": e.to_string()
            })))
        }
    }
}

/// POST /api/discovery/materialize
///
/// Trigger N-hop edge materialization (2-hop and 3-hop transitive edges).
pub async fn trigger_materialize(
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Admin: triggering N-hop materialization");

    let graph = app_state.neo4j_adapter.graph().clone();
    let materializer = match crate::services::nhop_materializer::NHopMaterializer::new(
        graph,
        crate::services::nhop_materializer::NHopConfig::default(),
    ) {
        Ok(m) => m,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Materializer configuration error",
                "details": e.to_string()
            })));
        }
    };

    match materializer.materialize_all().await {
        Ok(stats) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "complete",
            "two_hop_edges_created": stats.two_hop_edges_created,
            "three_hop_edges_created": stats.three_hop_edges_created,
            "nodes_processed": stats.nodes_processed,
            "duration_ms": stats.duration_ms,
        }))),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("disabled") {
                Ok(HttpResponse::ServiceUnavailable().json(serde_json::json!({
                    "error": "N-hop materialization is disabled",
                    "details": "Set NHOP_MATERIALIZATION_ENABLED=true to enable"
                })))
            } else {
                warn!("N-hop materialization failed: {e}");
                Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                    "error": "N-hop materialization failed",
                    "details": msg
                })))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Route configuration
// ---------------------------------------------------------------------------

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/discovery")
            .route("/search", web::get().to(search))
            .route("/related/{iri}", web::get().to(related))
            .route("/gaps", web::get().to(gaps))
            .route("/batch", web::post().to(batch_similar))
            .route("/index", web::post().to(trigger_index))
            .route("/train", web::post().to(trigger_train))
            .route("/materialize", web::post().to(trigger_materialize)),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6, "Expected 1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "Expected 0.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6, "Expected -1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_known_vectors() {
        // 45-degree angle in 2D → cos(45°) ≈ 0.7071
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        let expected = 1.0f32 / 2.0f32.sqrt();
        assert!(
            (sim - expected).abs() < 1e-5,
            "Expected {expected}, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_empty_vectors() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_normalize_weights_standard() {
        let (cw, tw) = normalize_weights(0.6, 0.4);
        assert!((cw - 0.6).abs() < 1e-6);
        assert!((tw - 0.4).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_weights_unbalanced() {
        let (cw, tw) = normalize_weights(3.0, 1.0);
        assert!((cw - 0.75).abs() < 1e-6);
        assert!((tw - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_weights_zero() {
        let (cw, tw) = normalize_weights(0.0, 0.0);
        assert!((cw - 0.5).abs() < 1e-6);
        assert!((tw - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_weights_sum_to_one() {
        let cases = vec![
            (0.3, 0.7),
            (1.0, 1.0),
            (0.9, 0.1),
            (5.0, 3.0),
            (0.01, 99.0),
        ];
        for (a, b) in cases {
            let (cw, tw) = normalize_weights(a, b);
            let sum = cw + tw;
            assert!(
                (sum - 1.0).abs() < 1e-5,
                "Weights {a},{b} normalized to {cw},{tw} (sum={sum})"
            );
        }
    }

    #[test]
    fn test_score_combination() {
        // Verify combined score computation
        let content_score = 0.8f32;
        let topology_score = 0.6f32;
        let (cw, tw) = normalize_weights(0.6, 0.4);
        let combined = cw * content_score + tw * topology_score;
        let expected = 0.6 * 0.8 + 0.4 * 0.6; // 0.48 + 0.24 = 0.72
        assert!(
            (combined - expected).abs() < 1e-5,
            "Expected {expected}, got {combined}"
        );
    }

    #[test]
    fn test_cosine_similarity_high_dimensional() {
        // 384-dim vectors (MiniLM size) — random normalized
        let mut a = vec![0.0f32; 384];
        let mut b = vec![0.0f32; 384];
        // Set a pattern: a and b share first 192 dims
        for i in 0..384 {
            a[i] = ((i as f32) * 0.01).sin();
            b[i] = if i < 192 {
                a[i]
            } else {
                ((i as f32) * 0.03).cos()
            };
        }
        let sim = cosine_similarity(&a, &b);
        // Should be positive (partial overlap) but less than 1.0
        assert!(sim > 0.0 && sim < 1.0, "Expected (0,1), got {sim}");
    }
}
