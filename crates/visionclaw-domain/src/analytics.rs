//! Analytics data types shared between actors and HTTP handlers.
//!
//! These types were originally in `webxr::handlers::api_handler::analytics::types`,
//! which made actors depend on the HTTP layer (dependency-inversion smell).
//! Moved to domain in ADR-090 A6.4 — pure data with no infrastructure deps.

use serde::{Deserialize, Serialize};

/// Parameters accepted by a clustering request — covers k-means, DBSCAN,
/// hierarchical, Louvain, affinity-prop, and graph-spectral variants. Every
/// field is `Option<T>` so the same struct can carry whichever algorithm's
/// configuration the caller selected.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClusteringParams {
    pub num_clusters: Option<u32>,
    pub min_cluster_size: Option<u32>,
    pub similarity: Option<String>,
    pub convergence_threshold: Option<f32>,
    pub max_iterations: Option<u32>,
    pub eps: Option<f32>,
    pub min_samples: Option<u32>,
    pub distance_threshold: Option<f32>,
    pub linkage: Option<String>,
    pub resolution: Option<f32>,
    pub random_state: Option<u32>,
    pub damping: Option<f32>,
    pub preference: Option<f32>,
    pub tolerance: Option<f64>,
    pub seed: Option<u64>,
    pub sigma: Option<f64>,
    pub min_modularity_gain: Option<f64>,
}

/// A single cluster produced by community detection / clustering algorithms.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    pub id: String,
    pub label: String,
    pub node_count: u32,
    pub coherence: f32,
    pub color: String,
    pub keywords: Vec<String>,
    pub nodes: Vec<u32>,
    pub centroid: Option<[f32; 3]>,
}
