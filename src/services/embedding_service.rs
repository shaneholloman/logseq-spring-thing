//! Embedding Service — sentence embeddings for ontology nodes via MiniLM-L6-v2.
//!
//! Two operational modes:
//! - **HTTP mode**: calls a configurable embedding endpoint (default: ruvector MiniLM service)
//! - **Batch mode**: accepts `Vec<String>`, returns `Vec<Vec<f32>>`
//!
//! Integrates with Neo4j to index OntologyClass nodes and perform similarity search.

use log::{debug, info, warn};
use neo4rs::Graph;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Neo4j error: {0}")]
    Neo4j(String),
    #[error("No embedding endpoint configured")]
    NotConfigured,
    #[error("Embedding dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
    #[error("Empty input: no texts provided")]
    EmptyInput,
    #[error("Endpoint returned error: {0}")]
    EndpointError(String),
}

// ---------------------------------------------------------------------------
// Request / Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct EmbedRequest {
    texts: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Statistics returned after indexing ontology nodes.
#[derive(Debug, Clone)]
pub struct IndexingStats {
    pub nodes_processed: usize,
    pub nodes_embedded: usize,
    pub nodes_skipped: usize,
    pub batches_sent: usize,
}

/// A single similarity search result.
#[derive(Debug, Clone)]
pub struct SimilarityResult {
    pub iri: String,
    pub label: String,
    pub score: f32,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const DEFAULT_ENDPOINT: &str = "http://ruvector-postgres:8080/embed";
const EMBEDDING_DIM: usize = 384;
const DEFAULT_BATCH_SIZE: usize = 64;

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

pub struct EmbeddingService {
    client: Client,
    endpoint_url: String,
    neo4j: Arc<Graph>,
    batch_size: usize,
}

impl EmbeddingService {
    /// Create a new EmbeddingService with the given Neo4j graph handle.
    pub fn new(neo4j: Arc<Graph>) -> Self {
        Self {
            client: Client::new(),
            endpoint_url: DEFAULT_ENDPOINT.to_string(),
            neo4j,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }

    /// Create with a custom endpoint URL.
    pub fn with_endpoint(mut self, url: impl Into<String>) -> Self {
        self.endpoint_url = url.into();
        self
    }

    /// Override the default batch size (64).
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size.max(1);
        self
    }

    // -----------------------------------------------------------------------
    // Core embedding methods
    // -----------------------------------------------------------------------

    /// Embed multiple texts in a single HTTP call. Returns one 384-dim vector per input.
    pub async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Err(EmbeddingError::EmptyInput);
        }

        let request_body = EmbedRequest {
            texts: texts.to_vec(),
        };

        let response = self
            .client
            .post(&self.endpoint_url)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::EndpointError(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let embed_response: EmbedResponse = response.json().await?;

        // Validate dimensions
        for (i, emb) in embed_response.embeddings.iter().enumerate() {
            if emb.len() != EMBEDDING_DIM {
                return Err(EmbeddingError::DimensionMismatch {
                    expected: EMBEDDING_DIM,
                    actual: emb.len(),
                });
            }
            debug!("Embedding {} has norm {:.4}", i, l2_norm(emb));
        }

        if embed_response.embeddings.len() != texts.len() {
            return Err(EmbeddingError::EndpointError(format!(
                "Expected {} embeddings, got {}",
                texts.len(),
                embed_response.embeddings.len()
            )));
        }

        Ok(embed_response.embeddings)
    }

    /// Embed a single text string.
    pub async fn embed_single(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let results = self.embed_texts(&[text.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::EndpointError("Empty response".to_string()))
    }

    // -----------------------------------------------------------------------
    // Neo4j integration: indexing
    // -----------------------------------------------------------------------

    /// Index all OntologyClass nodes by computing and storing their content embeddings.
    ///
    /// For each node, concatenates: `"{preferred_term}: {definition}. {scope_note}"`
    /// and stores the resulting 384-dim vector as `content_embedding_384`.
    pub async fn index_ontology_nodes(&self) -> Result<IndexingStats, EmbeddingError> {
        info!("Starting ontology node embedding indexing");

        // Fetch all OntologyClass nodes with text fields
        let query = neo4rs::query(
            "MATCH (c:OntologyClass) \
             RETURN c.iri AS iri, \
                    c.preferred_term AS preferred_term, \
                    c.definition AS definition, \
                    c.scope_note AS scope_note",
        );

        let mut result = self
            .neo4j
            .execute(query)
            .await
            .map_err(|e| EmbeddingError::Neo4j(format!("Failed to query OntologyClass nodes: {}", e)))?;

        // Collect nodes to embed
        let mut nodes: Vec<(String, String)> = Vec::new(); // (iri, concatenated_text)

        while let Some(row) = result
            .next()
            .await
            .map_err(|e| EmbeddingError::Neo4j(format!("Row iteration error: {}", e)))?
        {
            let iri: String = row
                .get("iri")
                .unwrap_or_default();
            let preferred_term: String = row
                .get("preferred_term")
                .unwrap_or_default();
            let definition: String = row
                .get("definition")
                .unwrap_or_default();
            let scope_note: String = row
                .get("scope_note")
                .unwrap_or_default();

            let text = build_embedding_text(&preferred_term, &definition, &scope_note);
            if text.trim().is_empty() {
                continue;
            }

            nodes.push((iri, text));
        }

        info!("Found {} OntologyClass nodes with text content", nodes.len());

        let total = nodes.len();
        let mut embedded = 0usize;
        let mut skipped = 0usize;
        let mut batches_sent = 0usize;

        // Process in batches
        for chunk in nodes.chunks(self.batch_size) {
            let texts: Vec<String> = chunk.iter().map(|(_, t)| t.clone()).collect();

            let embeddings = match self.embed_texts(&texts).await {
                Ok(e) => e,
                Err(err) => {
                    warn!(
                        "Batch embedding failed (skipping {} nodes): {}",
                        chunk.len(),
                        err
                    );
                    skipped += chunk.len();
                    continue;
                }
            };
            batches_sent += 1;

            // Store each embedding back to Neo4j
            for ((iri, _), embedding) in chunk.iter().zip(embeddings.iter()) {
                if let Err(e) = self.store_embedding(iri, embedding).await {
                    warn!("Failed to store embedding for {}: {}", iri, e);
                    skipped += 1;
                } else {
                    embedded += 1;
                }
            }

            debug!(
                "Batch {}: embedded {}/{} nodes",
                batches_sent,
                embedded,
                total
            );
        }

        let stats = IndexingStats {
            nodes_processed: total,
            nodes_embedded: embedded,
            nodes_skipped: skipped,
            batches_sent,
        };

        info!(
            "Ontology indexing complete: {} processed, {} embedded, {} skipped in {} batches",
            stats.nodes_processed, stats.nodes_embedded, stats.nodes_skipped, stats.batches_sent
        );

        Ok(stats)
    }

    /// Store a single embedding vector on an OntologyClass node.
    async fn store_embedding(&self, iri: &str, embedding: &[f32]) -> Result<(), EmbeddingError> {
        // neo4rs supports Vec<f64> natively for list properties;
        // convert f32 -> f64 for storage compatibility.
        let embedding_f64: Vec<f64> = embedding.iter().map(|&v| v as f64).collect();

        let query = neo4rs::query(
            "MATCH (c:OntologyClass {iri: $iri}) \
             SET c.content_embedding_384 = $embedding",
        )
        .param("iri", iri.to_string())
        .param("embedding", embedding_f64);

        self.neo4j
            .run(query)
            .await
            .map_err(|e| EmbeddingError::Neo4j(format!("Store embedding for {}: {}", iri, e)))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Similarity search
    // -----------------------------------------------------------------------

    /// Perform similarity search: embed the query, then rank all indexed nodes by
    /// cosine similarity. Returns the top-K results.
    pub async fn similarity_search(
        &self,
        query_text: &str,
        top_k: usize,
    ) -> Result<Vec<SimilarityResult>, EmbeddingError> {
        let query_embedding = self.embed_single(query_text).await?;

        let cypher = neo4rs::query(
            "MATCH (c:OntologyClass) \
             WHERE c.content_embedding_384 IS NOT NULL \
             RETURN c.iri AS iri, \
                    c.preferred_term AS label, \
                    c.content_embedding_384 AS embedding",
        );

        let mut result = self
            .neo4j
            .execute(cypher)
            .await
            .map_err(|e| EmbeddingError::Neo4j(format!("Similarity search query failed: {}", e)))?;

        let mut scored: Vec<SimilarityResult> = Vec::new();

        while let Some(row) = result
            .next()
            .await
            .map_err(|e| EmbeddingError::Neo4j(format!("Row iteration error: {}", e)))?
        {
            let iri: String = row.get("iri").unwrap_or_default();
            let label: String = row.get("label").unwrap_or_default();

            // Neo4j stores as f64 list; convert back to f32
            let embedding_f64: Vec<f64> = row.get("embedding").unwrap_or_default();
            let node_embedding: Vec<f32> = embedding_f64.iter().map(|&v| v as f32).collect();

            if node_embedding.len() != EMBEDDING_DIM {
                continue; // Skip malformed embeddings
            }

            let score = cosine_similarity(&query_embedding, &node_embedding);
            scored.push(SimilarityResult { iri, label, score });
        }

        // Sort descending by score
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        Ok(scored)
    }

    /// Combined similarity: blends content embedding similarity with topology embedding
    /// similarity (if available as `topology_embedding_384` property).
    ///
    /// `topology_weight` controls interpolation: 0.0 = pure content, 1.0 = pure topology.
    pub async fn combined_similarity(
        &self,
        query_text: &str,
        topology_weight: f32,
        top_k: usize,
    ) -> Result<Vec<SimilarityResult>, EmbeddingError> {
        let topology_weight = topology_weight.clamp(0.0, 1.0);
        let content_weight = 1.0 - topology_weight;

        let query_embedding = self.embed_single(query_text).await?;

        let cypher = neo4rs::query(
            "MATCH (c:OntologyClass) \
             WHERE c.content_embedding_384 IS NOT NULL \
             RETURN c.iri AS iri, \
                    c.preferred_term AS label, \
                    c.content_embedding_384 AS embedding, \
                    c.topology_embedding_384 AS topo_embedding",
        );

        let mut result = self
            .neo4j
            .execute(cypher)
            .await
            .map_err(|e| EmbeddingError::Neo4j(format!("Combined search query failed: {}", e)))?;

        let mut scored: Vec<SimilarityResult> = Vec::new();

        while let Some(row) = result
            .next()
            .await
            .map_err(|e| EmbeddingError::Neo4j(format!("Row iteration error: {}", e)))?
        {
            let iri: String = row.get("iri").unwrap_or_default();
            let label: String = row.get("label").unwrap_or_default();

            // Content embedding (required)
            let content_f64: Vec<f64> = row.get("embedding").unwrap_or_default();
            let content_emb: Vec<f32> = content_f64.iter().map(|&v| v as f32).collect();

            if content_emb.len() != EMBEDDING_DIM {
                continue;
            }

            let content_score = cosine_similarity(&query_embedding, &content_emb);

            // Topology embedding (optional)
            let topo_f64: Vec<f64> = row.get("topo_embedding").unwrap_or_default();
            let topo_emb: Vec<f32> = topo_f64.iter().map(|&v| v as f32).collect();

            let topo_score = if topo_emb.len() == EMBEDDING_DIM {
                cosine_similarity(&query_embedding, &topo_emb)
            } else {
                content_score // Fallback: use content score if no topology embedding
            };

            let combined_score = content_weight * content_score + topology_weight * topo_score;
            scored.push(SimilarityResult {
                iri,
                label,
                score: combined_score,
            });
        }

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        Ok(scored)
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Build the concatenated text for embedding from ontology node fields.
fn build_embedding_text(preferred_term: &str, definition: &str, scope_note: &str) -> String {
    let mut parts = Vec::with_capacity(3);

    let term = preferred_term.trim();
    let def = definition.trim();
    let note = scope_note.trim();

    if !term.is_empty() {
        if !def.is_empty() {
            parts.push(format!("{}: {}", term, def));
        } else {
            parts.push(term.to_string());
        }
    } else if !def.is_empty() {
        parts.push(def.to_string());
    }

    if !note.is_empty() {
        parts.push(note.to_string());
    }

    parts.join(". ")
}

/// Compute cosine similarity between two vectors.
/// Returns 0.0 if either vector has zero norm.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have equal length");

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < f32::EPSILON {
        return 0.0;
    }

    dot / denom
}

/// Compute L2 norm of a vector.
fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_scaled() {
        let a = vec![3.0, 4.0];
        let b = vec![6.0, 8.0]; // same direction, different magnitude
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_384_dim() {
        // Simulate two normalized 384-dim vectors
        let mut a = vec![0.0f32; EMBEDDING_DIM];
        let mut b = vec![0.0f32; EMBEDDING_DIM];

        // Set up partially overlapping directions
        for i in 0..192 {
            a[i] = 1.0;
            b[i] = 1.0;
        }
        for i in 192..384 {
            a[i] = 1.0;
            b[i] = -1.0;
        }

        let sim = cosine_similarity(&a, &b);
        // Expected: dot = 192 - 192 = 0, norms = sqrt(384) each
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_build_embedding_text_full() {
        let text = build_embedding_text("Enzyme", "A protein catalyst", "Used in metabolism");
        assert_eq!(text, "Enzyme: A protein catalyst. Used in metabolism");
    }

    #[test]
    fn test_build_embedding_text_no_scope_note() {
        let text = build_embedding_text("Enzyme", "A protein catalyst", "");
        assert_eq!(text, "Enzyme: A protein catalyst");
    }

    #[test]
    fn test_build_embedding_text_no_definition() {
        let text = build_embedding_text("Enzyme", "", "Used in metabolism");
        assert_eq!(text, "Enzyme. Used in metabolism");
    }

    #[test]
    fn test_build_embedding_text_only_term() {
        let text = build_embedding_text("Enzyme", "", "");
        assert_eq!(text, "Enzyme");
    }

    #[test]
    fn test_build_embedding_text_empty() {
        let text = build_embedding_text("", "", "");
        assert_eq!(text, "");
    }

    #[test]
    fn test_build_embedding_text_only_definition() {
        let text = build_embedding_text("", "A protein catalyst", "");
        assert_eq!(text, "A protein catalyst");
    }

    #[test]
    fn test_build_embedding_text_whitespace_trimming() {
        let text = build_embedding_text("  Enzyme  ", "  A protein  ", "  Note  ");
        assert_eq!(text, "Enzyme: A protein. Note");
    }

    #[test]
    fn test_l2_norm() {
        let v = vec![3.0, 4.0];
        assert!((l2_norm(&v) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_norm_unit_vector() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((l2_norm(&v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_similarity_result_ordering() {
        // Verify that our sort logic produces descending order
        let mut results = vec![
            SimilarityResult {
                iri: "a".to_string(),
                label: "A".to_string(),
                score: 0.5,
            },
            SimilarityResult {
                iri: "b".to_string(),
                label: "B".to_string(),
                score: 0.9,
            },
            SimilarityResult {
                iri: "c".to_string(),
                label: "C".to_string(),
                score: 0.1,
            },
        ];

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].iri, "b");
        assert_eq!(results[1].iri, "a");
    }

    #[test]
    fn test_batch_chunking_logic() {
        // Verify chunk behavior for batching
        let items: Vec<usize> = (0..150).collect();
        let batch_size = 64;
        let chunks: Vec<&[usize]> = items.chunks(batch_size).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 64);
        assert_eq!(chunks[1].len(), 64);
        assert_eq!(chunks[2].len(), 22);
    }

    #[test]
    fn test_embedding_dim_constant() {
        assert_eq!(EMBEDDING_DIM, 384);
    }
}
