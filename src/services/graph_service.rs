use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use std::collections::{HashMap, HashSet};
use actix_web::web;
use rand::Rng;
use std::io::{Error, ErrorKind};
use serde_json;
use std::pin::Pin;
use std::time::{Duration, Instant};
use futures::Future;
use log::{info, warn, error, debug};

use tokio::fs::File as TokioFile;
use crate::models::graph::GraphData;
use crate::utils::socket_flow_messages::Node;
use crate::models::edge::Edge;
use crate::models::metadata::MetadataStore;
use crate::app_state::AppState;
use crate::config::Settings;
use crate::utils::gpu_compute::GPUCompute;
use crate::models::simulation_params::{SimulationParams, SimulationPhase, SimulationMode};
use crate::models::pagination::PaginatedGraphData;

// Static flag to prevent multiple simultaneous graph rebuilds
static GRAPH_REBUILD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

// Cache configuration
const NODE_POSITION_CACHE_TTL_MS: u64 = 50; // 50ms cache time
const METADATA_FILE_WAIT_TIMEOUT_MS: u64 = 5000; // 5 second wait timeout
const METADATA_FILE_CHECK_INTERVAL_MS: u64 = 100; // Check every 100ms

#[derive(Clone)]
pub struct GraphService {
    graph_data: Arc<RwLock<GraphData>>,
    node_map: Arc<RwLock<HashMap<String, Node>>>,
    gpu_compute: Option<Arc<RwLock<GPUCompute>>>,
    node_positions_cache: Arc<RwLock<Option<(Vec<Node>, Instant)>>>,
    cache_enabled: bool,
}

impl GraphService {
    pub async fn new(settings: Arc<RwLock<Settings>>, gpu_compute: Option<Arc<RwLock<GPUCompute>>>) -> Self {
        // Get physics settings
        let physics_settings = settings.read().await.visualization.physics.clone();
        let node_map = Arc::new(RwLock::new(HashMap::new()));

        // Log GPU compute status for debugging
        if gpu_compute.is_some() {
            info!("[GraphService] GPU compute is enabled - physics simulation will run");
        } else {
            warn!("[GraphService] GPU compute is NOT enabled - physics simulation will not run");
        }

        // Create the GraphService with caching enabled 
        let _cache = Arc::new(RwLock::new(Option::<(Vec<Node>, Instant)>::None));
        let graph_service = Self {
            graph_data: Arc::new(RwLock::new(GraphData::default())),
            node_map: node_map.clone(),
            gpu_compute,
            node_positions_cache: Arc::new(RwLock::new(None)),
            cache_enabled: true,
            // Node position randomization is now handled entirely by the client side
        };
        
        // Start simulation loop
        let graph_data = Arc::clone(&graph_service.graph_data);
        let node_positions_cache = Arc::clone(&graph_service.node_positions_cache);
        let gpu_compute = graph_service.gpu_compute.clone();
        
        tokio::spawn(async move {
            let params = SimulationParams {
                iterations: physics_settings.iterations,
                spring_strength: physics_settings.spring_strength,
                repulsion: physics_settings.repulsion_strength,
                damping: physics_settings.damping,
                max_repulsion_distance: physics_settings.repulsion_distance,
                viewport_bounds: physics_settings.bounds_size,
                mass_scale: physics_settings.mass_scale,
                boundary_damping: physics_settings.boundary_damping,
                enable_bounds: physics_settings.enable_bounds,
                time_step: 0.016,  // ~60fps
                phase: SimulationPhase::Dynamic,
                mode: SimulationMode::Remote,
            };
            
            loop {
                // Update positions
                let mut graph = graph_data.write().await;
                let mut node_map = node_map.write().await;
                if physics_settings.enabled {
                    if let Some(gpu) = &gpu_compute {
                        if let Err(e) = Self::calculate_layout(gpu, &mut graph, &mut node_map, &params).await {
                            error!("[Graph] Error updating positions: {}", e);
                        } else {
                            debug!("[Graph] Successfully calculated layout for {} nodes", graph.nodes.len());
                        }
                    } else {
                        // Use CPU fallback when GPU is not available
                        debug!("[Graph] GPU compute not available - using CPU fallback for physics calculation");
                        if let Err(e) = Self::calculate_layout_cpu(&mut graph, &mut node_map, &params) {
                            error!("[Graph] Error updating positions with CPU fallback: {}", e);
                        } else {
                            debug!("[Graph] Successfully calculated layout with CPU fallback for {} nodes", graph.nodes.len());
                        }
                    }
                } else {
                    debug!("[Graph] Physics disabled in settings - skipping physics calculation");
                }
                drop(graph); // Release locks
                drop(node_map);
                // Sleep for ~16ms (60fps)
                tokio::time::sleep(tokio::time::Duration::from_millis(16)).await;
                // Clear cache after updates to ensure freshness
                let mut cache = node_positions_cache.write().await;
                *cache = None;
            }
        }); 

        graph_service
    }
    
    /// Wait for metadata file to be available (mounted by Docker)
    pub async fn wait_for_metadata_file() -> bool {
        info!("Checking for metadata file from Docker volume mount...");
        
        // Path to metadata file
        let metadata_path = std::path::Path::new("/app/data/metadata/metadata.json");
        
        // Start timer
        let start_time = Instant::now();
        let timeout = Duration::from_millis(METADATA_FILE_WAIT_TIMEOUT_MS);
        
        // Loop until timeout
        while start_time.elapsed() < timeout {
            // Check if file exists and is not empty
            match tokio::fs::metadata(&metadata_path).await {
                Ok(metadata) => {
                    if metadata.is_file() && metadata.len() > 0 {
                        // Try to open and validate the file
                        match TokioFile::open(&metadata_path).await {
                            Ok(_) => {
                                let elapsed = start_time.elapsed();
                                info!("Metadata file found and accessible after {:?}", elapsed);
                                return true;
                            }
                            Err(e) => {
                                debug!("Metadata file exists but couldn't be opened: {}", e);
                                // Continue waiting - might still be being written to
                            }
                        }
                    } else {
                        debug!("Metadata file exists but is empty or not a regular file");
                    }
                }
                Err(e) => {
                    debug!("Waiting for metadata file to be mounted: {}", e);
                }
            }
            
            // Sleep before checking again
            tokio::time::sleep(Duration::from_millis(METADATA_FILE_CHECK_INTERVAL_MS)).await;
        }
        
        // Timeout reached
        warn!("Timed out waiting for metadata file after {:?}", timeout);
        
        // Timeout reached, file not found or accessible
        false
    }

    pub async fn build_graph_from_metadata(metadata: &MetadataStore) -> Result<GraphData, Box<dyn std::error::Error + Send + Sync>> {
        // Check if a rebuild is already in progress
        if GRAPH_REBUILD_IN_PROGRESS.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            warn!("Graph rebuild already in progress, skipping duplicate rebuild");
            return Err("Graph rebuild already in progress".into());
        }
        
        // Create a guard struct to ensure the flag is reset when this function returns
        struct RebuildGuard;
        impl Drop for RebuildGuard {
            fn drop(&mut self) {
                GRAPH_REBUILD_IN_PROGRESS.store(false, Ordering::SeqCst);
            }
        }
        // This guard will reset the flag when it goes out of scope
        let _guard = RebuildGuard;
        
        let mut graph = GraphData::new();
        let mut edge_map = HashMap::new();
        let mut node_map = HashMap::new();

        // First pass: Create nodes from files in metadata
        let mut valid_nodes = HashSet::new();
        for file_name in metadata.keys() {
            let node_id = file_name.trim_end_matches(".md").to_string();
            valid_nodes.insert(node_id);
        }

        // Create nodes for all valid node IDs
        for node_id in &valid_nodes {
            // Get metadata for this node, including the node_id if available
            let metadata_entry = graph.metadata.get(&format!("{}.md", node_id));
            let stored_node_id = metadata_entry.map(|m| m.node_id.clone());
            
            // Create node with stored ID or generate a new one if not available
            let mut node = Node::new_with_id(node_id.clone(), stored_node_id);
            graph.id_to_metadata.insert(node.id.clone(), node_id.clone());

            // Get metadata for this node
            if let Some(metadata) = metadata.get(&format!("{}.md", node_id)) {
                // Set file size which also calculates mass
                node.set_file_size(metadata.file_size as u64);  // This will update both file_size and mass
                
                // Set the node label to the file name without extension
                // This will be used as the display name for the node
                node.label = metadata.file_name.trim_end_matches(".md").to_string();
                
                // Set visual properties from metadata
                node.size = Some(metadata.node_size as f32);
                
                // Add metadata fields to node's metadata map
                // Add all relevant metadata fields to ensure consistency
                node.metadata.insert("fileName".to_string(), metadata.file_name.clone());
                
                // Add name field (without .md extension) for client-side metadata ID mapping
                if metadata.file_name.ends_with(".md") {
                    let name = metadata.file_name[..metadata.file_name.len() - 3].to_string();
                    node.metadata.insert("name".to_string(), name.clone());
                    node.metadata.insert("metadataId".to_string(), name);
                } else {
                    node.metadata.insert("name".to_string(), metadata.file_name.clone());
                    node.metadata.insert("metadataId".to_string(), metadata.file_name.clone());
                }
                
                node.metadata.insert("fileSize".to_string(), metadata.file_size.to_string());
                node.metadata.insert("nodeSize".to_string(), metadata.node_size.to_string());
                node.metadata.insert("hyperlinkCount".to_string(), metadata.hyperlink_count.to_string());
                node.metadata.insert("sha1".to_string(), metadata.sha1.clone());
                node.metadata.insert("lastModified".to_string(), metadata.last_modified.to_string());
                
                if !metadata.perplexity_link.is_empty() {
                    node.metadata.insert("perplexityLink".to_string(), metadata.perplexity_link.clone());
                }
                
                if let Some(last_process) = metadata.last_perplexity_process {
                    node.metadata.insert("lastPerplexityProcess".to_string(), last_process.to_string());
                }
                
                // We don't add topic_counts to metadata as it would create circular references
                // and is already used to create edges
                
                // Ensure flags is set to 1 (default active state)
                node.data.flags = 1;
            }

            let node_clone = node.clone();
            graph.nodes.push(node_clone);
            // Store nodes in map by numeric ID for efficient lookups
            node_map.insert(node.id.clone(), node);
        }

        // Store metadata in graph
        graph.metadata = metadata.clone();

        // Second pass: Create edges from topic counts
        for (source_file, metadata) in metadata.iter() {
            let source_id = source_file.trim_end_matches(".md").to_string();
            // Find the node with this metadata_id to get its numeric ID
            let source_node = graph.nodes.iter().find(|n| n.metadata_id == source_id);
            if source_node.is_none() {
                continue; // Skip if node not found
            }
            let source_numeric_id = source_node.unwrap().id.clone();
            
            for (target_file, count) in &metadata.topic_counts {
                let target_id = target_file.trim_end_matches(".md").to_string();
                // Find the node with this metadata_id to get its numeric ID
                let target_node = graph.nodes.iter().find(|n| n.metadata_id == target_id);
                if target_node.is_none() {
                    continue; // Skip if node not found
                }
                let target_numeric_id = target_node.unwrap().id.clone();
                
                // Only create edge if both nodes exist and they're different
                if source_numeric_id != target_numeric_id {
                    let edge_key = if source_numeric_id < target_numeric_id {
                        (source_numeric_id.clone(), target_numeric_id.clone())
                    } else {
                        (target_numeric_id.clone(), source_numeric_id.clone())
                    };

                    edge_map.entry(edge_key)
                        .and_modify(|weight| *weight += *count as f32)
                        .or_insert(*count as f32);
                }
            }
        }

        // Convert edge map to edges
        graph.edges = edge_map.into_iter()
            .map(|((source, target), weight)| {
                Edge::new(source, target, weight)
            })
            .collect();

        // Initialize random positions
        Self::initialize_random_positions(&mut graph);

        info!("Built graph with {} nodes and {} edges", graph.nodes.len(), graph.edges.len());
        Ok(graph)
    }

    pub async fn build_graph(state: &web::Data<AppState>) -> Result<GraphData, Box<dyn std::error::Error + Send + Sync>> {
        // Check if a rebuild is already in progress
        if GRAPH_REBUILD_IN_PROGRESS.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            warn!("Graph rebuild already in progress, skipping duplicate rebuild");
            return Err("Graph rebuild already in progress".into());
        }
        
        // Create a guard struct to ensure the flag is reset when this function returns
        struct RebuildGuard;
        impl Drop for RebuildGuard {
            fn drop(&mut self) {
                GRAPH_REBUILD_IN_PROGRESS.store(false, Ordering::SeqCst);
            }
        }
        // This guard will reset the flag when it goes out of scope
        let _guard = RebuildGuard;
        
        let current_graph = state.graph_service.get_graph_data_mut().await;
        let mut graph = GraphData::new();
        let mut node_map = HashMap::new();

        // Copy metadata from current graph
        graph.metadata = current_graph.metadata.clone();

        let mut edge_map = HashMap::new();

        // Create nodes from metadata entries
        let mut valid_nodes = HashSet::new();
        for file_name in graph.metadata.keys() {
            let node_id = file_name.trim_end_matches(".md").to_string();
            valid_nodes.insert(node_id);
        }

        // Create nodes for all valid node IDs
        for node_id in &valid_nodes {
            // Get metadata for this node, including the node_id if available
            let metadata_entry = graph.metadata.get(&format!("{}.md", node_id));
            let stored_node_id = metadata_entry.map(|m| m.node_id.clone());
            
            // Create node with stored ID or generate a new one if not available
            let mut node = Node::new_with_id(node_id.clone(), stored_node_id);
            graph.id_to_metadata.insert(node.id.clone(), node_id.clone());

            // Get metadata for this node
            if let Some(metadata) = graph.metadata.get(&format!("{}.md", node_id)) {
                // Set file size which also calculates mass
                node.set_file_size(metadata.file_size as u64);  // This will update both file_size and mass
                
                // Set the node label to the file name without extension
                // This will be used as the display name for the node
                node.label = metadata.file_name.trim_end_matches(".md").to_string();
                
                // Set visual properties from metadata
                node.size = Some(metadata.node_size as f32);
                
                // Add metadata fields to node's metadata map
                // Add all relevant metadata fields to ensure consistency
                node.metadata.insert("fileName".to_string(), metadata.file_name.clone());
                
                // Add name field (without .md extension) for client-side metadata ID mapping
                if metadata.file_name.ends_with(".md") {
                    let name = metadata.file_name[..metadata.file_name.len() - 3].to_string();
                    node.metadata.insert("name".to_string(), name.clone());
                    node.metadata.insert("metadataId".to_string(), name);
                } else {
                    node.metadata.insert("name".to_string(), metadata.file_name.clone());
                    node.metadata.insert("metadataId".to_string(), metadata.file_name.clone());
                }
                
                node.metadata.insert("fileSize".to_string(), metadata.file_size.to_string());
                node.metadata.insert("nodeSize".to_string(), metadata.node_size.to_string());
                node.metadata.insert("hyperlinkCount".to_string(), metadata.hyperlink_count.to_string());
                node.metadata.insert("sha1".to_string(), metadata.sha1.clone());
                node.metadata.insert("lastModified".to_string(), metadata.last_modified.to_string());
                
                if !metadata.perplexity_link.is_empty() {
                    node.metadata.insert("perplexityLink".to_string(), metadata.perplexity_link.clone());
                }
                
                if let Some(last_process) = metadata.last_perplexity_process {
                    node.metadata.insert("lastPerplexityProcess".to_string(), last_process.to_string());
                }
                
                // We don't add topic_counts to metadata as it would create circular references
                // and is already used to create edges
                
                // Ensure flags is set to 1 (default active state)
                node.data.flags = 1;
            }
            
            let node_clone = node.clone();
            graph.nodes.push(node_clone);
            // Store nodes in map by numeric ID for efficient lookups
            node_map.insert(node.id.clone(), node);
        }

        // Create edges from metadata topic counts
        for (source_file, metadata) in graph.metadata.iter() {
            let source_id = source_file.trim_end_matches(".md").to_string();
            // Find the node with this metadata_id to get its numeric ID
            let source_node = graph.nodes.iter().find(|n| n.metadata_id == source_id);
            if source_node.is_none() {
                continue; // Skip if node not found
            }
            let source_numeric_id = source_node.unwrap().id.clone();
            
            // Process outbound links from this file to other topics
            for (target_file, count) in &metadata.topic_counts {
                let target_id = target_file.trim_end_matches(".md").to_string();
                // Find the node with this metadata_id to get its numeric ID
                let target_node = graph.nodes.iter().find(|n| n.metadata_id == target_id);
                if target_node.is_none() {
                    continue; // Skip if node not found
                }
                let target_numeric_id = target_node.unwrap().id.clone();
                
                // Only create edge if both nodes exist and they're different
                if source_numeric_id != target_numeric_id {
                    let edge_key = if source_numeric_id < target_numeric_id {
                        (source_numeric_id.clone(), target_numeric_id.clone())
                    } else {
                        (target_numeric_id.clone(), source_numeric_id.clone())
                    };

                    // Sum the weights for bi-directional references
                    edge_map.entry(edge_key)
                        .and_modify(|w| *w += *count as f32)
                        .or_insert(*count as f32);
                }
            }
        }

        // Convert edge map to edges
        graph.edges = edge_map.into_iter()
            .map(|((source, target), weight)| {
                Edge::new(source, target, weight)
            })
            .collect();

        // Initialize random positions for all nodes
        Self::initialize_random_positions(&mut graph);

        info!("Built graph with {} nodes and {} edges", graph.nodes.len(), graph.edges.len());
        Ok(graph)
    }

    fn initialize_random_positions(graph: &mut GraphData) {
        let mut rng = rand::thread_rng();
        let node_count = graph.nodes.len() as f32;
        let initial_radius = 3.0; // Increasing radius for better visibility
        let golden_ratio = (1.0 + 5.0_f32.sqrt()) / 2.0;
        
        // Log the initialization process
        info!("Initializing random positions for {} nodes with radius {}", 
             node_count, initial_radius);
        info!("First 5 node numeric IDs: {}", graph.nodes.iter().take(5).map(|n| n.id.clone()).collect::<Vec<_>>().join(", "));
        info!("First 5 node metadata IDs: {}", graph.nodes.iter().take(5).map(|n| n.metadata_id.clone()).collect::<Vec<_>>().join(", "));
        
        // Use Fibonacci sphere distribution for more uniform initial positions
        for (i, node) in graph.nodes.iter_mut().enumerate() {
            let i_float: f32 = i as f32;
            
            // Calculate Fibonacci sphere coordinates
            let theta = 2.0 * std::f32::consts::PI * i_float / golden_ratio;
            let phi = (1.0 - 2.0 * (i_float + 0.5) / node_count).acos();
            
            // Add slight randomness to prevent exact overlaps
            let r = initial_radius * (0.9 + rng.gen_range(0.0..0.2));
            
            node.set_x(r * phi.sin() * theta.cos());
            node.set_y(r * phi.sin() * theta.sin());
            node.set_z(r * phi.cos());
            
            // Initialize with zero velocity
            node.set_vx(0.0);
            node.set_vy(0.0);
            node.set_vz(0.0);

            // Log first 5 nodes for debugging
            if i < 5 {
                info!("Initialized node {}: id={}, pos=[{:.3},{:.3},{:.3}]", 
                     i,
                     node.id,
                     node.data.position.x, 
                     node.data.position.y, 
                     node.data.position.z);
            }
        }
    }

    pub async fn calculate_layout(
        gpu_compute: &Arc<RwLock<GPUCompute>>,
        graph: &mut GraphData,
        node_map: &mut HashMap<String, Node>, 
        params: &SimulationParams,
    ) -> std::io::Result<()> {
        {
            info!("[calculate_layout] Starting GPU physics calculation for {} nodes, {} edges with mode {:?}", 
                  graph.nodes.len(), graph.edges.len(), params.mode);
            
            // Get current timestamp for performance tracking
            let start_time = std::time::Instant::now();

            let mut gpu_compute = gpu_compute.write().await;

            info!("[calculate_layout] params: iterations={}, spring_strength={:.3}, repulsion={:.3}, damping={:.3}",
                 params.iterations, params.spring_strength, params.repulsion, params.damping);
            
            // Update data and parameters
            if let Err(e) = gpu_compute.update_graph_data(graph) {
                error!("[calculate_layout] Failed to update graph data in GPU: {}, node count: {}", 
                      e, graph.nodes.len());
                // Log more details about the graph for debugging
                if !graph.nodes.is_empty() {
                    debug!("First node: id={}, position=[{:.3},{:.3},{:.3}]", graph.nodes[0].id, graph.nodes[0].data.position.x, graph.nodes[0].data.position.y, graph.nodes[0].data.position.z);
                }
                return Err(e);
            }
            
            if let Err(e) = gpu_compute.update_simulation_params(params) {
                error!("[calculate_layout] Failed to update simulation parameters in GPU: {}", e);
                return Err(e);
            }
            
            // Perform computation step
            if let Err(e) = gpu_compute.step() {
                error!("[calculate_layout] Failed to execute physics step: {}, graph has {} nodes and {} edges", 
                       e, graph.nodes.len(), graph.edges.len());
                return Err(e);
            }
            
            // Get updated positions
            let updated_nodes = match gpu_compute.get_node_data() {
                Ok(nodes) => {
                    info!("[calculate_layout] Successfully retrieved {} nodes from GPU", nodes.len());
                    nodes
                },
                Err(e) => {
                    error!("[calculate_layout] Failed to get node data from GPU: {}", e);
                    return Err(e);
                }
            };
            
            // Update graph with new positions
            let mut nodes_updated = 0;
            for (i, node) in graph.nodes.iter_mut().enumerate() {
                if i >= updated_nodes.len() {
                    error!("[calculate_layout] Node index out of range: {} >= {}", i, updated_nodes.len());
                    continue;
                }
                
                // Update position and velocity from GPU data
                node.data = updated_nodes[i];
                nodes_updated += 1;
                
                // Update node_map as well
                if let Some(map_node) = node_map.get_mut(&node.id) {
                    map_node.data = updated_nodes[i];
                } else {
                    warn!("[calculate_layout] Node {} not found in node_map", node.id);
                }
            }
            
            // Log performance info
            let elapsed = start_time.elapsed();
            
                // Log sample positions for debugging (first 2 nodes)
                let sample_positions = if graph.nodes.len() >= 2 {
                    format!("[{:.2},{:.2},{:.2}], [{:.2},{:.2},{:.2}]", 
                        graph.nodes[0].data.position.x, graph.nodes[0].data.position.y, graph.nodes[0].data.position.z,
                        graph.nodes[1].data.position.x, graph.nodes[1].data.position.y, graph.nodes[1].data.position.z)
                } else if graph.nodes.len() == 1 {
                    format!("[{:.2},{:.2},{:.2}]", graph.nodes[0].data.position.x, graph.nodes[0].data.position.y, graph.nodes[0].data.position.z)
                } else { "no nodes".to_string() };
            
                info!("[calculate_layout] Updated positions for {}/{} nodes in {:?}. Sample positions: {}", nodes_updated, graph.nodes.len(), elapsed, sample_positions);
            
            Ok(())
        }
    }

    /// CPU fallback implementation of force-directed graph layout
    pub fn calculate_layout_cpu(
        graph: &mut GraphData,
        node_map: &mut HashMap<String, Node>,
        params: &SimulationParams,
    ) -> std::io::Result<()> {
        // Get current timestamp for performance tracking
        let start_time = std::time::Instant::now();
        
        // Early return if there are no nodes to process
        if graph.nodes.is_empty() {
            debug!("[calculate_layout_cpu] No nodes to process");
            return Ok(());
        }
        
        // Convert the simulation parameters to local variables for easier access
        let spring_strength = params.spring_strength;
        let repulsion = params.repulsion;
        let damping = params.damping;
        let max_repulsion_distance = params.max_repulsion_distance;
        let time_step = params.time_step;
        let enable_bounds = params.enable_bounds;
        let bounds_size = params.viewport_bounds;
        let boundary_damping = params.boundary_damping;
        let mass_scale = params.mass_scale;
        
        // Create a copy of nodes for thread safety during force calculation
        let nodes_copy: Vec<Node> = graph.nodes.clone();
        
        // Initialize force accumulators for each node
        let mut forces: Vec<(f32, f32, f32)> = vec![(0.0, 0.0, 0.0); graph.nodes.len()];
        
        // Calculate repulsive forces between all pairs of nodes
        // This is an O(n²) operation - the most expensive part of the algorithm
        for i in 0..nodes_copy.len() {
            for j in (i+1)..nodes_copy.len() {
                let node_i = &nodes_copy[i];
                let node_j = &nodes_copy[j];
                
                // Calculate distance between nodes
                let dx = node_j.data.position.x - node_i.data.position.x;
                let dy = node_j.data.position.y - node_i.data.position.y;
                let dz = node_j.data.position.z - node_i.data.position.z;
                
                let distance_squared = dx * dx + dy * dy + dz * dz;
                
                // Avoid division by zero and limit maximum repulsion distance
                if distance_squared < 0.0001 {
                    continue;
                }
                
                let distance = distance_squared.sqrt();
                
                // Only apply repulsion within max_repulsion_distance
                if distance > max_repulsion_distance {
                    continue;
                }
                
                // Use inverse-square law for repulsion (like gravity/electrostatic)
                // Calculate repulsion strength based on node masses (stored in data.mass) and distance
                let mass_i = (node_i.data.mass as f32 / 255.0) * 10.0 * mass_scale;
                let mass_j = (node_j.data.mass as f32 / 255.0) * 10.0 * mass_scale;
                
                // Normalize the repulsion to be between 0 and 1 based on max distance
                let _normalized_distance = distance / max_repulsion_distance;
                let repulsion_factor = repulsion * mass_i * mass_j / distance_squared;
                
                // Normalize direction
                let nx = dx / distance;
                let ny = dy / distance;
                let nz = dz / distance;
                
                // Apply repulsive force (nodes push each other away)
                let fx = nx * repulsion_factor;
                let fy = ny * repulsion_factor;
                let fz = nz * repulsion_factor;
                
                // Add forces (equal and opposite for each node)
                forces[i].0 -= fx;
                forces[i].1 -= fy;
                forces[i].2 -= fz;
                
                forces[j].0 += fx;
                forces[j].1 += fy;
                forces[j].2 += fz;
            }
        }
        
        // Calculate attractive forces for edges (spring forces)
        for edge in &graph.edges {
            // Find indices of source and target nodes
            let source_idx = graph.nodes.iter().position(|n| n.id == edge.source);
            let target_idx = graph.nodes.iter().position(|n| n.id == edge.target);
            
            if let (Some(i), Some(j)) = (source_idx, target_idx) {
                let node_i = &nodes_copy[i];
                let node_j = &nodes_copy[j];
                
                // Calculate distance between nodes
                let dx = node_j.data.position.x - node_i.data.position.x;
                let dy = node_j.data.position.y - node_i.data.position.y;
                let dz = node_j.data.position.z - node_i.data.position.z;
                
                let distance_squared = dx * dx + dy * dy + dz * dz;
                
                // Avoid division by zero
                if distance_squared < 0.0001 {
                    continue;
                }
                
                let distance = distance_squared.sqrt();
                
                // Spring force increases with distance and edge weight
                let spring_factor = spring_strength * edge.weight * distance;
                
                // Normalize direction
                let nx = dx / distance;
                let ny = dy / distance;
                let nz = dz / distance;
                
                // Apply spring force (edges pull nodes together)
                let fx = nx * spring_factor;
                let fy = ny * spring_factor;
                let fz = nz * spring_factor;
                
                // Add forces (pulling both nodes toward each other)
                forces[i].0 += fx;
                forces[i].1 += fy;
                forces[i].2 += fz;
                
                forces[j].0 -= fx;
                forces[j].1 -= fy;
                forces[j].2 -= fz;
            }
        }
        
        // Update velocities and positions based on calculated forces
        for (i, node) in graph.nodes.iter_mut().enumerate() {
            // Apply force to velocity with damping
            node.set_vx(node.data.velocity.x * damping + forces[i].0 * time_step);
            node.set_vy(node.data.velocity.y * damping + forces[i].1 * time_step);
            node.set_vz(node.data.velocity.z * damping + forces[i].2 * time_step);
            
            // Update position based on velocity
            node.set_x(node.data.position.x + node.data.velocity.x * time_step);
            node.set_y(node.data.position.y + node.data.velocity.y * time_step);
            node.set_z(node.data.position.z + node.data.velocity.z * time_step);
            
            // Apply boundary constraints if enabled
            if enable_bounds {
                let half_bounds = bounds_size / 2.0;
                
                // For each axis, if position exceeds boundary:
                // 1. Clamp position to boundary
                // 2. Reverse velocity with damping
                
                if node.data.position.x > half_bounds {
                    node.set_x(half_bounds);
                    node.set_vx(-node.data.velocity.x * boundary_damping);
                } else if node.data.position.x < -half_bounds {
                    node.set_x(-half_bounds);
                    node.set_vx(-node.data.velocity.x * boundary_damping);
                }
                
                if node.data.position.y > half_bounds {
                    node.set_y(half_bounds);
                    node.set_vy(-node.data.velocity.y * boundary_damping);
                } else if node.data.position.y < -half_bounds {
                    node.set_y(-half_bounds);
                    node.set_vy(-node.data.velocity.y * boundary_damping);
                }
                
                if node.data.position.z > half_bounds {
                    node.set_z(half_bounds);
                    node.set_vz(-node.data.velocity.z * boundary_damping);
                } else if node.data.position.z < -half_bounds {
                    node.set_z(-half_bounds);
                    node.set_vz(-node.data.velocity.z * boundary_damping);
                }
            }
            
            // Update node_map as well
            if let Some(map_node) = node_map.get_mut(&node.id) {
                map_node.data = node.data.clone();
            }
        }
        
        // Log performance info
        let elapsed = start_time.elapsed();
        info!("[calculate_layout_cpu] Updated positions for {} nodes in {:?}", 
              graph.nodes.len(), elapsed);
        
        Ok(())
    }

    pub async fn get_paginated_graph_data(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedGraphData, Box<dyn std::error::Error + Send + Sync>> {
        let graph = self.graph_data.read().await;
        
        // Convert page and page_size to usize for vector operations
        let page = page as usize;
        let page_size = page_size as usize;
        let total_nodes = graph.nodes.len();
        
        let start = page * page_size;
        let end = std::cmp::min((page + 1) * page_size, total_nodes);

        let mut page_nodes: Vec<Node> = graph.nodes
            .iter()
            .skip(start)
            .take(end - start)
            .cloned() 
            .collect();

        // Get edges that connect to these nodes
        let node_ids: HashSet<String> = page_nodes.iter()
            .map(|n| n.id.clone())
            .collect();

        let edges: Vec<Edge> = graph.edges
            .iter()
            .filter(|e| node_ids.contains(&e.source) || node_ids.contains(&e.target))
            .cloned()
            .collect();

        Ok(PaginatedGraphData {
            nodes: page_nodes,
            edges,
            metadata: serde_json::to_value(graph.metadata.clone()).unwrap_or_default(),
            total_nodes,
            total_edges: graph.edges.len(),
            total_pages: ((total_nodes as f32 / page_size as f32).ceil()) as u32,
            current_page: page as u32,
        })
    }
    
    // Clear position cache to force a refresh on next request
    pub async fn clear_position_cache(&self) {
        let mut cache = self.node_positions_cache.write().await;
        *cache = None;
    }

    pub async fn get_node_positions(&self) -> Vec<Node> {
        let start_time = Instant::now();

        // First check if we have a valid cached result
        if self.cache_enabled {
            let cache = self.node_positions_cache.read().await;
            if let Some((cached_nodes, timestamp)) = &*cache {
                let age = start_time.duration_since(*timestamp);
                
                // If cache is still fresh, use it
                if age < Duration::from_millis(NODE_POSITION_CACHE_TTL_MS) {
                    debug!("Using cached node positions ({} nodes, age: {:?})", 
                           cached_nodes.len(), age);
                    return cached_nodes.clone();
                }
            }
        }

        // No valid cache, fetch from graph data
        let nodes = {
            let graph = self.graph_data.read().await;
            
            // Only log node position data in debug level
            debug!("get_node_positions: reading {} nodes from graph (cache miss)", graph.nodes.len());
            
            // Clone the nodes vector 
            graph.nodes.clone()
        };

        // Update cache with new result
        if self.cache_enabled {
            let mut cache = self.node_positions_cache.write().await;
            *cache = Some((nodes.clone(), start_time));
        }

        let elapsed = start_time.elapsed();
        debug!("Node position fetch completed in {:?} for {} nodes", elapsed, nodes.len());
        
        // Log first 5 nodes only when debug is enabled
        let sample_size = std::cmp::min(5, nodes.len());
        if sample_size > 0 && log::log_enabled!(log::Level::Debug) {
            debug!("Node position sample: {} samples of {} nodes", sample_size, nodes.len());
        }
        nodes
    }

    pub async fn get_graph_data_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, GraphData> {
        self.graph_data.write().await
    }

    pub async fn get_node_map_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, HashMap<String, Node>> {
        self.node_map.write().await
    }
    
    // Add method to get GPU compute instance
    pub async fn get_gpu_compute(&self) -> Option<Arc<RwLock<GPUCompute>>> {
        self.gpu_compute.clone()
    }

    pub async fn update_node_positions(&self, updates: Vec<(u16, Node)>) -> Result<(), Error> {
        let mut graph = self.graph_data.write().await;
        let mut node_map = self.node_map.write().await;

        for (node_id_u16, node_data) in updates {
            let node_id = node_id_u16.to_string();
            if let Some(node) = node_map.get_mut(&node_id) {
                node.data = node_data.data.clone();
            }
        }

        // Update graph nodes with new positions from the map
        for node in &mut graph.nodes {
            if let Some(updated_node) = node_map.get(&node.id) {
                node.data = updated_node.data.clone();
            }
        }

        Ok(())
    }

    pub fn update_positions(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + '_>> {
        Box::pin(async move {
            if let Some(gpu) = &self.gpu_compute {
                let mut gpu = gpu.write().await;
                gpu.compute_forces()?;
                Ok(())
            } else {
                // Initialize GPU if not already done
                if self.gpu_compute.is_none() {
                    let settings = Arc::new(RwLock::new(Settings::default()));
                    let graph_data = GraphData::default(); // Or get your actual graph data
                    self.initialize_gpu(settings, &graph_data).await?;
                    return self.update_positions().await;
                }
                Err(Error::new(ErrorKind::Other, "GPU compute not initialized"))
            }
        })
    }

    pub async fn initialize_gpu(&mut self, _settings: Arc<RwLock<Settings>>, graph_data: &GraphData) -> Result<(), Error> {
        info!("Initializing GPU compute system...");

        // If GPU is already initialized, don't reinitialize
        if self.gpu_compute.is_some() {
            info!("GPU compute is already initialized, skipping initialization");
            return Ok(());
        }

        match GPUCompute::new(graph_data).await {
            Ok(gpu_instance) => {
                // Try a test computation before accepting the GPU
                {
                    let mut gpu = gpu_instance.write().await;
                    if let Err(e) = gpu.compute_forces() {
                        error!("GPU test computation failed: {}", e);
                        return Err(Error::new(ErrorKind::Other, format!("GPU test computation failed: {}", e)));
                    }
                    info!("GPU test computation succeeded");
                }

                self.gpu_compute = Some(gpu_instance);
                info!("GPU compute system successfully initialized");
                Ok(())
            }
            Err(e) => {
                error!("Failed to initialize GPU compute: {}. Physics simulation will not work.", e);
                Err(Error::new(ErrorKind::Other, format!("GPU initialization failed: {}", e)))
            }
        }
    }

    // Development test function to verify metadata transfer
    #[cfg(test)]
    pub async fn test_metadata_transfer() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use chrono::Utc;
        use std::collections::HashMap;
        use crate::models::metadata::Metadata;

        // Create test metadata
        let mut metadata = crate::models::metadata::MetadataStore::new();
        let file_name = "test.md";
        
        // Create a test metadata entry
        let meta = Metadata {
            file_name: file_name.to_string(),
            file_size: 1000,
            node_size: 1.5,
            hyperlink_count: 5,
            sha1: "abc123".to_string(),
            node_id: "1".to_string(),
            last_modified: Utc::now(),
            perplexity_link: "https://example.com".to_string(),
            last_perplexity_process: Some(Utc::now()),
            topic_counts: HashMap::new(),
        };
        
        metadata.insert(file_name.to_string(), meta.clone());
        
        // Build graph from metadata
        let graph = Self::build_graph_from_metadata(&metadata).await?;
        
        // Check that the graph has one node with the correct metadata
        assert_eq!(graph.nodes.len(), 1);
        
        // Verify metadata_id
        let node = &graph.nodes[0];
        assert_eq!(node.metadata_id, "test");
        
        // Verify metadata fields
        assert!(node.metadata.contains_key("fileName"));
        assert_eq!(node.metadata.get("fileName").unwrap(), "test.md");
        
        assert!(node.metadata.contains_key("fileSize"));
        assert_eq!(node.metadata.get("fileSize").unwrap(), "1000");
        
        assert!(node.metadata.contains_key("nodeSize"));
        assert_eq!(node.metadata.get("nodeSize").unwrap(), "1.5");
        
        assert!(node.metadata.contains_key("hyperlinkCount"));
        assert_eq!(node.metadata.get("hyperlinkCount").unwrap(), "5");
        
        assert!(node.metadata.contains_key("sha1"));
        assert!(node.metadata.contains_key("lastModified"));
        
        // Check flags
        assert_eq!(node.data.flags, 1);

        println!("All metadata tests passed!");
        Ok(())
    }
}
