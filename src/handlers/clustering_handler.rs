use crate::actors::messages::{GetSettings, UpdateSettings};
use crate::app_state::AppState;
use crate::config::ClusteringConfiguration;
use crate::{ok_json, error_json, bad_request, service_unavailable};
use actix_web::{web, HttpRequest, HttpResponse};
use log::{debug, error, info, warn};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;

/// Minimum cluster size for meaningful grouping
const MIN_CLUSTER_SIZE: usize = 3;
/// Default similarity metric for clustering
const DEFAULT_SIMILARITY_METRIC: &str = "cosine";
/// Maximum iterations for convergence
const MAX_CLUSTERING_ITERATIONS: usize = 100;
/// Default number of clusters when not specified
const DEFAULT_CLUSTER_COUNT: u32 = 8;
/// Default convergence threshold for iterative algorithms (f32 for ClusteringParams)
const DEFAULT_CONVERGENCE_THRESHOLD: f32 = 0.001;
/// Default tolerance threshold for iterative algorithms (f64 for ClusteringParams)
const DEFAULT_TOLERANCE: f64 = 0.001;
/// Default resolution parameter for community detection
const DEFAULT_RESOLUTION: f32 = 1.0;
/// Default sigma for spectral clustering (f64 for ClusteringParams)
const DEFAULT_SIGMA: f64 = 1.0;
/// Default minimum modularity gain for Louvain algorithm (f64 for ClusteringParams)
const DEFAULT_MIN_MODULARITY_GAIN: f64 = 0.01;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/clustering")
            .route("/configure", web::post().to(configure_clustering))
            .route("/start", web::post().to(start_clustering))
            .route("/status", web::get().to(get_clustering_status))
            .route("/results", web::get().to(get_clustering_results))
            .route("/export", web::post().to(export_cluster_assignments))
            .route("/dbscan", web::post().to(run_dbscan)),
    );
}

async fn configure_clustering(
    _req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<ClusteringConfiguration>,
) -> Result<HttpResponse, actix_web::Error> {
    let config = payload.into_inner();

    info!(
        "Clustering configuration request: algorithm={}, clusters={}",
        config.algorithm, config.num_clusters
    );

    
    if let Err(e) = validate_clustering_config(&config) {
        return bad_request!("Invalid clustering configuration: {}", e);
    }

    
    let settings_update = json!({
        "visualisation": {
            "graphs": {
                "logseq": {
                    "physics": {
                        "clusteringAlgorithm": config.algorithm,
                        "clusterCount": config.num_clusters,
                        "clusteringResolution": config.resolution,
                        "clusteringIterations": config.iterations
                    }
                },
                "visionflow": {
                    "physics": {
                        "clusteringAlgorithm": config.algorithm,
                        "clusterCount": config.num_clusters,
                        "clusteringResolution": config.resolution,
                        "clusteringIterations": config.iterations
                    }
                }
            }
        }
    });

    
    let mut app_settings = match state.settings_addr.send(GetSettings).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            error!("Failed to get current settings: {}", e);
            return error_json!("Failed to get current settings");
        }
        Err(e) => {
            error!("Settings actor error: {}", e);
            return service_unavailable!("Settings service unavailable");
        }
    };

    if let Err(e) = app_settings.merge_update(settings_update) {
        error!("Failed to merge clustering configuration: {}", e);
        return error_json!("Failed to update clustering configuration: {}", e);
    }

    
    match state
        .settings_addr
        .send(UpdateSettings {
            settings: app_settings,
        })
        .await
    {
        Ok(Ok(())) => {
            info!("Clustering configuration saved successfully");
            ok_json!(json!({
                "status": "Clustering configuration updated successfully",
                "config": config
            }))
        }
        Ok(Err(e)) => {
            error!("Failed to save clustering configuration: {}", e);
            error_json!("Failed to save clustering configuration: {}", e)
        }
        Err(e) => {
            error!("Settings actor error: {}", e);
            service_unavailable!("Settings service unavailable")
        }
    }
}

async fn start_clustering(
    _req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<Value>,
) -> Result<HttpResponse, actix_web::Error> {
    let request = payload.into_inner();

    let start = Instant::now();

    info!("Starting real GPU clustering analysis");
    debug!(
        "Clustering request: {}",
        serde_json::to_string_pretty(&request).unwrap_or_default()
    );

    // Fetch current settings for clustering defaults
    let (settings_algorithm, settings_cluster_count, settings_resolution, settings_iterations) =
        match state.settings_addr.send(GetSettings).await {
            Ok(Ok(settings)) => {
                let physics = &settings.visualisation.graphs.logseq.physics;
                (
                    Some(physics.clustering_algorithm.clone()),
                    Some(physics.cluster_count),
                    Some(physics.clustering_resolution),
                    Some(physics.clustering_iterations),
                )
            }
            Ok(Err(e)) => {
                warn!("Failed to get settings for clustering defaults, using hardcoded: {}", e);
                (None, None, None, None)
            }
            Err(e) => {
                warn!("Settings actor unavailable for clustering defaults, using hardcoded: {}", e);
                (None, None, None, None)
            }
        };

    // Request body values override settings, which override hardcoded defaults
    let algorithm = request
        .get("algorithm")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or(settings_algorithm)
        .unwrap_or_else(|| "louvain".to_string());

    let cluster_count = request
        .get("clusterCount")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .or(settings_cluster_count)
        .unwrap_or(DEFAULT_CLUSTER_COUNT);

    let resolution = settings_resolution.unwrap_or(DEFAULT_RESOLUTION);

    let max_iterations = settings_iterations.unwrap_or(MAX_CLUSTERING_ITERATIONS as u32);

    let task_id = uuid::Uuid::new_v4().to_string();

    info!(
        "Starting GPU clustering with algorithm: {}, clusters: {}",
        algorithm, cluster_count
    );

    // Route PerformGPUClustering through GPUManagerActor → AnalyticsSupervisor → ClusteringActor
    if let Some(gpu_manager) = state.gpu_manager_addr.as_ref() {

        use crate::actors::messages::PerformGPUClustering;

        let request = PerformGPUClustering {
            method: algorithm.to_string(),
            params: crate::handlers::api_handler::analytics::ClusteringParams {
                num_clusters: Some(cluster_count),
                max_iterations: Some(max_iterations),
                convergence_threshold: Some(DEFAULT_CONVERGENCE_THRESHOLD),
                resolution: Some(resolution),
                eps: None,
                min_samples: None,
                min_cluster_size: Some(MIN_CLUSTER_SIZE as u32),
                similarity: Some(DEFAULT_SIMILARITY_METRIC.to_string()),
                distance_threshold: None,
                linkage: None,
                random_state: None,
                damping: None,
                preference: None,
                tolerance: Some(DEFAULT_TOLERANCE),
                seed: None,
                sigma: Some(DEFAULT_SIGMA),
                min_modularity_gain: Some(DEFAULT_MIN_MODULARITY_GAIN),
            },
            task_id: format!("{}_{}", algorithm, chrono::Utc::now().timestamp_millis()),
        };

        let clustering_result = gpu_manager.send(request).await;

        match clustering_result {
            Ok(Ok(cluster_results)) => {
                let computation_time_ms = start.elapsed().as_millis() as u64;

                info!(
                    "GPU clustering completed successfully with {} clusters in {}ms",
                    cluster_results.len(),
                    computation_time_ms
                );

                // Compute real modularity from cluster size distribution (Newman approximation)
                let total_nodes: usize = cluster_results.iter().map(|c| c.nodes.len()).sum();
                let modularity = if total_nodes > 0 {
                    1.0 - cluster_results
                        .iter()
                        .map(|c| (c.nodes.len() as f64 / total_nodes as f64).powi(2))
                        .sum::<f64>()
                } else {
                    0.0
                };

                // Average coherence across clusters (weighted by size)
                let avg_coherence = if total_nodes > 0 {
                    cluster_results
                        .iter()
                        .map(|c| c.coherence as f64 * c.nodes.len() as f64)
                        .sum::<f64>()
                        / total_nodes as f64
                } else {
                    0.0
                };

                // Populate shared node_analytics with cluster assignments for V3 wire format
                if let Ok(mut analytics) = state.node_analytics.write() {
                    for (idx, cluster) in cluster_results.iter().enumerate() {
                        for &node_id in &cluster.nodes {
                            let entry = analytics.entry(node_id).or_insert((0, 0.0, 0));
                            entry.0 = idx as u32;
                        }
                    }
                    info!(
                        "Populated node_analytics with {} entries across {} clusters",
                        total_nodes,
                        cluster_results.len()
                    );
                }

                ok_json!(json!({
                    "status": "completed",
                    "taskId": task_id,
                    "algorithm": algorithm,
                    "clusterCount": cluster_results.len(),
                    "clustersFound": cluster_results.len(),
                    "totalNodes": total_nodes,
                    "modularity": (modularity * 1000.0).round() / 1000.0,
                    "avgCoherence": (avg_coherence * 1000.0).round() / 1000.0,
                    "computationTimeMs": computation_time_ms,
                    "gpuAccelerated": true
                }))
            }
            Ok(Err(e)) => {
                error!("GPU clustering failed: {}", e);
                Ok(HttpResponse::InternalServerError().json(json!({
                    "status": "failed",
                    "taskId": task_id,
                    "algorithm": algorithm,
                    "error": format!("GPU clustering failed: {}", e),
                    "gpuAccelerated": false
                })))
            }
            Err(e) => {
                error!("GPU actor communication error: {}", e);
                service_unavailable!("GPU compute actor unavailable")
            }
        }
    } else {
        warn!("GPU compute not available, clustering request cannot be processed");
        service_unavailable!("GPU compute not available")
    }
}

async fn get_clustering_status(
    _req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Clustering status request");

    if let Some(gpu_addr) = state.get_gpu_compute_addr().await {
        use crate::actors::messages::GetClusteringResults;

        match gpu_addr.send(GetClusteringResults).await {
            Ok(Ok(cluster_results)) => {
                
                let algorithm = cluster_results
                    .get("algorithm_used")
                    .and_then(|v| v.as_str())
                    .unwrap_or("adaptive")
                    .to_string();
                let clusters_len = cluster_results
                    .get("clusters")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.len())
                    .unwrap_or(0);
                let modularity = cluster_results
                    .get("modularity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                let computation_time = cluster_results
                    .get("computation_time_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                ok_json!(json!({
                    "status": "completed",
                    "algorithm": algorithm,
                    "progress": 1.0,
                    "clustersFound": clusters_len,
                    "lastRun": chrono::Utc::now().to_rfc3339(),
                    "gpuAvailable": true,
                    "modularity": modularity,
                    "computationTimeMs": computation_time
                }))
            }
            Ok(Err(e)) => {
                info!("No clustering results available: {}", e);
                ok_json!(json!({
                    "status": "idle",
                    "algorithm": "none",
                    "progress": 0.0,
                    "clustersFound": 0,
                    "lastRun": null,
                    "gpuAvailable": true,
                    "note": "No clustering has been performed yet"
                }))
            }
            Err(e) => {
                error!("GPU actor communication error: {}", e);
                service_unavailable!("GPU compute actor unavailable")
            }
        }
    } else {
        ok_json!(json!({
            "status": "unavailable",
            "algorithm": "none",
            "progress": 0.0,
            "clustersFound": 0,
            "lastRun": null,
            "gpuAvailable": false,
            "note": "GPU compute not available"
        }))
    }
}

async fn get_clustering_results(
    _req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    info!("Clustering results request");

    if let Some(gpu_addr) = state.get_gpu_compute_addr().await {
        use crate::actors::messages::{GetClusteringResults, GetGraphData};
use crate::{ok_json, error_json, service_unavailable};



        
        let graph_data = match state.graph_service_addr.send(GetGraphData).await {
            Ok(Ok(data)) => data,
            Ok(Err(e)) => {
                error!("Failed to get graph data: {}", e);
                return error_json!("Failed to get graph data for clustering results");
            }
            Err(e) => {
                error!("Graph service communication error: {}", e);
                return service_unavailable!("Graph service unavailable");
            }
        };

        
        match gpu_addr.send(GetClusteringResults).await {
            Ok(Ok(cluster_results)) => {
                
                let clusters = if let Some(clusters_array) =
                    cluster_results.get("clusters").and_then(|v| v.as_array())
                {
                    clusters_array.iter().map(|cluster| {
                        json!({
                            "id": cluster.get("id").and_then(|v| v.as_u64()).unwrap_or(0),
                            "nodeIds": cluster.get("node_ids").and_then(|v| v.as_array()).unwrap_or(&vec![]),
                            "nodeCount": cluster.get("node_ids").and_then(|v| v.as_array()).map(|arr| arr.len()).unwrap_or(0),
                            "coherence": cluster.get("coherence").and_then(|v| v.as_f64()).unwrap_or(0.5),
                            "centroid": cluster.get("centroid").and_then(|v| v.as_array()).unwrap_or(&vec![]),
                            "keywords": cluster.get("keywords").and_then(|v| v.as_array()).unwrap_or(&vec![serde_json::Value::String("cluster".to_string())])
                        })
                    }).collect::<Vec<_>>()
                } else {
                    vec![]
                };

                
                let algorithm = cluster_results
                    .get("algorithm_used")
                    .and_then(|v| v.as_str())
                    .unwrap_or("adaptive")
                    .to_string();
                let modularity = cluster_results
                    .get("modularity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                let computation_time = cluster_results
                    .get("computation_time_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                ok_json!(json!({
                    "clusters": clusters,
                    "totalNodes": graph_data.nodes.len(),
                    "algorithmUsed": algorithm,
                    "modularity": modularity,
                    "lastUpdated": chrono::Utc::now().to_rfc3339(),
                    "gpuAvailable": true,
                    "computationTimeMs": computation_time,
                    "gpuAccelerated": true
                }))
            }
            Ok(Err(e)) => {
                info!("No clustering results available: {}", e);
                ok_json!(json!({
                    "clusters": [],
                    "totalNodes": graph_data.nodes.len(),
                    "algorithmUsed": "none",
                    "modularity": 0.0,
                    "lastUpdated": chrono::Utc::now().to_rfc3339(),
                    "gpuAvailable": true,
                    "note": "No clustering results available - run clustering first"
                }))
            }
            Err(e) => {
                error!("GPU actor communication error: {}", e);
                service_unavailable!("GPU compute actor unavailable")
            }
        }
    } else {
        ok_json!(json!({
            "clusters": [],
            "totalNodes": 0,
            "algorithmUsed": "none",
            "modularity": 0.0,
            "lastUpdated": chrono::Utc::now().to_rfc3339(),
            "gpuAvailable": false,
            "note": "GPU compute not available"
        }))
    }
}

async fn export_cluster_assignments(
    _req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<Value>,
) -> Result<HttpResponse, actix_web::Error> {
    let export_request = payload.into_inner();

    info!("Cluster assignment export request");
    debug!(
        "Export request: {}",
        serde_json::to_string_pretty(&export_request).unwrap_or_default()
    );

    let format = export_request
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    if !["json", "csv", "graphml"].contains(&format) {
        return bad_request!("format must be 'json', 'csv', or 'graphml'");
    }

    #[cfg(feature = "gpu")]
    if let Some(gpu_addr) = state.get_gpu_compute_addr().await {
        info!("Attempting to get clustering data from GPU compute actor");

        
        match gpu_addr
            .send(crate::actors::messages::GetClusteringResults)
            .await
        {
            Ok(Ok(clustering_results)) => {
                info!("Successfully retrieved clustering results from GPU");

                let export_data = match format {
                    "csv" => {
                        let mut csv_content = "node_id,cluster_id,x,y,z\n".to_string();
                        if let Some(clusters_array) = clustering_results.get("clusters").and_then(|v| v.as_array()) {
                        for cluster in clusters_array {
                            if let Some(node_ids) = cluster.get("node_ids").and_then(|v| v.as_array()) {
                                let cluster_id = cluster.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                                for node_id in node_ids {
                                    if let Some(id) = node_id.as_u64() {
                                        
                                        let position = cluster.get("centroid").and_then(|v| v.as_array())
                                            .map(|arr| (
                                                arr.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0),
                                                arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
                                                arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0)
                                            )).unwrap_or((0.0, 0.0, 0.0));

                                        csv_content.push_str(&format!("{},{},{},{},{}\n",
                                            id, cluster_id, position.0, position.1, position.2));
                                    }
                                }
                            }
                        }
                        } 
                        csv_content
                    },
                    "graphml" => {
                        let mut graphml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_string();
                        graphml.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
                        graphml.push_str("  <key id=\"cluster\" for=\"node\" attr.name=\"cluster\" attr.type=\"int\"/>\n");
                        graphml.push_str("  <graph id=\"clusters\" edgedefault=\"undirected\">\n");

                        if let Some(clusters_array) = clustering_results.get("clusters").and_then(|v| v.as_array()) {
                        for cluster in clusters_array {
                            if let Some(node_ids) = cluster.get("node_ids").and_then(|v| v.as_array()) {
                                let cluster_id = cluster.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                                for node_id in node_ids {
                                    if let Some(id) = node_id.as_u64() {
                                        graphml.push_str(&format!("    <node id=\"{}\">\n", id));
                                        graphml.push_str(&format!("      <data key=\"cluster\">{}</data>\n", cluster_id));
                                        graphml.push_str("    </node>\n");
                                    }
                                }
                            }
                        }
                        } 

                        graphml.push_str("  </graph>\n</graphml>\n");
                        graphml
                    },
                    _ => {
                        json!({
                            "clusters": clustering_results.get("clusters").unwrap_or(&serde_json::Value::Array(vec![])),
                            "algorithm": clustering_results.get("algorithm").unwrap_or(&serde_json::Value::String("unknown".to_string())),
                            "parameters": clustering_results.get("parameters").unwrap_or(&serde_json::Value::Object(serde_json::Map::new())),
                            "performance": clustering_results.get("performance_metrics").unwrap_or(&serde_json::Value::Object(serde_json::Map::new())),
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "data_source": "gpu_compute_actor"
                        }).to_string()
                    }
                };

                let content_type = match format {
                    "csv" => "text/csv",
                    "graphml" => "application/xml",
                    _ => "application/json",
                };

                return Ok(HttpResponse::Ok()
                    .content_type(content_type)
                    .insert_header((
                        "Content-Disposition",
                        format!("attachment; filename=\"clusters.{}\"", format),
                    ))
                    .body(export_data));
            }
            Ok(Err(e)) => {
                warn!("GPU compute actor failed to get clustering results: {}", e);
            }
            Err(e) => {
                warn!("Failed to communicate with GPU compute actor: {}", e);
            }
        }
    }

    
    match state
        .graph_service_addr
        .send(crate::actors::messages::GetGraphData)
        .await
    {
        Ok(Ok(graph_data)) => {
            if !graph_data.nodes.is_empty() {
                info!(
                    "Using graph data for clustering export with {} nodes",
                    graph_data.nodes.len()
                );

                
                let mut clusters = HashMap::new();
                for node in &graph_data.nodes {
                    
                    let cluster_key = node
                        .node_type
                        .as_ref()
                        .or(node.group.as_ref())
                        .cloned()
                        .unwrap_or_else(|| "default".to_string());

                    clusters
                        .entry(cluster_key)
                        .or_insert_with(Vec::new)
                        .push(node.id);
                }

                let export_data = match format {
                    "csv" => {
                        let mut csv_content = "node_id,cluster_id\n".to_string();
                        for (cluster_name, node_ids) in clusters {
                            let cluster_id =
                                cluster_name.chars().map(|c| c as u32).sum::<u32>() % 100;
                            for node_id in node_ids {
                                csv_content.push_str(&format!("{},{}\n", node_id, cluster_id));
                            }
                        }
                        csv_content
                    }
                    "graphml" => {
                        let mut graphml =
                            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_string();
                        graphml.push_str(
                            "<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n",
                        );
                        graphml.push_str("  <key id=\"cluster\" for=\"node\" attr.name=\"cluster\" attr.type=\"string\"/>\n");
                        graphml.push_str("  <graph id=\"clusters\" edgedefault=\"undirected\">\n");

                        for (cluster_name, node_ids) in clusters {
                            for node_id in node_ids {
                                graphml.push_str(&format!("    <node id=\"{}\">\n", node_id));
                                graphml.push_str(&format!(
                                    "      <data key=\"cluster\">{}</data>\n",
                                    cluster_name
                                ));
                                graphml.push_str("    </node>\n");
                            }
                        }

                        graphml.push_str("  </graph>\n</graphml>\n");
                        graphml
                    }
                    _ => {
                        let cluster_objects: Vec<serde_json::Value> = clusters
                            .into_iter()
                            .enumerate()
                            .map(|(idx, (name, nodes))| {
                                json!({
                                    "id": idx,
                                    "name": name,
                                    "node_ids": nodes,
                                    "size": nodes.len()
                                })
                            })
                            .collect();

                        json!({
                            "clusters": cluster_objects,
                            "algorithm": "metadata_based",
                            "node_count": graph_data.nodes.len(),
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "data_source": "graph_service_metadata"
                        })
                        .to_string()
                    }
                };

                let content_type = match format {
                    "csv" => "text/csv",
                    "graphml" => "application/xml",
                    _ => "application/json",
                };

                return Ok(HttpResponse::Ok()
                    .content_type(content_type)
                    .insert_header((
                        "Content-Disposition",
                        format!("attachment; filename=\"clusters.{}\"", format),
                    ))
                    .body(export_data));
            }
        }
        _ => {
            warn!("Failed to get graph data for clustering export");
        }
    }

    
    let empty_response = match format {
        "csv" => "# No clustering data available\n# Try running clustering analysis first\nnode_id,cluster_id\n".to_string(),
        "graphml" => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- No clustering data available. Try running clustering analysis first. -->\n<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n  <graph id=\"empty\" edgedefault=\"undirected\">\n  </graph>\n</graphml>\n"
        ),
        _ => json!({
            "clusters": [],
            "message": "No clustering data available",
            "suggestions": [
                "Run clustering analysis first with POST /clustering/analyze",
                "Ensure graph data is loaded",
                "Check GPU compute actor status"
            ],
            "gpu_available": cfg!(feature = "gpu") && {
                #[cfg(feature = "gpu")]
                { state.try_get_gpu_compute_addr().is_some() }
                #[cfg(not(feature = "gpu"))]
                { false }
            },
            "timestamp": chrono::Utc::now().to_rfc3339()
        }).to_string(),
    };

    let content_type = match format {
        "csv" => "text/csv",
        "graphml" => "application/xml",
        _ => "application/json",
    };

    Ok(HttpResponse::Ok()
        .content_type(content_type)
        .body(empty_response))
}

/// POST /clustering/dbscan
/// Runs standalone DBSCAN clustering on the GPU.
///
/// Request body:
/// ```json
/// { "epsilon": 0.5, "minPoints": 5 }
/// ```
async fn run_dbscan(
    _req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<Value>,
) -> Result<HttpResponse, actix_web::Error> {
    let request = payload.into_inner();

    let epsilon = request
        .get("epsilon")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(0.5);

    let min_points = request
        .get("minPoints")
        .or_else(|| request.get("min_points"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(5);

    if epsilon <= 0.0 {
        return bad_request!("epsilon must be positive");
    }
    if min_points == 0 {
        return bad_request!("minPoints must be at least 1");
    }

    info!(
        "Starting DBSCAN clustering with epsilon={}, minPoints={}",
        epsilon, min_points
    );

    let start = Instant::now();

    if let Some(gpu_manager) = state.gpu_manager_addr.as_ref() {
        use crate::actors::messages::RunDBSCAN;
        use crate::actors::messages::DBSCANParams;

        let msg = RunDBSCAN {
            params: DBSCANParams {
                epsilon,
                min_points,
            },
        };

        match gpu_manager.send(msg).await {
            Ok(Ok(result)) => {
                let computation_time_ms = start.elapsed().as_millis() as u64;

                // Populate shared node_analytics for V3 wire format
                if let Ok(mut analytics) = state.node_analytics.write() {
                    for cluster in &result.clusters {
                        for &node_id in &cluster.nodes {
                            let entry = analytics.entry(node_id).or_insert((0, 0.0, 0));
                            // Use the cluster label from the DBSCAN result
                            entry.0 = cluster.label.split_whitespace()
                                .last()
                                .and_then(|s| s.parse::<u32>().ok())
                                .unwrap_or(0);
                        }
                    }
                }

                ok_json!(json!({
                    "status": "completed",
                    "algorithm": "dbscan",
                    "epsilon": epsilon,
                    "minPoints": min_points,
                    "numClusters": result.num_clusters,
                    "numNoisePoints": result.num_noise_points,
                    "totalNodes": result.stats.total_nodes,
                    "clusters": result.clusters.iter().map(|c| json!({
                        "id": c.id,
                        "label": c.label,
                        "nodeCount": c.node_count,
                        "coherence": c.coherence,
                        "nodes": c.nodes,
                        "centroid": c.centroid,
                        "color": c.color,
                        "keywords": c.keywords,
                    })).collect::<Vec<_>>(),
                    "stats": {
                        "totalNodes": result.stats.total_nodes,
                        "numClusters": result.stats.num_clusters,
                        "numNoisePoints": result.stats.num_noise_points,
                        "largestClusterSize": result.stats.largest_cluster_size,
                        "smallestClusterSize": result.stats.smallest_cluster_size,
                        "averageClusterSize": result.stats.average_cluster_size,
                        "computationTimeMs": computation_time_ms
                    },
                    "gpuAccelerated": true,
                    "computationTimeMs": computation_time_ms
                }))
            }
            Ok(Err(e)) => {
                error!("DBSCAN clustering failed: {}", e);
                Ok(HttpResponse::InternalServerError().json(json!({
                    "status": "failed",
                    "algorithm": "dbscan",
                    "error": format!("DBSCAN clustering failed: {}", e),
                    "gpuAccelerated": false
                })))
            }
            Err(e) => {
                error!("GPU actor communication error: {}", e);
                service_unavailable!("GPU compute actor unavailable")
            }
        }
    } else {
        warn!("GPU compute not available for DBSCAN clustering");
        service_unavailable!("GPU compute not available")
    }
}

fn validate_clustering_config(config: &ClusteringConfiguration) -> Result<(), String> {
    
    if ![
        "none",
        "kmeans",
        "spectral",
        "louvain",
        "hierarchical",
        "dbscan",
    ]
    .contains(&config.algorithm.as_str())
    {
        return Err("algorithm must be 'none', 'kmeans', 'spectral', 'louvain', 'hierarchical', or 'dbscan'".to_string());
    }

    
    if config.num_clusters < 2 || config.num_clusters > 50 {
        return Err("num_clusters must be between 2 and 50".to_string());
    }

    
    if config.resolution < 0.1 || config.resolution > 5.0 {
        return Err("resolution must be between 0.1 and 5.0".to_string());
    }

    
    if config.iterations < 10 || config.iterations > 1000 {
        return Err("iterations must be between 10 and 1000".to_string());
    }

    Ok(())
}
