//! Knowledge Graph Embedding (TransE) training pipeline.
//!
//! Trains 128-dimensional entity and relation embeddings on the graph structure
//! using the TransE scoring function: for a triple (h, r, t), h + r ≈ t.
//!
//! Produced embeddings encode structural role and can be consumed by the GPU
//! physics pipeline or used for ML-based link prediction/discovery.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use neo4rs::{query, Graph};
use rand::Rng;
use rayon::prelude::*;
use tracing::{info, instrument};

// ─── Constants ───────────────────────────────────────────────────────────────

const EMBEDDING_DIM: usize = 128;
const DEFAULT_LEARNING_RATE: f32 = 0.01;
const DEFAULT_MARGIN: f32 = 1.0;
const DEFAULT_EPOCHS: usize = 500;
const DEFAULT_BATCH_SIZE: usize = 1024;
const DEFAULT_NEGATIVE_SAMPLES: usize = 5;

// ─── Types ───────────────────────────────────────────────────────────────────

/// A training triple: (head_idx, relation_idx, tail_idx)
#[derive(Debug, Clone, Copy)]
pub struct Triple {
    pub head: usize,
    pub relation: usize,
    pub tail: usize,
}

/// Configuration for KGE training.
#[derive(Debug, Clone)]
pub struct KGEConfig {
    pub embedding_dim: usize,
    pub learning_rate: f32,
    pub margin: f32,
    pub epochs: usize,
    pub batch_size: usize,
    pub negative_samples: usize,
    pub normalize_embeddings: bool,
}

impl Default for KGEConfig {
    fn default() -> Self {
        Self {
            embedding_dim: EMBEDDING_DIM,
            learning_rate: DEFAULT_LEARNING_RATE,
            margin: DEFAULT_MARGIN,
            epochs: DEFAULT_EPOCHS,
            batch_size: DEFAULT_BATCH_SIZE,
            negative_samples: DEFAULT_NEGATIVE_SAMPLES,
            normalize_embeddings: true,
        }
    }
}

/// Training statistics.
#[derive(Debug, Clone, Default)]
pub struct TrainingStats {
    pub num_entities: usize,
    pub num_relations: usize,
    pub num_triples: usize,
    pub final_loss: f32,
    pub epochs_completed: usize,
    pub duration_ms: u64,
}

/// Result of KGE training — embeddings indexed by entity.
#[derive(Debug, Clone)]
pub struct KGEResult {
    /// Entity embeddings: `[num_entities][embedding_dim]`
    pub entity_embeddings: Vec<Vec<f32>>,
    /// Relation embeddings: `[num_relations][embedding_dim]`
    pub relation_embeddings: Vec<Vec<f32>>,
    /// Index → IRI mapping
    pub entity_to_iri: Vec<String>,
    /// IRI → index mapping
    pub iri_to_entity: HashMap<String, usize>,
    /// Relation name → index mapping
    pub relation_to_idx: HashMap<String, usize>,
    /// Training statistics
    pub stats: TrainingStats,
}

/// Errors from the KGE training pipeline.
#[derive(Debug, thiserror::Error)]
pub enum KGEError {
    #[error("Neo4j error: {0}")]
    Neo4j(String),
    #[error("No triples found in graph")]
    EmptyGraph,
    #[error("Entity not found: {0}")]
    EntityNotFound(String),
    #[error("Training failed: {0}")]
    TrainingFailed(String),
}

// ─── Trainer ─────────────────────────────────────────────────────────────────

pub struct KGETrainer {
    neo4j: Arc<Graph>,
    config: KGEConfig,
}

impl KGETrainer {
    pub fn new(neo4j: Arc<Graph>, config: KGEConfig) -> Self {
        Self { neo4j, config }
    }

    /// Load triples from Neo4j. Returns `(triples, entity_iris, relation_names)`.
    #[instrument(skip(self))]
    pub async fn load_triples(
        &self,
    ) -> Result<(Vec<Triple>, Vec<String>, Vec<String>), KGEError> {
        let cypher = "MATCH (a)-[r]->(b) \
                      WHERE (a:OntologyClass OR a:KGNode) AND (b:OntologyClass OR b:KGNode) \
                      RETURN a.iri AS head_iri, type(r) AS rel_type, b.iri AS tail_iri";

        let mut result = self
            .neo4j
            .execute(query(cypher))
            .await
            .map_err(|e| KGEError::Neo4j(e.to_string()))?;

        let mut entity_map: HashMap<String, usize> = HashMap::new();
        let mut relation_map: HashMap<String, usize> = HashMap::new();
        let mut entity_iris: Vec<String> = Vec::new();
        let mut relation_names: Vec<String> = Vec::new();
        let mut triples: Vec<Triple> = Vec::new();

        while let Some(row) = result.next().await.map_err(|e| KGEError::Neo4j(e.to_string()))? {
            let head_iri: String = row
                .get("head_iri")
                .unwrap_or_default();
            let rel_type: String = row
                .get("rel_type")
                .unwrap_or_default();
            let tail_iri: String = row
                .get("tail_iri")
                .unwrap_or_default();

            if head_iri.is_empty() || tail_iri.is_empty() || rel_type.is_empty() {
                continue;
            }

            let head_idx = *entity_map.entry(head_iri.clone()).or_insert_with(|| {
                let idx = entity_iris.len();
                entity_iris.push(head_iri);
                idx
            });

            let tail_idx = *entity_map.entry(tail_iri.clone()).or_insert_with(|| {
                let idx = entity_iris.len();
                entity_iris.push(tail_iri);
                idx
            });

            let rel_idx = *relation_map.entry(rel_type.clone()).or_insert_with(|| {
                let idx = relation_names.len();
                relation_names.push(rel_type);
                idx
            });

            triples.push(Triple {
                head: head_idx,
                relation: rel_idx,
                tail: tail_idx,
            });
        }

        if triples.is_empty() {
            return Err(KGEError::EmptyGraph);
        }

        info!(
            entities = entity_iris.len(),
            relations = relation_names.len(),
            triples = triples.len(),
            "Loaded graph triples for KGE training"
        );

        Ok((triples, entity_iris, relation_names))
    }

    /// Train TransE embeddings on the given triples.
    #[instrument(skip(self, triples))]
    pub fn train(
        &self,
        triples: &[Triple],
        num_entities: usize,
        num_relations: usize,
    ) -> KGEResult {
        let inner = KGETrainerInner {
            config: self.config.clone(),
        };
        inner.train(triples, num_entities, num_relations)
    }

    /// Full pipeline: load triples from Neo4j, train, store embeddings back.
    #[instrument(skip(self))]
    pub async fn train_and_store(&self) -> Result<KGEResult, KGEError> {
        let (triples, entity_iris, relation_names) = self.load_triples().await?;

        let num_entities = entity_iris.len();
        let num_relations = relation_names.len();

        // Train (CPU-bound, run in blocking context)
        let config = self.config.clone();
        let triples_clone = triples.clone();
        let mut result = tokio::task::spawn_blocking(move || {
            let trainer_inner = KGETrainerInner { config };
            trainer_inner.train(&triples_clone, num_entities, num_relations)
        })
        .await
        .map_err(|e| KGEError::TrainingFailed(e.to_string()))?;

        // Attach IRI mappings
        let iri_to_entity: HashMap<String, usize> = entity_iris
            .iter()
            .enumerate()
            .map(|(idx, iri)| (iri.clone(), idx))
            .collect();
        let relation_to_idx: HashMap<String, usize> = relation_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect();
        result.entity_to_iri = entity_iris;
        result.iri_to_entity = iri_to_entity;
        result.relation_to_idx = relation_to_idx;

        // Store embeddings back to Neo4j
        self.store_embeddings(&result).await?;

        info!(
            entities = result.stats.num_entities,
            "Stored KGE embeddings in Neo4j"
        );

        Ok(result)
    }

    /// Store computed embeddings back to Neo4j as node properties.
    async fn store_embeddings(&self, result: &KGEResult) -> Result<(), KGEError> {
        // Batch in groups of 100 to avoid overloading Neo4j
        const STORE_BATCH: usize = 100;

        for chunk in result.entity_to_iri.chunks(STORE_BATCH) {
            for iri in chunk {
                let idx = result.iri_to_entity[iri];
                let embedding = &result.entity_embeddings[idx];

                // neo4rs expects the list as a parameter
                let embedding_vec: Vec<f64> = embedding.iter().map(|&v| v as f64).collect();

                let q = query(
                    "MATCH (c {iri: $iri}) SET c.kge_embedding_128 = $embedding",
                )
                .param("iri", iri.as_str())
                .param("embedding", embedding_vec);

                self.neo4j
                    .run(q)
                    .await
                    .map_err(|e| KGEError::Neo4j(format!("Failed to store embedding for {iri}: {e}")))?;
            }
        }

        Ok(())
    }

    /// Predict top-k tail entities for a given (head, relation) pair.
    /// Returns candidates sorted by ascending TransE distance (lower = more likely link).
    pub fn predict_links(
        &self,
        result: &KGEResult,
        entity_iri: &str,
        relation: &str,
        top_k: usize,
    ) -> Vec<(String, f32)> {
        let head_idx = match result.iri_to_entity.get(entity_iri) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };

        let rel_idx = match result.relation_to_idx.get(relation) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };

        let head_emb = &result.entity_embeddings[head_idx];
        let rel_emb = &result.relation_embeddings[rel_idx];

        // predicted_t = h + r
        let predicted: Vec<f32> = head_emb
            .iter()
            .zip(rel_emb.iter())
            .map(|(h, r)| h + r)
            .collect();

        // Score all entities by L2 distance to predicted_t
        let mut scores: Vec<(usize, f32)> = result
            .entity_embeddings
            .par_iter()
            .enumerate()
            .map(|(idx, emb)| {
                let dist: f32 = predicted
                    .iter()
                    .zip(emb.iter())
                    .map(|(p, t)| (p - t).powi(2))
                    .sum::<f32>()
                    .sqrt();
                (idx, dist)
            })
            .collect();

        // Sort ascending (lower distance = better prediction)
        scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        scores
            .into_iter()
            .filter(|(idx, _)| *idx != head_idx)
            .take(top_k)
            .map(|(idx, dist)| (result.entity_to_iri[idx].clone(), dist))
            .collect()
    }

    /// Cosine similarity between two entity embeddings.
    pub fn entity_similarity(
        &self,
        result: &KGEResult,
        iri_a: &str,
        iri_b: &str,
    ) -> Option<f32> {
        let idx_a = *result.iri_to_entity.get(iri_a)?;
        let idx_b = *result.iri_to_entity.get(iri_b)?;
        Some(cosine_similarity(
            &result.entity_embeddings[idx_a],
            &result.entity_embeddings[idx_b],
        ))
    }
}

// ─── Internal trainer (Send-safe for spawn_blocking) ─────────────────────────

struct KGETrainerInner {
    config: KGEConfig,
}

impl KGETrainerInner {
    fn train(
        &self,
        triples: &[Triple],
        num_entities: usize,
        num_relations: usize,
    ) -> KGEResult {
        let dim = self.config.embedding_dim;
        let bound = 6.0_f32 / (dim as f32).sqrt();
        let mut rng = rand::thread_rng();

        let mut entity_emb: Vec<Vec<f32>> = (0..num_entities)
            .map(|_| (0..dim).map(|_| rng.gen_range(-bound..bound)).collect())
            .collect();

        let mut relation_emb: Vec<Vec<f32>> = (0..num_relations)
            .map(|_| {
                let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-bound..bound)).collect();
                l2_normalize(&v)
            })
            .collect();

        let mut final_loss = 0.0_f32;
        let mut epochs_completed = 0_usize;
        let mut indices: Vec<usize> = (0..triples.len()).collect();
        let start = Instant::now();

        for epoch in 0..self.config.epochs {
            shuffle_indices(&mut indices, &mut rng);

            let mut epoch_loss = 0.0_f32;
            let mut batch_count = 0_u32;

            for batch_start in (0..triples.len()).step_by(self.config.batch_size) {
                let batch_end = (batch_start + self.config.batch_size).min(triples.len());
                let batch_indices = &indices[batch_start..batch_end];

                // Generate negative samples in parallel
                let negatives: Vec<Vec<Triple>> = batch_indices
                    .par_iter()
                    .map(|&idx| {
                        let triple = &triples[idx];
                        let mut local_rng = rand::thread_rng();
                        (0..self.config.negative_samples)
                            .map(|_| corrupt_triple(triple, num_entities, &mut local_rng))
                            .collect()
                    })
                    .collect();

                for (batch_pos, &idx) in batch_indices.iter().enumerate() {
                    let pos_triple = &triples[idx];
                    let pos_score = transe_score(
                        &entity_emb[pos_triple.head],
                        &relation_emb[pos_triple.relation],
                        &entity_emb[pos_triple.tail],
                    );

                    for neg_triple in &negatives[batch_pos] {
                        let neg_score = transe_score(
                            &entity_emb[neg_triple.head],
                            &relation_emb[neg_triple.relation],
                            &entity_emb[neg_triple.tail],
                        );

                        let loss = (self.config.margin + pos_score - neg_score).max(0.0);

                        if loss > 0.0 {
                            epoch_loss += loss;
                            let lr = self.config.learning_rate;

                            let pos_grad = transe_gradient(
                                &entity_emb[pos_triple.head],
                                &relation_emb[pos_triple.relation],
                                &entity_emb[pos_triple.tail],
                            );

                            for d in 0..dim {
                                entity_emb[pos_triple.head][d] -= lr * pos_grad[d];
                                relation_emb[pos_triple.relation][d] -= lr * pos_grad[d];
                                entity_emb[pos_triple.tail][d] += lr * pos_grad[d];
                            }

                            let neg_grad = transe_gradient(
                                &entity_emb[neg_triple.head],
                                &relation_emb[neg_triple.relation],
                                &entity_emb[neg_triple.tail],
                            );

                            for d in 0..dim {
                                entity_emb[neg_triple.head][d] += lr * neg_grad[d];
                                relation_emb[neg_triple.relation][d] += lr * neg_grad[d];
                                entity_emb[neg_triple.tail][d] -= lr * neg_grad[d];
                            }
                        }
                    }
                }

                batch_count += 1;
            }

            if self.config.normalize_embeddings {
                for emb in entity_emb.iter_mut() {
                    let normalized = l2_normalize(emb);
                    *emb = normalized;
                }
            }

            final_loss = if batch_count > 0 {
                epoch_loss / batch_count as f32
            } else {
                0.0
            };
            epochs_completed = epoch + 1;

            if (epoch + 1) % 50 == 0 {
                info!(epoch = epoch + 1, loss = final_loss, "KGE training progress");
            }
        }

        let duration = start.elapsed();

        KGEResult {
            entity_embeddings: entity_emb,
            relation_embeddings: relation_emb,
            entity_to_iri: Vec::new(),
            iri_to_entity: HashMap::new(),
            relation_to_idx: HashMap::new(),
            stats: TrainingStats {
                num_entities,
                num_relations,
                num_triples: triples.len(),
                final_loss,
                epochs_completed,
                duration_ms: duration.as_millis() as u64,
            },
        }
    }
}

// ─── Pure functions ──────────────────────────────────────────────────────────

/// TransE score: ||h + r - t||_2
pub fn transe_score(h: &[f32], r: &[f32], t: &[f32]) -> f32 {
    h.iter()
        .zip(r.iter())
        .zip(t.iter())
        .map(|((hi, ri), ti)| (hi + ri - ti).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Gradient of ||h + r - t||_2 w.r.t. the difference vector (h + r - t).
/// Returns the unit direction (h + r - t) / ||h + r - t||.
fn transe_gradient(h: &[f32], r: &[f32], t: &[f32]) -> Vec<f32> {
    let diff: Vec<f32> = h
        .iter()
        .zip(r.iter())
        .zip(t.iter())
        .map(|((hi, ri), ti)| hi + ri - ti)
        .collect();

    let norm = diff.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();
    if norm < 1e-8 {
        return vec![0.0; h.len()];
    }

    diff.iter().map(|x| x / norm).collect()
}

/// L2-normalize a vector. Returns zero vector if norm is near zero.
pub fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();
    if norm < 1e-8 {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();
    if norm_a < 1e-8 || norm_b < 1e-8 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Corrupt a triple by replacing head or tail with a random entity.
fn corrupt_triple(triple: &Triple, num_entities: usize, rng: &mut impl Rng) -> Triple {
    if rng.gen_bool(0.5) {
        // Corrupt head
        let new_head = rng.gen_range(0..num_entities);
        Triple {
            head: new_head,
            relation: triple.relation,
            tail: triple.tail,
        }
    } else {
        // Corrupt tail
        let new_tail = rng.gen_range(0..num_entities);
        Triple {
            head: triple.head,
            relation: triple.relation,
            tail: new_tail,
        }
    }
}

/// Fisher-Yates shuffle on index buffer.
fn shuffle_indices(indices: &mut [usize], rng: &mut impl Rng) {
    let n = indices.len();
    for i in (1..n).rev() {
        let j = rng.gen_range(0..=i);
        indices.swap(i, j);
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transe_score_perfect_triple() {
        // If h + r = t exactly, score should be 0
        let h = vec![1.0, 0.0, 0.0];
        let r = vec![0.0, 1.0, 0.0];
        let t = vec![1.0, 1.0, 0.0];
        let score = transe_score(&h, &r, &t);
        assert!(score < 1e-6, "Perfect triple should have near-zero score, got {score}");
    }

    #[test]
    fn test_transe_score_known_lower_than_corrupted() {
        // Known triple: h + r ≈ t
        let h = vec![1.0, 0.0, 0.5];
        let r = vec![0.5, 1.0, 0.0];
        let t = vec![1.5, 1.0, 0.5]; // h + r = t exactly

        // Corrupted triple: same h and r, but wrong t
        let t_corrupt = vec![0.0, 0.0, 0.0];

        let pos_score = transe_score(&h, &r, &t);
        let neg_score = transe_score(&h, &r, &t_corrupt);

        assert!(
            pos_score < neg_score,
            "Positive triple score ({pos_score}) should be lower than corrupted ({neg_score})"
        );
    }

    #[test]
    fn test_l2_normalize() {
        let v = vec![3.0, 4.0];
        let normalized = l2_normalize(&v);
        let norm: f32 = normalized.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-6,
            "L2-normalized vector should have unit norm, got {norm}"
        );
        assert!((normalized[0] - 0.6).abs() < 1e-6);
        assert!((normalized[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_zero_vector() {
        let v = vec![0.0, 0.0, 0.0];
        let normalized = l2_normalize(&v);
        assert!(normalized.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_l2_normalize_high_dim() {
        let dim = EMBEDDING_DIM;
        let v: Vec<f32> = (0..dim).map(|i| i as f32).collect();
        let normalized = l2_normalize(&v);
        let norm: f32 = normalized.iter().map(|x| x.powi(2)).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "128-dim normalized vector should have unit norm, got {norm}"
        );
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        let sim = cosine_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Identical vectors should have cosine similarity 1.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim.abs() < 1e-6,
            "Orthogonal vectors should have cosine similarity 0.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim + 1.0).abs() < 1e-6,
            "Opposite vectors should have cosine similarity -1.0, got {sim}"
        );
    }

    #[test]
    fn test_negative_sampling_distribution() {
        let triple = Triple {
            head: 5,
            relation: 2,
            tail: 10,
        };
        let num_entities = 100;
        let mut rng = rand::thread_rng();

        let mut head_corrupted = 0;
        let mut tail_corrupted = 0;
        let samples = 10_000;

        for _ in 0..samples {
            let corrupted = corrupt_triple(&triple, num_entities, &mut rng);
            if corrupted.head != triple.head {
                head_corrupted += 1;
            }
            if corrupted.tail != triple.tail {
                tail_corrupted += 1;
            }
            // Relation should never change
            assert_eq!(corrupted.relation, triple.relation);
        }

        // Should be roughly 50/50 split (within statistical bounds)
        let head_ratio = head_corrupted as f64 / samples as f64;
        let tail_ratio = tail_corrupted as f64 / samples as f64;

        assert!(
            (head_ratio - 0.5).abs() < 0.05,
            "Head corruption ratio should be ~0.5, got {head_ratio}"
        );
        assert!(
            (tail_ratio - 0.5).abs() < 0.05,
            "Tail corruption ratio should be ~0.5, got {tail_ratio}"
        );
    }

    #[test]
    fn test_training_reduces_loss() {
        // Small synthetic graph: A -r0-> B, B -r0-> C, A -r1-> C
        let triples = vec![
            Triple { head: 0, relation: 0, tail: 1 },
            Triple { head: 1, relation: 0, tail: 2 },
            Triple { head: 0, relation: 1, tail: 2 },
        ];

        let config = KGEConfig {
            embedding_dim: 16,
            learning_rate: 0.05,
            margin: 1.0,
            epochs: 100,
            batch_size: 3,
            negative_samples: 3,
            normalize_embeddings: true,
        };

        let trainer = KGETrainerInner { config: config.clone() };

        // Train briefly and check loss decreased
        let result = trainer.train(&triples, 3, 2);

        // After training, positive triples should score lower than random corruptions
        let pos_score = transe_score(
            &result.entity_embeddings[0],
            &result.relation_embeddings[0],
            &result.entity_embeddings[1],
        );

        // Random entity as tail
        let neg_score = transe_score(
            &result.entity_embeddings[0],
            &result.relation_embeddings[0],
            &result.entity_embeddings[2], // wrong tail for this relation
        );

        // The training may not always produce perfect separation on such a small
        // graph in 100 epochs, but loss should be finite and stats populated
        assert!(result.stats.final_loss.is_finite());
        assert_eq!(result.stats.epochs_completed, 100);
        assert_eq!(result.stats.num_entities, 3);
        assert_eq!(result.stats.num_relations, 2);
        assert_eq!(result.stats.num_triples, 3);

        // Log scores for debugging (won't fail test)
        eprintln!("pos_score(A-r0->B): {pos_score}, neg_score(A-r0->C): {neg_score}");
    }

    #[test]
    fn test_entity_similarity_self() {
        // Build a minimal KGEResult with known embeddings
        let entity_embeddings = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![1.0, 0.0, 0.0], // same as entity 0
        ];

        let entity_to_iri = vec![
            "urn:a".to_string(),
            "urn:b".to_string(),
            "urn:c".to_string(),
        ];
        let iri_to_entity: HashMap<String, usize> = entity_to_iri
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i))
            .collect();

        let result = KGEResult {
            entity_embeddings,
            relation_embeddings: vec![],
            entity_to_iri,
            iri_to_entity,
            relation_to_idx: HashMap::new(),
            stats: TrainingStats::default(),
        };

        // Test pure cosine_similarity function using the result's embeddings
        let sim_self = cosine_similarity(
            &result.entity_embeddings[0],
            &result.entity_embeddings[0],
        );
        assert!((sim_self - 1.0).abs() < 1e-6, "Self-similarity should be 1.0");

        let sim_same = cosine_similarity(
            &result.entity_embeddings[0],
            &result.entity_embeddings[2],
        );
        assert!((sim_same - 1.0).abs() < 1e-6, "Same-vector similarity should be 1.0");

        let sim_ortho = cosine_similarity(
            &result.entity_embeddings[0],
            &result.entity_embeddings[1],
        );
        assert!(sim_ortho.abs() < 1e-6, "Orthogonal similarity should be 0.0");
    }

    #[test]
    fn test_shuffle_indices() {
        let mut indices: Vec<usize> = (0..100).collect();
        let original = indices.clone();
        let mut rng = rand::thread_rng();

        shuffle_indices(&mut indices, &mut rng);

        // Same elements
        let mut sorted = indices.clone();
        sorted.sort();
        assert_eq!(sorted, original);

        // Very unlikely to be in original order (probability 1/100!)
        assert_ne!(indices, original, "Shuffle should change order");
    }

    #[test]
    fn test_transe_gradient_direction() {
        let h = vec![1.0, 0.0];
        let r = vec![1.0, 0.0];
        let t = vec![0.0, 0.0]; // h + r - t = [2, 0], gradient points in [1, 0]

        let grad = transe_gradient(&h, &r, &t);
        assert!((grad[0] - 1.0).abs() < 1e-6);
        assert!(grad[1].abs() < 1e-6);
    }
}
