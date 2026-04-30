//! Clustering Actor - Handles K-means clustering and community detection algorithms

use actix::prelude::*;
use log::{error, info, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use super::shared::{GPUState, SharedGPUContext};
use crate::actors::client_coordinator_actor::ClientCoordinatorActor;
use crate::actors::messages::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusteringStats {
    pub total_clusters: usize,
    pub average_cluster_size: f32,
    pub largest_cluster_size: usize,
    pub smallest_cluster_size: usize,
    pub silhouette_score: f32,
    pub computation_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityDetectionStats {
    pub total_communities: usize,
    pub modularity: f32,
    pub average_community_size: f32,
    pub largest_community: usize,
    pub smallest_community: usize,
    pub computation_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Community {
    pub id: String,
    pub nodes: Vec<u32>,
    pub internal_edges: usize,
    pub external_edges: usize,
    pub density: f32,
}

/// Type alias for the shared node analytics map: node_id -> (cluster_id, anomaly_score, community_id)
type NodeAnalyticsMap = Arc<std::sync::RwLock<std::collections::HashMap<u32, (u32, f32, u32)>>>;

pub struct ClusteringActor {

    gpu_state: GPUState,


    shared_context: Option<Arc<SharedGPUContext>>,

    /// Maps GPU buffer index -> actual graph node ID.
    /// Populated lazily from the GPU `node_graph_id` buffer before clustering.
    /// When empty, raw buffer indices are used as-is (backward compat).
    node_id_map: Vec<u32>,

    /// Shared analytics store — populated after clustering. Per ADR-061 the
    /// binary frame no longer carries analytics columns; this map is now an
    /// in-process side table for REST API consumers. Wire-side delivery to
    /// clients goes via `BroadcastAnalyticsUpdate` on the
    /// `ClientCoordinatorActor`.
    node_analytics: Option<NodeAnalyticsMap>,

    /// PRD-007 §B1 / ADR-061 §D2: address of the `ClientCoordinatorActor`
    /// for emitting `BroadcastAnalyticsUpdate` on kernel completion.
    client_coordinator_addr: Option<Addr<ClientCoordinatorActor>>,

    /// Monotonic generation counter — increments on every recompute; merged
    /// by the client side-table for ordering.
    analytics_generation: Arc<AtomicU64>,
}

impl ClusteringActor {
    pub fn new() -> Self {
        Self {
            gpu_state: GPUState::default(),
            shared_context: None,
            node_id_map: Vec::new(),
            node_analytics: None,
            client_coordinator_addr: None,
            analytics_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Download the buffer_index -> graph_node_id mapping from the GPU
    /// `node_graph_id` DeviceBuffer. Caches the result in `self.node_id_map`.
    /// Returns an empty Vec if the GPU context is unavailable.
    fn ensure_node_id_map(&mut self) {
        if !self.node_id_map.is_empty() {
            return;
        }
        if let Some(ref ctx) = self.shared_context {
            if let Ok(uc) = ctx.unified_compute.lock() {
                let n = uc.num_nodes;
                if n > 0 {
                    // Use allocated_nodes for copy_to (device buffer may be overallocated)
                    let alloc_n = uc.node_graph_id.len();
                    let mut ids = vec![0i32; alloc_n];
                    use cust::memory::CopyDestination;
                    if uc.node_graph_id.copy_to(&mut ids).is_ok() {
                        ids.truncate(n);
                        // Check whether the buffer was actually populated
                        // (all zeros means it was never uploaded).
                        let has_real_ids = ids.iter().any(|&id| id != 0);
                        if has_real_ids {
                            self.node_id_map = ids.iter().map(|&id| id as u32).collect();
                            info!(
                                "ClusteringActor: Downloaded node_id_map ({} entries) from GPU",
                                self.node_id_map.len()
                            );
                        }
                    }
                }
                // Also fix gpu_state.num_nodes if it was never set
                if self.gpu_state.num_nodes == 0 && n > 0 {
                    self.gpu_state.num_nodes = n as u32;
                }
            }
        }
    }

    /// Translate a GPU buffer index to the actual graph node ID.
    /// Falls back to the raw index if no mapping is available.
    #[inline]
    fn translate_gpu_index(&self, gpu_index: usize) -> u32 {
        if gpu_index < self.node_id_map.len() {
            self.node_id_map[gpu_index]
        } else {
            gpu_index as u32
        }
    }

    fn generate_cluster_keywords(nodes: &[u32]) -> Vec<String> {
        if nodes.is_empty() {
            return vec!["empty".to_string()];
        }

        
        let mut keywords = Vec::new();
        match nodes.len() {
            1 => keywords.push("singleton".to_string()),
            2..=5 => keywords.push("small".to_string()),
            6..=20 => keywords.push("medium".to_string()),
            _ => keywords.push("large".to_string()),
        }

        
        keywords.push(format!("cluster-{}", nodes[0] % 10));
        keywords
    }

    
    async fn perform_kmeans_clustering(
        &mut self,
        params: KMeansParams,
    ) -> Result<KMeansResult, String> {
        info!(
            "ClusteringActor: Starting K-means clustering with {} clusters",
            params.num_clusters
        );

        let unified_compute_arc = match &self.shared_context {
            Some(ctx) => Arc::clone(&ctx.unified_compute),
            None => {
                return Err("GPU context not initialized".to_string());
            }
        };

        let num_clusters = params.num_clusters;
        let max_iterations = params.max_iterations.unwrap_or(100);
        let tolerance = params.tolerance.unwrap_or(0.001);
        let seed = params.seed.unwrap_or(42);

        // Move blocking GPU operations to dedicated blocking thread pool
        // This prevents std::sync::Mutex::lock() from blocking Tokio worker threads
        let blocking_result = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut unified_compute = match unified_compute_arc.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("ClusteringActor: GPU mutex was poisoned, recovering");
                    poisoned.into_inner()
                }
            };

            let start_time = Instant::now();

            let gpu_result = unified_compute
                .run_kmeans_clustering_with_metrics(
                    num_clusters,
                    max_iterations,
                    tolerance,
                    seed,
                )
                .map_err(|e| {
                    error!("GPU K-means clustering failed: {}", e);
                    format!("K-means clustering failed: {}", e)
                })?;

            let computation_time = start_time.elapsed();
            info!(
                "ClusteringActor: K-means clustering completed in {:?}",
                computation_time
            );

            Ok((gpu_result, computation_time))
        }).await;

        let (gpu_result, computation_time) = match blocking_result {
            Ok(inner_result) => inner_result?,
            Err(join_err) => return Err(format!("GPU blocking task panicked: {}", join_err)),
        };

        let (assignments, centroids, inertia, actual_iterations, converged) = gpu_result;

        // Ensure we have the GPU buffer index -> graph node ID mapping
        self.ensure_node_id_map();

        let clusters = self.convert_gpu_kmeans_result_to_clusters(
            assignments.iter().map(|&x| x as u32).collect(),
            params.num_clusters as u32,
        )?;

        let cluster_sizes: Vec<usize> = clusters.iter().map(|c| c.nodes.len()).collect();
        let avg_cluster_size = if !cluster_sizes.is_empty() {
            cluster_sizes.iter().sum::<usize>() as f32 / cluster_sizes.len() as f32
        } else {
            0.0
        };

        
        let silhouette_score = if clusters.len() > 1 && !assignments.is_empty() {
            self.calculate_silhouette_score(&assignments, &centroids, &clusters)?
        } else {
            0.0
        };

        let cluster_stats = ClusteringStats {
            total_clusters: clusters.len(),
            average_cluster_size: avg_cluster_size,
            largest_cluster_size: cluster_sizes.iter().max().copied().unwrap_or(0),
            smallest_cluster_size: cluster_sizes.iter().min().copied().unwrap_or(0),
            silhouette_score,
            computation_time_ms: computation_time.as_millis() as u64,
        };

        // Per ADR-061: maintain the in-process side table for REST API
        // consumers, but DO NOT depend on it for the per-frame binary
        // stream. Wire-side delivery to clients goes via
        // BroadcastAnalyticsUpdate emitted below.
        if let Some(ref analytics_map) = self.node_analytics {
            if let Ok(mut map) = analytics_map.write() {
                for (gpu_idx, &cluster_id) in assignments.iter().enumerate() {
                    let node_id = self.translate_gpu_index(gpu_idx);
                    let entry = map.entry(node_id).or_insert((0, 0.0, 0));
                    entry.0 = cluster_id as u32; // cluster_id
                }
                info!(
                    "ClusteringActor: Populated node_analytics with cluster assignments for {} nodes",
                    assignments.len()
                );
            }
        }

        // ADR-061 §D2: emit analytics_update side-channel message.
        if let Some(ref coord) = self.client_coordinator_addr {
            let generation = self.analytics_generation.fetch_add(1, AtomicOrdering::Relaxed) + 1;
            let entries: Vec<AnalyticsEntry> = assignments
                .iter()
                .enumerate()
                .map(|(gpu_idx, &cluster_id)| AnalyticsEntry {
                    id: self.translate_gpu_index(gpu_idx),
                    cluster_id: Some(cluster_id as u32),
                    community_id: None,
                    anomaly_score: None,
                    sssp_distance: None,
                    sssp_parent: None,
                })
                .collect();
            coord.do_send(BroadcastAnalyticsUpdate {
                source: AnalyticsSource::Clustering,
                generation,
                entries,
            });
        }

        Ok(KMeansResult {
            cluster_assignments: assignments,
            centroids,
            inertia,
            iterations: actual_iterations,
            clusters,
            stats: cluster_stats,
            converged,
            final_iteration: actual_iterations,
        })
    }

    
    async fn perform_community_detection(
        &mut self,
        params: CommunityDetectionParams,
    ) -> Result<CommunityDetectionResult, String> {
        info!(
            "ClusteringActor: Starting {:?} community detection",
            params.algorithm
        );

        let unified_compute_arc = match &self.shared_context {
            Some(ctx) => Arc::clone(&ctx.unified_compute),
            None => {
                return Err("GPU context not initialized".to_string());
            }
        };

        let algorithm = params.algorithm.clone();
        let max_iterations = params.max_iterations.unwrap_or(100);
        let seed = params.seed.unwrap_or(42);

        // Move blocking GPU operations to dedicated blocking thread pool
        // This prevents std::sync::Mutex::lock() from blocking Tokio worker threads
        let blocking_result = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut unified_compute = match unified_compute_arc.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("ClusteringActor: GPU mutex was poisoned, recovering");
                    poisoned.into_inner()
                }
            };

            let start_time = Instant::now();

            let gpu_result = match algorithm {
                CommunityDetectionAlgorithm::LabelPropagation => unified_compute
                    .run_community_detection_label_propagation(
                        max_iterations,
                        seed,
                    )
                    .map_err(|e| {
                        error!("GPU label propagation failed: {}", e);
                        format!("Label propagation failed: {}", e)
                    })?,
                CommunityDetectionAlgorithm::Louvain => {
                    unified_compute
                        .run_louvain_community_detection(
                            max_iterations,
                            1.0,
                            seed,
                        )
                        .map_err(|e| {
                            error!("GPU Louvain community detection failed: {}", e);
                            format!("Louvain community detection failed: {}", e)
                        })?
                }
            };

            let computation_time = start_time.elapsed();
            info!(
                "ClusteringActor: Community detection completed in {:?}",
                computation_time
            );

            Ok((gpu_result, computation_time))
        }).await;

        let (gpu_result, computation_time) = match blocking_result {
            Ok(inner_result) => inner_result?,
            Err(join_err) => return Err(format!("GPU blocking task panicked: {}", join_err)),
        };

        // Ensure we have the GPU buffer index -> graph node ID mapping
        self.ensure_node_id_map();

        let (node_labels, num_communities, modularity, iterations, community_sizes, converged) =
            gpu_result;
        let communities = self.convert_gpu_community_result_to_communities(
            node_labels.iter().map(|&x| x as u32).collect(),
        )?;

        
        let actual_community_sizes: Vec<usize> =
            communities.iter().map(|c| c.nodes.len()).collect();
        let actual_modularity = self.calculate_modularity(&communities);

        // Per ADR-061: maintain the in-process side table for REST API
        // consumers; per-frame binary stream no longer carries community_id.
        // Wire-side delivery to clients goes via BroadcastAnalyticsUpdate
        // emitted below.
        if let Some(ref analytics_map) = self.node_analytics {
            if let Ok(mut map) = analytics_map.write() {
                for (gpu_idx, &community_label) in node_labels.iter().enumerate() {
                    let node_id = self.translate_gpu_index(gpu_idx);
                    let entry = map.entry(node_id).or_insert((0, 0.0, 0));
                    entry.0 = community_label as u32; // cluster_id (REST consumers)
                    entry.2 = community_label as u32; // community_id (REST consumers)
                }
                info!(
                    "ClusteringActor: Populated node_analytics with community assignments for {} nodes",
                    node_labels.len()
                );
            }
        }

        // ADR-061 §D2: emit analytics_update side-channel message.
        if let Some(ref coord) = self.client_coordinator_addr {
            let generation = self.analytics_generation.fetch_add(1, AtomicOrdering::Relaxed) + 1;
            let entries: Vec<AnalyticsEntry> = node_labels
                .iter()
                .enumerate()
                .map(|(gpu_idx, &community_label)| AnalyticsEntry {
                    id: self.translate_gpu_index(gpu_idx),
                    cluster_id: None,
                    community_id: Some(community_label as u32),
                    anomaly_score: None,
                    sssp_distance: None,
                    sssp_parent: None,
                })
                .collect();
            coord.do_send(BroadcastAnalyticsUpdate {
                source: AnalyticsSource::Community,
                generation,
                entries,
            });
        }

        let stats = CommunityDetectionStats {
            total_communities: communities.len(),
            modularity: actual_modularity,
            average_community_size: if !actual_community_sizes.is_empty() {
                actual_community_sizes.iter().sum::<usize>() as f32
                    / actual_community_sizes.len() as f32
            } else {
                0.0
            },
            largest_community: actual_community_sizes.iter().max().copied().unwrap_or(0) as usize,
            smallest_community: actual_community_sizes.iter().min().copied().unwrap_or(0) as usize,
            computation_time_ms: computation_time.as_millis() as u64,
        };

        Ok(CommunityDetectionResult {
            node_labels: node_labels,
            num_communities,
            modularity,
            iterations,
            community_sizes,
            converged,
            communities,
            stats,
            algorithm: params.algorithm,
        })
    }

    /// Run standalone DBSCAN clustering on the GPU.
    async fn perform_dbscan_clustering(
        &mut self,
        params: DBSCANParams,
    ) -> Result<DBSCANResult, String> {
        info!(
            "ClusteringActor: Starting DBSCAN clustering with epsilon={}, min_points={}",
            params.epsilon, params.min_points
        );

        let unified_compute_arc = match &self.shared_context {
            Some(ctx) => Arc::clone(&ctx.unified_compute),
            None => {
                return Err("GPU context not initialized".to_string());
            }
        };

        let epsilon = params.epsilon;
        let min_points = params.min_points;

        // Move blocking GPU operations to dedicated blocking thread pool
        let blocking_result = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut unified_compute = match unified_compute_arc.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("ClusteringActor: GPU mutex was poisoned, recovering");
                    poisoned.into_inner()
                }
            };

            let start_time = Instant::now();

            let labels = unified_compute
                .run_dbscan_clustering(epsilon, min_points as i32)
                .map_err(|e| {
                    error!("GPU DBSCAN clustering failed: {}", e);
                    format!("DBSCAN clustering failed: {}", e)
                })?;

            let computation_time = start_time.elapsed();
            info!(
                "ClusteringActor: DBSCAN clustering completed in {:?}",
                computation_time
            );

            Ok((labels, computation_time))
        })
        .await;

        let (labels, computation_time) = match blocking_result {
            Ok(inner_result) => inner_result?,
            Err(join_err) => return Err(format!("GPU blocking task panicked: {}", join_err)),
        };

        // Ensure we have the GPU buffer index -> graph node ID mapping
        self.ensure_node_id_map();

        // Build cluster groups from labels (ignoring noise = -1)
        let mut cluster_nodes: std::collections::HashMap<i32, Vec<u32>> =
            std::collections::HashMap::new();
        let mut num_noise = 0usize;

        for (gpu_idx, &label) in labels.iter().enumerate() {
            if label < 0 {
                num_noise += 1;
                continue;
            }
            let graph_node_id = self.translate_gpu_index(gpu_idx);
            cluster_nodes
                .entry(label)
                .or_insert_with(Vec::new)
                .push(graph_node_id);
        }

        let num_clusters = cluster_nodes.len();

        // Convert to Cluster structs matching the existing pattern
        let mut clusters = Vec::new();
        for (cluster_label, nodes) in &cluster_nodes {
            clusters.push(crate::handlers::api_handler::analytics::Cluster {
                id: Uuid::new_v4().to_string(),
                label: format!("DBSCAN Cluster {}", cluster_label),
                node_count: nodes.len() as u32,
                coherence: self.calculate_cluster_coherence(nodes, &labels),
                color: format!(
                    "#{:02X}{:02X}{:02X}",
                    (*cluster_label as usize * 50 % 255) as u8,
                    (*cluster_label as usize * 100 % 255) as u8,
                    (*cluster_label as usize * 150 % 255) as u8
                ),
                keywords: Self::generate_cluster_keywords(nodes),
                centroid: Some(self.calculate_cluster_centroid(nodes)),
                nodes: nodes.clone(),
            });
        }

        let cluster_sizes: Vec<usize> = clusters.iter().map(|c| c.nodes.len()).collect();

        // Populate in-process side table for REST API consumers.
        if let Some(ref analytics_map) = self.node_analytics {
            if let Ok(mut map) = analytics_map.write() {
                for (gpu_idx, &label) in labels.iter().enumerate() {
                    if label >= 0 {
                        let node_id = self.translate_gpu_index(gpu_idx);
                        let entry = map.entry(node_id).or_insert((0, 0.0, 0));
                        entry.0 = label as u32; // cluster_id
                    }
                }
                info!(
                    "ClusteringActor: Populated node_analytics with DBSCAN cluster assignments for {} nodes",
                    labels.len() - num_noise
                );
            }
        }

        // ADR-061 §D2: emit analytics_update for DBSCAN under the
        // Clustering source.
        if let Some(ref coord) = self.client_coordinator_addr {
            let generation = self.analytics_generation.fetch_add(1, AtomicOrdering::Relaxed) + 1;
            let entries: Vec<AnalyticsEntry> = labels
                .iter()
                .enumerate()
                .filter(|(_, &label)| label >= 0)
                .map(|(gpu_idx, &label)| AnalyticsEntry {
                    id: self.translate_gpu_index(gpu_idx),
                    cluster_id: Some(label as u32),
                    community_id: None,
                    anomaly_score: None,
                    sssp_distance: None,
                    sssp_parent: None,
                })
                .collect();
            coord.do_send(BroadcastAnalyticsUpdate {
                source: AnalyticsSource::Clustering,
                generation,
                entries,
            });
        }

        let stats = DBSCANStats {
            total_nodes: labels.len(),
            num_clusters,
            num_noise_points: num_noise,
            largest_cluster_size: cluster_sizes.iter().max().copied().unwrap_or(0),
            smallest_cluster_size: cluster_sizes.iter().min().copied().unwrap_or(0),
            average_cluster_size: if !cluster_sizes.is_empty() {
                cluster_sizes.iter().sum::<usize>() as f32 / cluster_sizes.len() as f32
            } else {
                0.0
            },
            computation_time_ms: computation_time.as_millis() as u64,
        };

        Ok(DBSCANResult {
            labels,
            num_clusters,
            num_noise_points: num_noise,
            clusters,
            stats,
        })
    }


    fn convert_gpu_kmeans_result_to_clusters(
        &self,
        gpu_result: Vec<u32>,
        num_clusters: u32,
    ) -> Result<Vec<crate::handlers::api_handler::analytics::Cluster>, String> {
        // gpu_state.num_nodes may lag behind the actual GPU node count when
        // context was set but gpu_state was not forwarded; skip the check when 0.
        if self.gpu_state.num_nodes > 0 && gpu_result.len() != self.gpu_state.num_nodes as usize {
            return Err(format!(
                "GPU result size mismatch: expected {}, got {}",
                self.gpu_state.num_nodes,
                gpu_result.len()
            ));
        }

        let mut cluster_nodes: Vec<Vec<u32>> = vec![Vec::new(); num_clusters as usize];

        for (gpu_idx, &cluster_id) in gpu_result.iter().enumerate() {
            if (cluster_id as usize) < cluster_nodes.len() {
                let graph_node_id = self.translate_gpu_index(gpu_idx);
                cluster_nodes[cluster_id as usize].push(graph_node_id);
            }
        }


        let mut clusters = Vec::new();
        for (cluster_id, nodes) in cluster_nodes.into_iter().enumerate() {
            if !nodes.is_empty() {
                clusters.push(crate::handlers::api_handler::analytics::Cluster {
                    id: Uuid::new_v4().to_string(),
                    label: format!("Cluster {}", cluster_id),
                    node_count: nodes.len() as u32,
                    coherence: {
                        
                        let assignments_i32: Vec<i32> =
                            gpu_result.iter().map(|&x| x as i32).collect();
                        self.calculate_cluster_coherence(&nodes, &assignments_i32)
                    },
                    color: format!(
                        "#{:02X}{:02X}{:02X}",
                        (cluster_id * 50 % 255) as u8,
                        (cluster_id * 100 % 255) as u8,
                        (cluster_id * 150 % 255) as u8
                    ),
                    keywords: Self::generate_cluster_keywords(&nodes),
                    centroid: Some(self.calculate_cluster_centroid(&nodes)),
                    nodes,
                });
            }
        }

        info!(
            "ClusteringActor: Generated {} non-empty clusters",
            clusters.len()
        );
        Ok(clusters)
    }

    
    fn convert_gpu_community_result_to_communities(
        &self,
        gpu_result: Vec<u32>,
    ) -> Result<Vec<Community>, String> {
        if self.gpu_state.num_nodes > 0 && gpu_result.len() != self.gpu_state.num_nodes as usize {
            return Err(format!(
                "GPU result size mismatch: expected {}, got {}",
                self.gpu_state.num_nodes,
                gpu_result.len()
            ));
        }

        let mut community_nodes: std::collections::HashMap<u32, Vec<u32>> =
            std::collections::HashMap::new();

        for (gpu_idx, &community_id) in gpu_result.iter().enumerate() {
            let graph_node_id = self.translate_gpu_index(gpu_idx);
            community_nodes
                .entry(community_id)
                .or_insert_with(Vec::new)
                .push(graph_node_id);
        }

        
        let mut communities = Vec::new();
        for (community_id, nodes) in community_nodes {
            let internal_edges = self.calculate_internal_edges(&nodes);
            let external_edges = self.calculate_external_edges(&nodes);
            let density = self.calculate_community_density(&nodes);

            communities.push(Community {
                id: community_id.to_string(),
                nodes,
                internal_edges,
                external_edges,
                density,
            });
        }

        info!(
            "ClusteringActor: Generated {} communities",
            communities.len()
        );
        Ok(communities)
    }

    
    #[allow(dead_code)]
    fn generate_cluster_color(cluster_id: usize) -> [f32; 3] {
        let mut rng = rand::thread_rng();

        
        let hue = (cluster_id as f32 * 137.5) % 360.0; 
        let saturation = 0.7 + (rng.gen::<f32>() * 0.3); 
        let value = 0.8 + (rng.gen::<f32>() * 0.2); 

        
        let c = value * saturation;
        let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
        let m = value - c;

        let (r, g, b) = match hue as i32 / 60 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        [r + m, g + m, b + m]
    }

    
    
    fn calculate_silhouette_score(
        &self,
        assignments: &[i32],
        centroids: &[(f32, f32, f32)],
        clusters: &[crate::handlers::api_handler::analytics::Cluster],
    ) -> Result<f32, String> {
        if clusters.len() < 2 || assignments.is_empty() {
            return Ok(0.0);
        }

        
        let mut total_silhouette = 0.0;
        let mut valid_samples = 0;

        for (node_idx, &cluster_id) in assignments.iter().enumerate() {
            if cluster_id < 0 || cluster_id as usize >= centroids.len() {
                continue;
            }

            
            let own_cluster_nodes: Vec<usize> = assignments
                .iter()
                .enumerate()
                .filter(|(_, &cid)| cid == cluster_id)
                .map(|(idx, _)| idx)
                .collect();

            let intra_cluster_distance = if own_cluster_nodes.len() > 1 {
                let mut total_distance = 0.0;
                let mut count = 0;
                for &other_node in &own_cluster_nodes {
                    if other_node != node_idx {
                        total_distance +=
                            self.calculate_node_distance(node_idx, other_node, centroids);
                        count += 1;
                    }
                }
                if count > 0 {
                    total_distance / count as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };

            
            let mut min_inter_cluster_distance = f32::INFINITY;
            for other_cluster_id in 0..centroids.len() {
                if other_cluster_id != cluster_id as usize {
                    let other_cluster_nodes: Vec<usize> = assignments
                        .iter()
                        .enumerate()
                        .filter(|(_, &cid)| cid == other_cluster_id as i32)
                        .map(|(idx, _)| idx)
                        .collect();

                    if !other_cluster_nodes.is_empty() {
                        let mut total_distance = 0.0;
                        for &other_node in &other_cluster_nodes {
                            total_distance +=
                                self.calculate_node_distance(node_idx, other_node, centroids);
                        }
                        let avg_distance = total_distance / other_cluster_nodes.len() as f32;
                        min_inter_cluster_distance = min_inter_cluster_distance.min(avg_distance);
                    }
                }
            }

            
            if min_inter_cluster_distance.is_finite() && intra_cluster_distance.is_finite() {
                let max_distance = intra_cluster_distance.max(min_inter_cluster_distance);
                if max_distance > 0.0 {
                    let silhouette =
                        (min_inter_cluster_distance - intra_cluster_distance) / max_distance;
                    total_silhouette += silhouette;
                    valid_samples += 1;
                }
            }
        }

        Ok(if valid_samples > 0 {
            total_silhouette / valid_samples as f32
        } else {
            0.0
        })
    }

    /// Compute Euclidean distance between two nodes using actual GPU positions.
    /// Falls back to centroid Euclidean distance if positions are unavailable.
    fn calculate_node_distance(
        &self,
        node1: usize,
        node2: usize,
        centroids: &[(f32, f32, f32)],
    ) -> f32 {
        // Try to use actual spatial positions from GPU
        if let Some(ref ctx) = self.shared_context {
            if let Ok(uc) = ctx.unified_compute.lock() {
                let n = uc.num_nodes;
                if node1 < n && node2 < n {
                    let mut px = vec![0.0f32; n];
                    let mut py = vec![0.0f32; n];
                    let mut pz = vec![0.0f32; n];
                    if uc.download_positions(&mut px, &mut py, &mut pz).is_ok() {
                        let dx = px[node1] - px[node2];
                        let dy = py[node1] - py[node2];
                        let dz = pz[node1] - pz[node2];
                        return (dx * dx + dy * dy + dz * dz).sqrt();
                    }
                }
            }
        }

        // Fallback: Euclidean distance between cluster centroids
        if centroids.len() >= 2 {
            let c1 = node1 % centroids.len();
            let c2 = node2 % centroids.len();
            let (x1, y1, z1) = centroids[c1];
            let (x2, y2, z2) = centroids[c2];
            let dx = x1 - x2;
            let dy = y1 - y2;
            let dz = z1 - z2;
            (dx * dx + dy * dy + dz * dz).sqrt()
        } else {
            1.0
        }
    }

    
    fn calculate_modularity(&self, communities: &[Community]) -> f32 {
        let _num_nodes = self.gpu_state.num_nodes as f32;
        let total_edges = communities
            .iter()
            .map(|c| c.internal_edges + c.external_edges)
            .sum::<usize>() as f32;

        if total_edges == 0.0 || communities.is_empty() {
            return 0.0;
        }

        let mut modularity = 0.0;

        for community in communities {
            let m = total_edges / 2.0; 
            let e_in = community.internal_edges as f32 / (2.0 * m); 
            let degree_sum = (community.internal_edges + community.external_edges) as f32;
            let a_sq = (degree_sum / (2.0 * m)).powi(2); 

            modularity += e_in - a_sq;
        }

        modularity.max(0.0).min(1.0)
    }

    /// Compute cluster coherence using actual Euclidean distances between
    /// node positions from the GPU layout. High coherence means nodes in
    /// this cluster are spatially close together.
    fn calculate_cluster_coherence(&self, nodes: &[u32], _assignments: &[i32]) -> f32 {
        if nodes.len() < 2 {
            return 1.0;
        }

        // Try to load actual positions from GPU for Euclidean distance
        let positions: Option<(Vec<f32>, Vec<f32>, Vec<f32>)> =
            self.shared_context.as_ref().and_then(|ctx| {
                ctx.unified_compute.lock().ok().and_then(|uc| {
                    let n = uc.num_nodes;
                    let mut px = vec![0.0f32; n];
                    let mut py = vec![0.0f32; n];
                    let mut pz = vec![0.0f32; n];
                    uc.download_positions(&mut px, &mut py, &mut pz)
                        .ok()
                        .map(|_| (px, py, pz))
                })
            });

        let mut total_distance = 0.0f32;
        let mut pair_count = 0u64;

        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let dist = if let Some((ref px, ref py, ref pz)) = positions {
                    let ni = nodes[i] as usize;
                    let nj = nodes[j] as usize;
                    if ni < px.len() && nj < px.len() {
                        let dx = px[ni] - px[nj];
                        let dy = py[ni] - py[nj];
                        let dz = pz[ni] - pz[nj];
                        (dx * dx + dy * dy + dz * dz).sqrt()
                    } else {
                        1.0
                    }
                } else {
                    // No GPU positions available; use constant to avoid fake ordering
                    1.0
                };
                total_distance += dist;
                pair_count += 1;
            }
        }

        if pair_count > 0 {
            let avg_distance = total_distance / pair_count as f32;
            // Inverse relationship: smaller avg distance = higher coherence
            (1.0 / (1.0 + avg_distance)).max(0.1).min(1.0)
        } else {
            1.0
        }
    }

    /// Compute cluster centroid from actual GPU node positions.
    /// Falls back to origin if positions are unavailable.
    fn calculate_cluster_centroid(&self, nodes: &[u32]) -> [f32; 3] {
        if nodes.is_empty() {
            return [0.0, 0.0, 0.0];
        }

        // Try actual positions from GPU
        if let Some(ref ctx) = self.shared_context {
            if let Ok(uc) = ctx.unified_compute.lock() {
                let n = uc.num_nodes;
                let mut px = vec![0.0f32; n];
                let mut py = vec![0.0f32; n];
                let mut pz = vec![0.0f32; n];
                if uc.download_positions(&mut px, &mut py, &mut pz).is_ok() {
                    let mut sx = 0.0f32;
                    let mut sy = 0.0f32;
                    let mut sz = 0.0f32;
                    let mut count = 0u32;
                    for &nid in nodes {
                        let idx = nid as usize;
                        if idx < n {
                            sx += px[idx];
                            sy += py[idx];
                            sz += pz[idx];
                            count += 1;
                        }
                    }
                    if count > 0 {
                        let c = count as f32;
                        return [sx / c, sy / c, sz / c];
                    }
                }
            }
        }

        // Fallback: return origin
        [0.0, 0.0, 0.0]
    }

    /// Count edges where both endpoints are in the node set (internal)
    /// by downloading the CSR graph from the GPU.
    /// Returns 0 if GPU edge data is unavailable.
    fn calculate_internal_edges(&self, nodes: &[u32]) -> usize {
        if nodes.len() < 2 {
            return 0;
        }

        let node_set: std::collections::HashSet<u32> = nodes.iter().copied().collect();

        if let Some(ref ctx) = self.shared_context {
            if let Ok(uc) = ctx.unified_compute.lock() {
                let n = uc.num_nodes;
                let num_edges = uc.num_edges;
                if n > 0 && num_edges > 0 {
                    // Use allocated sizes for copy_to (device buffers may be overallocated)
                    let mut row_offsets = vec![0i32; uc.edge_row_offsets.len()];
                    let mut col_indices = vec![0i32; uc.edge_col_indices.len()];
                    use cust::memory::CopyDestination;
                    if uc.edge_row_offsets.copy_to(&mut row_offsets).is_ok()
                        && uc.edge_col_indices.copy_to(&mut col_indices).is_ok()
                    {
                        row_offsets.truncate(n + 1);
                        col_indices.truncate(num_edges);
                        let mut internal_count = 0usize;
                        for &node_id in nodes {
                            let idx = node_id as usize;
                            if idx < n {
                                let start = row_offsets[idx] as usize;
                                let end = row_offsets[idx + 1] as usize;
                                for &neighbor in &col_indices[start..end.min(col_indices.len())] {
                                    if node_set.contains(&(neighbor as u32)) {
                                        internal_count += 1;
                                    }
                                }
                            }
                        }
                        // Each internal edge counted twice (once from each endpoint)
                        return internal_count / 2;
                    }
                }
            }
        }

        // GPU edge data unavailable
        0
    }

    /// Count edges where exactly one endpoint is in the node set (external)
    /// by downloading the CSR graph from the GPU.
    /// Returns 0 if GPU edge data is unavailable.
    fn calculate_external_edges(&self, nodes: &[u32]) -> usize {
        if nodes.is_empty() {
            return 0;
        }

        let node_set: std::collections::HashSet<u32> = nodes.iter().copied().collect();

        if let Some(ref ctx) = self.shared_context {
            if let Ok(uc) = ctx.unified_compute.lock() {
                let n = uc.num_nodes;
                let num_edges = uc.num_edges;
                if n > 0 && num_edges > 0 {
                    let mut row_offsets = vec![0i32; uc.edge_row_offsets.len()];
                    let mut col_indices = vec![0i32; uc.edge_col_indices.len()];
                    use cust::memory::CopyDestination;
                    if uc.edge_row_offsets.copy_to(&mut row_offsets).is_ok()
                        && uc.edge_col_indices.copy_to(&mut col_indices).is_ok()
                    {
                        row_offsets.truncate(n + 1);
                        col_indices.truncate(num_edges);
                        let mut external_count = 0usize;
                        for &node_id in nodes {
                            let idx = node_id as usize;
                            if idx < n {
                                let start = row_offsets[idx] as usize;
                                let end = row_offsets[idx + 1] as usize;
                                for &neighbor in &col_indices[start..end.min(col_indices.len())] {
                                    if !node_set.contains(&(neighbor as u32)) {
                                        external_count += 1;
                                    }
                                }
                            }
                        }
                        return external_count;
                    }
                }
            }
        }

        // GPU edge data unavailable
        0
    }

    
    fn calculate_community_density(&self, nodes: &[u32]) -> f32 {
        let n = nodes.len();
        if n < 2 {
            return 1.0;
        }

        let max_possible_edges = n * (n - 1) / 2;
        let actual_edges = self.calculate_internal_edges(nodes);

        (actual_edges as f32 / max_possible_edges as f32).min(1.0)
    }
}

impl Actor for ClusteringActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("Clustering Actor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Clustering Actor stopped");
    }
}

// === Message Handlers ===

impl Handler<SetSharedGPUContext> for ClusteringActor {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: SetSharedGPUContext, _ctx: &mut Self::Context) -> Self::Result {
        info!("ClusteringActor: Received SharedGPUContext from ResourceActor");
        self.shared_context = Some(msg.context);
        // Invalidate cached node_id_map so it gets rebuilt from the new context
        self.node_id_map.clear();
        info!("ClusteringActor: SharedGPUContext stored successfully");
        Ok(())
    }
}

impl Handler<RunKMeans> for ClusteringActor {
    type Result = actix::ResponseFuture<Result<KMeansResult, String>>;

    fn handle(&mut self, msg: RunKMeans, _ctx: &mut Self::Context) -> Self::Result {
        info!(
            "ClusteringActor: Received RunKMeans request with {} clusters",
            msg.params.num_clusters
        );

        
        let mut actor_clone = Self {
            gpu_state: self.gpu_state.clone(),
            shared_context: self.shared_context.clone(),
            node_id_map: self.node_id_map.clone(),
            node_analytics: self.node_analytics.clone(),
            client_coordinator_addr: self.client_coordinator_addr.clone(),
            analytics_generation: self.analytics_generation.clone(),
        };

        Box::pin(async move { actor_clone.perform_kmeans_clustering(msg.params).await })
    }
}

impl Handler<RunCommunityDetection> for ClusteringActor {
    type Result = actix::ResponseFuture<Result<CommunityDetectionResult, String>>;

    fn handle(&mut self, msg: RunCommunityDetection, _ctx: &mut Self::Context) -> Self::Result {
        info!("ClusteringActor: Received RunCommunityDetection request");

        
        let mut actor_clone = Self {
            gpu_state: self.gpu_state.clone(),
            shared_context: self.shared_context.clone(),
            node_id_map: self.node_id_map.clone(),
            node_analytics: self.node_analytics.clone(),
            client_coordinator_addr: self.client_coordinator_addr.clone(),
            analytics_generation: self.analytics_generation.clone(),
        };

        Box::pin(async move { actor_clone.perform_community_detection(msg.params).await })
    }
}

impl Handler<RunDBSCAN> for ClusteringActor {
    type Result = actix::ResponseFuture<Result<DBSCANResult, String>>;

    fn handle(&mut self, msg: RunDBSCAN, _ctx: &mut Self::Context) -> Self::Result {
        info!(
            "ClusteringActor: Received RunDBSCAN request (epsilon={}, min_points={})",
            msg.params.epsilon, msg.params.min_points
        );

        let mut actor_clone = Self {
            gpu_state: self.gpu_state.clone(),
            shared_context: self.shared_context.clone(),
            node_id_map: self.node_id_map.clone(),
            node_analytics: self.node_analytics.clone(),
            client_coordinator_addr: self.client_coordinator_addr.clone(),
            analytics_generation: self.analytics_generation.clone(),
        };

        Box::pin(async move { actor_clone.perform_dbscan_clustering(msg.params).await })
    }
}

impl Handler<SetNodeAnalytics> for ClusteringActor {
    type Result = ();

    fn handle(&mut self, msg: SetNodeAnalytics, _ctx: &mut Self::Context) {
        info!("ClusteringActor: Received shared node_analytics map");
        self.node_analytics = Some(msg.node_analytics);
    }
}

impl Handler<crate::actors::messages::SetClientCoordinatorAddr> for ClusteringActor {
    type Result = ();

    fn handle(
        &mut self,
        msg: crate::actors::messages::SetClientCoordinatorAddr,
        _ctx: &mut Self::Context,
    ) {
        info!("ClusteringActor: Received ClientCoordinatorActor address for analytics_update emission");
        self.client_coordinator_addr = Some(msg.addr);
    }
}

impl Handler<UpdateGPUGraphData> for ClusteringActor {
    type Result = Result<(), String>;

    fn handle(&mut self, msg: UpdateGPUGraphData, _ctx: &mut Self::Context) -> Self::Result {
        let num_nodes = msg.graph.nodes.len() as u32;
        let num_edges = msg.graph.edges.len() as u32;
        info!(
            "ClusteringActor: UpdateGPUGraphData received — {} nodes, {} edges",
            num_nodes, num_edges
        );
        self.gpu_state.num_nodes = num_nodes;
        self.gpu_state.num_edges = num_edges;
        // Force re-download of the node_id_map on next clustering operation
        self.node_id_map.clear();
        Ok(())
    }
}

impl Handler<PerformGPUClustering> for ClusteringActor {
    type Result = actix::ResponseFuture<
        Result<Vec<crate::handlers::api_handler::analytics::Cluster>, String>,
    >;

    fn handle(&mut self, msg: PerformGPUClustering, _ctx: &mut Self::Context) -> Self::Result {
        info!(
            "ClusteringActor: Received PerformGPUClustering request with method: {}",
            msg.method
        );

        
        let mut actor_clone = Self {
            gpu_state: self.gpu_state.clone(),
            shared_context: self.shared_context.clone(),
            node_id_map: self.node_id_map.clone(),
            node_analytics: self.node_analytics.clone(),
            client_coordinator_addr: self.client_coordinator_addr.clone(),
            analytics_generation: self.analytics_generation.clone(),
        };

        let method = msg.method.clone();
        Box::pin(async move {
            match method.to_lowercase().as_str() {
                "dbscan" => {
                    let dbscan_params = DBSCANParams {
                        epsilon: msg.params.eps.unwrap_or(0.5),
                        min_points: msg.params.min_samples.unwrap_or(5),
                    };
                    let result = actor_clone.perform_dbscan_clustering(dbscan_params).await?;
                    Ok(result.clusters)
                }
                "louvain" => {
                    let community_params = CommunityDetectionParams {
                        algorithm: CommunityDetectionAlgorithm::Louvain,
                        max_iterations: msg.params.max_iterations,
                        convergence_tolerance: msg.params.convergence_threshold,
                        synchronous: None,
                        seed: msg.params.seed.map(|s| s as u32),
                    };
                    let result = actor_clone.perform_community_detection(community_params).await?;
                    // Convert communities to Cluster format
                    Ok(result.communities.into_iter().map(|c| {
                        crate::handlers::api_handler::analytics::Cluster {
                            id: c.id.clone(),
                            label: format!("Community {}", c.id),
                            node_count: c.nodes.len() as u32,
                            coherence: c.density,
                            color: format!(
                                "#{:02X}{:02X}{:02X}",
                                (c.id.parse::<usize>().unwrap_or(0) * 50 % 255) as u8,
                                (c.id.parse::<usize>().unwrap_or(0) * 100 % 255) as u8,
                                (c.id.parse::<usize>().unwrap_or(0) * 150 % 255) as u8
                            ),
                            keywords: vec![format!("community-{}", c.id)],
                            centroid: None,
                            nodes: c.nodes,
                        }
                    }).collect())
                }
                _ => {
                    // Default: K-means
                    let params = KMeansParams {
                        num_clusters: msg.params.num_clusters.unwrap_or(8) as usize,
                        max_iterations: msg.params.max_iterations,
                        tolerance: msg.params.convergence_threshold,
                        seed: None,
                    };
                    let result = actor_clone.perform_kmeans_clustering(params).await?;
                    Ok(result.clusters)
                }
            }
        })
    }
}
