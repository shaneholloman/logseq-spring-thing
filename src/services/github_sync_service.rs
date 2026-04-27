// src/services/github_sync_service.rs
//! GitHub Sync Service
//!
//! Synchronizes markdown files from GitHub repository to Neo4j.
//! - Supports dual-graph import via `GITHUB_BASE_PATHS` (comma-separated)
//!   e.g. `"mainKnowledgeGraph/pages,workingGraph/pages"`.
//!   Falls back to single `GITHUB_BASE_PATH` for backward compat.
//! - Tags each node with `graph_source` (first path segment, e.g. `"mainKnowledgeGraph"`)
//! - Detects v2 format (`iri::` + `rdf-type:: owl:Class`) via `FileFormat::VisionClawV2`
//! - Parses public:: true pages as knowledge graph nodes (KnowledgeGraphRepository - Neo4jAdapter)
//! - Extracts OntologyBlock sections as OWL data (Neo4jOntologyRepository)
//! - Enriches graph nodes with owl_class_iri metadata via OntologyEnrichmentService
//! - Triggers OntologyPipelineService for automatic reasoning and constraint generation
//! - Uses SHA1 filtering to process only changed files (unless FORCE_FULL_SYNC=1)
//! - Batch processing (50 files) to avoid memory issues with large repositories
//! - Selective data wipe: only clears Neo4j data for graph sources removed from config

use crate::adapters::neo4j_ontology_repository::Neo4jOntologyRepository;
use crate::ports::knowledge_graph_repository::KnowledgeGraphRepository;
use crate::ports::ontology_repository::OntologyRepository;
use crate::services::github::config::GitHubConfig;
use crate::services::github::content_enhanced::EnhancedContentAPI;
use crate::services::github::types::GitHubFileBasicMetadata;
use crate::services::parsers::{KnowledgeGraphParser, OntologyParser};
use crate::services::ontology_enrichment_service::OntologyEnrichmentService;
use crate::services::ontology_reasoner::OntologyReasoner;
use crate::services::edge_classifier::EdgeClassifier;
use crate::services::ontology_pipeline_service::OntologyPipelineService;
use crate::services::ingest_saga::{saga_enabled, serialise_node_for_pod, IngestSaga, NodeSagaPlan};
use crate::adapters::whelk_inference_engine::WhelkInferenceEngine;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::{Duration, Instant};

const BATCH_SIZE: usize = 50; // Save to database every 50 files

#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    KnowledgeGraph,
    Ontology,
    Skip,
}

/// Detected file format used for routing through the correct parser pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum FileFormat {
    /// VisionClaw v2 format: starts with `iri::` and contains `rdf-type:: owl:Class`
    VisionClawV2,
    /// Legacy OntologyBlock v4 format: contains `### OntologyBlock`
    OntologyV4,
    /// Public Logseq page: `public:: true` in the first 25 lines
    PublicNote,
    /// Private/non-public note (default fallback)
    PrivateNote,
}

#[derive(Debug, Clone)]
pub struct SyncStatistics {
    pub total_files: usize,
    pub kg_files_processed: usize,
    pub ontology_files_processed: usize,
    pub skipped_files: usize,
    pub errors: Vec<String>,
    pub duration: Duration,
    pub total_nodes: usize,
    pub total_edges: usize,
}

pub struct GitHubSyncService {
    content_api: Arc<EnhancedContentAPI>,
    kg_parser: Arc<KnowledgeGraphParser>,
    onto_parser: Arc<OntologyParser>,
    kg_repo: Arc<dyn KnowledgeGraphRepository>,
    onto_repo: Arc<Neo4jOntologyRepository>,
    enrichment_service: Arc<OntologyEnrichmentService>,
    pipeline_service: Option<Arc<OntologyPipelineService>>,
    /// Pod-first-Neo4j-second saga (ADR-051). `None` → legacy path.
    saga: Option<Arc<IngestSaga>>,
}

impl GitHubSyncService {
    pub fn new(
        content_api: Arc<EnhancedContentAPI>,
        kg_repo: Arc<dyn KnowledgeGraphRepository>,
        onto_repo: Arc<Neo4jOntologyRepository>,
    ) -> Self {
        // Initialize ontology enrichment service
        let inference_engine = Arc::new(WhelkInferenceEngine::new());
        let reasoner = Arc::new(OntologyReasoner::new(
            inference_engine,
            onto_repo.clone() as Arc<dyn OntologyRepository>,
        ));
        let classifier = Arc::new(EdgeClassifier::new());
        let enrichment_service = Arc::new(OntologyEnrichmentService::new(
            reasoner,
            classifier,
        ));

        Self {
            content_api,
            kg_parser: Arc::new(KnowledgeGraphParser::new()),
            onto_parser: Arc::new(OntologyParser::new()),
            kg_repo,
            onto_repo,
            enrichment_service,
            pipeline_service: None,
            saga: None,
        }
    }

    /// Set the ontology pipeline service for automatic reasoning
    pub fn set_pipeline_service(&mut self, pipeline: Arc<OntologyPipelineService>) {
        info!("GitHubSyncService: Ontology pipeline service registered");
        self.pipeline_service = Some(pipeline);
    }

    /// Wire the Pod-first-Neo4j-second saga. When set, batch ingest routes
    /// through the saga (Pod write → Neo4j commit) instead of the legacy
    /// Neo4j-only path. Feature-flag-gated by `POD_SAGA_ENABLED`.
    pub fn set_saga(&mut self, saga: Arc<IngestSaga>) {
        info!("GitHubSyncService: IngestSaga registered (ADR-051)");
        self.saga = Some(saga);
    }

    /// Synchronize graphs from GitHub - processes in batches with progress logging.
    ///
    /// Supports dual-graph import via `GITHUB_BASE_PATHS` (comma-separated).
    /// Falls back to single `GITHUB_BASE_PATH` for backward compatibility.
    /// Each base path is tagged with a `graph_source` derived from the first
    /// path segment (e.g. `mainKnowledgeGraph/pages` -> `"mainKnowledgeGraph"`).
    pub async fn sync_graphs(&self) -> Result<SyncStatistics, String> {
        info!("Starting GitHub sync (batch size: {})", BATCH_SIZE);
        let start_time = Instant::now();

        let mut stats = SyncStatistics {
            total_files: 0,
            kg_files_processed: 0,
            ontology_files_processed: 0,
            skipped_files: 0,
            errors: Vec::new(),
            duration: Duration::from_secs(0),
            total_nodes: 0,
            total_edges: 0,
        };

        // Resolve base paths (dual-graph or single)
        let base_paths = self.resolve_base_paths();
        info!("Sync targets: {:?}", base_paths);

        // Detect base path change — clear stale Neo4j data for removed graph sources
        let any_path_changed = self.detect_and_handle_base_paths_change(&base_paths).await;

        let force_full_sync = any_path_changed || std::env::var("FORCE_FULL_SYNC")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        let mut all_files_to_update: Vec<GitHubFileBasicMetadata> = Vec::new();

        for base_path in &base_paths {
            let graph_source = GitHubConfig::graph_source_for_path(base_path);
            info!("Syncing graph source '{}' from base path '{}'", graph_source, base_path);

            // Fetch files for this base path
            let files = match self.fetch_markdown_files_for_path(base_path).await {
                Ok(files) => {
                    info!("Found {} markdown files in '{}'", files.len(), base_path);
                    files
                }
                Err(e) => {
                    let error_msg = format!("Failed to fetch files from '{}': {}", base_path, e);
                    error!("{}", error_msg);
                    stats.errors.push(error_msg);
                    continue;
                }
            };

            stats.total_files += files.len();

            // SHA1 filtering
            let files_to_process = if force_full_sync {
                info!("Full sync required for '{}' — processing ALL {} files", base_path, files.len());
                files.clone()
            } else {
                match self.filter_changed_files(&files).await {
                    Ok(filtered) => {
                        info!("Processing {} changed files ({} unchanged) in '{}'",
                            filtered.len(), files.len() - filtered.len(), base_path);
                        stats.skipped_files += files.len() - filtered.len();
                        filtered
                    }
                    Err(e) => {
                        error!("SHA1 filter failed for '{}': {}", base_path, e);
                        files.clone()
                    }
                }
            };

            all_files_to_update.extend(files_to_process.clone());

            // Process in batches with graph_source tagging
            for (batch_idx, batch) in files_to_process.chunks(BATCH_SIZE).enumerate() {
                let batch_start = Instant::now();
                let total_batches = (files_to_process.len() + BATCH_SIZE - 1) / BATCH_SIZE;
                info!("Processing batch {}/{} ({} files) for graph '{}'",
                    batch_idx + 1, total_batches, batch.len(), graph_source);

                match self.process_batch_with_source(batch, &graph_source, &mut stats).await {
                    Ok(_) => {
                        info!("Batch {} for '{}' completed in {:?}",
                            batch_idx + 1, graph_source, batch_start.elapsed());
                    }
                    Err(e) => {
                        error!("Batch {} for '{}' failed: {}", batch_idx + 1, graph_source, e);
                        stats.errors.push(format!("Batch {} [{}]: {}", batch_idx + 1, graph_source, e));
                    }
                }
            }
        }

        // Update metadata
        if let Err(e) = self.update_file_metadata(&all_files_to_update).await {
            warn!("Failed to update file_metadata: {}", e);
        }

        stats.duration = start_time.elapsed();
        info!("Sync complete: {} nodes, {} edges in {:?}",
            stats.total_nodes, stats.total_edges, stats.duration);

        Ok(stats)
    }

    /// Resolve the list of base paths from env vars.
    /// `GITHUB_BASE_PATHS` (comma-separated) takes precedence.
    /// Falls back to `GITHUB_BASE_PATH` for backward compat.
    fn resolve_base_paths(&self) -> Vec<String> {
        if let Ok(paths) = std::env::var("GITHUB_BASE_PATHS") {
            let multi: Vec<String> = paths
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !multi.is_empty() {
                return multi;
            }
        }
        // Fallback to single GITHUB_BASE_PATH (already loaded by GitHubClient)
        let single = std::env::var("GITHUB_BASE_PATH").unwrap_or_default();
        if single.is_empty() {
            vec![]
        } else {
            vec![single]
        }
    }

    /// Fetch markdown files for a specific base path.
    /// Uses the Trees API (single call) filtered to the given path prefix,
    /// falling back to the recursive Contents API.
    async fn fetch_markdown_files_for_path(
        &self,
        base_path: &str,
    ) -> Result<Vec<GitHubFileBasicMetadata>, String> {
        // Try the Trees API first — it returns the entire repo tree; we filter client-side
        match self.content_api.list_markdown_files_via_tree_for_path(base_path).await {
            Ok(files) => {
                info!("Trees API returned {} markdown files for '{}'", files.len(), base_path);
                Ok(files)
            }
            Err(e) => {
                warn!("Trees API failed for '{}' ({}), falling back to Contents API", base_path, e);
                self.content_api
                    .list_markdown_files(base_path)
                    .await
                    .map_err(|e| format!("GitHub API error for '{}': {}", base_path, e))
            }
        }
    }

    /// Process a batch of files with graph source tagging.
    /// Wraps `process_batch` and stamps `graph_source` on every node produced.
    async fn process_batch_with_source(
        &self,
        files: &[GitHubFileBasicMetadata],
        graph_source: &str,
        stats: &mut SyncStatistics,
    ) -> Result<(), String> {
        self.process_batch(files, graph_source, stats).await
    }

    /// Process a batch of files with parallel content fetching
    /// Uses FuturesUnordered to fetch file contents in parallel (the main I/O bottleneck)
    /// while processing/parsing sequentially to maintain state consistency.
    async fn process_batch(
        &self,
        files: &[GitHubFileBasicMetadata],
        graph_source: &str,
        stats: &mut SyncStatistics,
    ) -> Result<(), String> {
        let mut batch_nodes = std::collections::HashMap::new();
        let mut batch_edges = std::collections::HashMap::new();
        let mut public_pages = std::collections::HashSet::new();

        info!("🔍 [DEBUG] Starting batch with {} files (parallel fetch)", files.len());

        // Phase 1: Fetch all file contents in parallel
        const PARALLEL_FETCHES: usize = 8;

        // Helper to create fetch future with consistent type
        fn create_fetch_future(
            content_api: Arc<EnhancedContentAPI>,
            file: GitHubFileBasicMetadata,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = (GitHubFileBasicMetadata, Result<String, String>)> + Send>> {
            let download_url = file.download_url.clone();
            Box::pin(async move {
                let result = content_api.fetch_file_content(&download_url).await
                    .map_err(|e| format!("Failed to fetch content: {}", e));
                (file, result)
            })
        }

        let mut fetch_futures: FuturesUnordered<_> = FuturesUnordered::new();
        let mut fetched_contents: Vec<(GitHubFileBasicMetadata, Result<String, String>)> = Vec::with_capacity(files.len());
        let mut file_iter = files.iter().cloned().peekable();

        // Seed initial batch of parallel fetches
        while fetch_futures.len() < PARALLEL_FETCHES {
            if let Some(file) = file_iter.next() {
                fetch_futures.push(create_fetch_future(Arc::clone(&self.content_api), file));
            } else {
                break;
            }
        }

        // Collect all fetched contents
        while let Some((file, content_result)) = fetch_futures.next().await {
            fetched_contents.push((file, content_result));

            // Add next file to maintain parallelism
            if let Some(file) = file_iter.next() {
                fetch_futures.push(create_fetch_future(Arc::clone(&self.content_api), file));
            }
        }

        info!("🔍 [DEBUG] Fetched {} files, now processing", fetched_contents.len());

        // Phase 2: Process fetched contents sequentially (modifies shared state)
        for (idx, (file, content_result)) in fetched_contents.into_iter().enumerate() {
            if idx % 10 == 0 && idx > 0 {
                info!("  Progress: {}/{} files in batch (nodes so far: {}, edges: {})",
                    idx, files.len(), batch_nodes.len(), batch_edges.len());
            }

            match content_result {
                Ok(content) => {
                    match self.process_fetched_file(&file, &content, &mut batch_nodes, &mut batch_edges, &mut public_pages).await {
                        Ok(()) => {
                            stats.kg_files_processed += 1;
                            debug!("✓ Processed {}: {} nodes total, {} edges total",
                                file.name, batch_nodes.len(), batch_edges.len());
                        }
                        Err(e) => {
                            warn!("Error processing {}: {}", file.name, e);
                            stats.errors.push(format!("{}: {}", file.name, e));
                        }
                    }
                }
                Err(e) => {
                    warn!("Error fetching {}: {}", file.name, e);
                    stats.errors.push(format!("{}: {}", file.name, e));
                }
            }
        }

        info!("🔍 [DEBUG] After processing: {} nodes, {} edges, {} public_pages",
            batch_nodes.len(), batch_edges.len(), public_pages.len());

        // Don't filter nodes/edges - save everything to maintain graph connectivity
        // Edge cross-references between batches should be preserved
        // let nodes_before_filter = batch_nodes.len();
        // self.filter_linked_pages(&mut batch_nodes, &public_pages);
        // info!("🔍 [DEBUG] After filter_linked_pages: {} nodes (removed {})",
        //     batch_nodes.len(), nodes_before_filter - batch_nodes.len());

        // let edges_before_filter = batch_edges.len();
        // self.filter_orphan_edges(&mut batch_edges, &batch_nodes);
        // info!("🔍 [DEBUG] After filter_orphan_edges: {} edges (removed {})",
        //     batch_edges.len(), edges_before_filter - batch_edges.len());

        // Stamp graph_source on every node in this batch
        if !graph_source.is_empty() {
            for node in batch_nodes.values_mut() {
                node.graph_source = Some(graph_source.to_string());
                node.metadata.insert("graph_source".to_string(), graph_source.to_string());
            }
        }

        // Save batch to database
        if !batch_nodes.is_empty() {
            let node_vec: Vec<_> = batch_nodes.into_values().collect();
            let edge_vec: Vec<_> = batch_edges.into_values().collect();

            stats.total_nodes += node_vec.len();
            stats.total_edges += edge_vec.len();

            info!("💾 Saving batch: {} nodes, {} edges", node_vec.len(), edge_vec.len());

            // ADR-051: Pod-first-Neo4j-second saga
            // When the saga is wired AND POD_SAGA_ENABLED=true, route nodes
            // through PodClient → IngestSaga so Pod content is written before
            // the Neo4j commit. Edges always go through the legacy path —
            // they are cheap cross-references that can be re-derived.
            let use_saga = self.saga.is_some() && saga_enabled();

            if use_saga {
                let saga = self.saga.as_ref().expect("saga is Some").clone();
                let plans: Vec<NodeSagaPlan> = node_vec.iter().map(|n| {
                    let pod_url = saga.default_pod_url_for(n);
                    NodeSagaPlan {
                        node: n.clone(),
                        pod_url,
                        content: serialise_node_for_pod(n),
                        content_type: "application/json".to_string(),
                        auth_header: None, // server-Nostr signing path
                    }
                }).collect();

                info!("🔐 [saga] Executing Pod-first-Neo4j-second saga for {} nodes", plans.len());
                let result = saga.execute_batch(plans).await;
                info!(
                    "🔐 [saga] Result: {} complete, {} pending, {} failed (duration: {:?})",
                    result.complete.len(), result.pending.len(), result.failed.len(), result.duration
                );
                if !result.pending.is_empty() {
                    for (node_id, err) in &result.pending {
                        stats.errors.push(format!("Saga pending node {}: {}", node_id, err));
                    }
                }
                if !result.failed.is_empty() {
                    for (node_id, err) in &result.failed {
                        stats.errors.push(format!("Saga failed node {}: {}", node_id, err));
                    }
                }

                // Edges are separate — still commit them through the legacy
                // path. Orphaned edges (target never landed) will be filtered
                // by downstream graph consumers.
                if !edge_vec.is_empty() {
                    let mut edge_graph = crate::models::graph::GraphData::new();
                    edge_graph.edges = edge_vec;
                    if let Err(e) = self.kg_repo.save_graph(&edge_graph).await {
                        warn!("[saga] Edge-only save_graph failed: {}", e);
                        stats.errors.push(format!("Edge commit: {}", e));
                    }
                }
            } else {
                let mut graph = crate::models::graph::GraphData::new();
                graph.nodes = node_vec;
                graph.edges = edge_vec;

                info!("🔍 [DEBUG] Calling save_graph() with {} nodes, {} edges",
                    graph.nodes.len(), graph.edges.len());

                self.kg_repo.save_graph(&graph).await.map_err(|e| {
                    error!("❌ save_graph() failed: {}", e);
                    format!("Failed to save batch: {}", e)
                })?;

                info!("✅ [DEBUG] save_graph() completed successfully");
            }
        } else {
            warn!("⚠️ [DEBUG] Batch is EMPTY after filtering - nothing to save!");
        }

        Ok(())
    }

    /// Process a file with pre-fetched content (used by parallel batch processing)
    async fn process_fetched_file(
        &self,
        file: &GitHubFileBasicMetadata,
        content: &str,
        nodes: &mut std::collections::HashMap<u32, crate::models::node::Node>,
        edges: &mut std::collections::HashMap<String, crate::models::edge::Edge>,
        public_pages: &mut std::collections::HashSet<String>,
    ) -> Result<(), String> {
        debug!("🔍 Processing file: {}", file.name);
        debug!("🔍 Content size: {} bytes for {}", content.len(), file.name);

        // Detect file type
        let file_type = self.detect_file_type(&content);
        debug!("🔍 Detected file type: {:?} for {}", file_type, file.name);

        let page_name = file.name.trim_end_matches(".md");

        match file_type {
            FileType::KnowledgeGraph => {
                // Process public:: true files as knowledge graph nodes
                debug!("🔍 Parsing knowledge graph from {}", file.name);
                let mut parsed = self.kg_parser.parse(&content, &file.name)
                    .map_err(|e| format!("Parse error: {}", e))?;

                info!("📊 Parsed {}: {} nodes, {} edges",
                    file.name, parsed.nodes.len(), parsed.edges.len());

                // ✅ ENRICH WITH ONTOLOGY DATA
                debug!("🦉 Enriching graph with ontology data for {}", file.name);
                match self.enrichment_service.enrich_graph(&mut parsed, &file.path, &content).await {
                    Ok((nodes_enriched, edges_enriched)) => {
                        debug!("✅ Enriched {}: {} nodes with owl_class_iri, {} edges with owl_property_iri",
                            file.name, nodes_enriched, edges_enriched);
                    }
                    Err(e) => {
                        warn!("⚠️  Failed to enrich {}: {} (continuing with unenriched data)", file.name, e);
                    }
                }

                // Add to public pages
                public_pages.insert(page_name.to_string());
                debug!("✓ Added '{}' to public_pages (total: {})", page_name, public_pages.len());

                // Add nodes from KG parser
                let nodes_before = nodes.len();
                for node in parsed.nodes {
                    debug!("  → Node {}: {} (type: {:?})",
                        node.id, node.label,
                        node.metadata.get("type"));
                    nodes.insert(node.id, node);
                }
                let kg_nodes_added = nodes.len() - nodes_before;
                info!("✓ Added {} KG nodes from {} (total now: {})",
                    kg_nodes_added, file.name, nodes.len());

                // Add edges from KG parser
                let edges_before = edges.len();
                for edge in parsed.edges {
                    edges.insert(edge.id.clone(), edge);
                }
                if edges.len() > edges_before {
                    debug!("✓ Added {} edges from {}", edges.len() - edges_before, file.name);
                }

                // Also check for and parse ontology blocks in this file
                if content.contains("### OntologyBlock") {
                    debug!("🦉 Detected OntologyBlock in {}, extracting ontology data", file.name);

                    // Use parse_enhanced to get the full OntologyBlock with relationships
                    match self.onto_parser.parse_enhanced(&content, &file.name) {
                        Ok(block) => {
                            // Convert OntologyBlock relationships into graph edges.
                            // The source is this page's node; targets are resolved via page_name_to_id.
                            let source_id = self.kg_parser.page_name_to_id(page_name);

                            let relationship_types: Vec<(&str, &[String], f32, &str)> = vec![
                                ("hierarchical",  &block.is_subclass_of, 2.5, "rdfs:subClassOf"),
                                ("structural",    &block.has_part,       1.5, "mv:hasPart"),
                                ("structural",    &block.is_part_of,     1.5, "mv:isPartOf"),
                                ("dependency",    &block.requires,       1.5, "mv:requires"),
                                ("dependency",    &block.depends_on,     1.5, "mv:dependsOn"),
                                ("dependency",    &block.enables,        1.5, "mv:enables"),
                                ("associative",   &block.relates_to,     1.0, "mv:relatedTo"),
                                ("bridge",        &block.bridges_to,     1.0, "mv:bridgesTo"),
                                ("bridge",        &block.bridges_from,   1.0, "mv:bridgesFrom"),
                            ];

                            let mut onto_edges_added = 0usize;
                            for (edge_type, targets, weight, owl_iri) in &relationship_types {
                                for target_name in *targets {
                                    // Strip [[...]] brackets if present
                                    let clean_name = target_name
                                        .trim_start_matches("[[")
                                        .trim_end_matches("]]")
                                        .trim();
                                    if clean_name.is_empty() { continue; }

                                    let target_id = self.kg_parser.page_name_to_id(clean_name);
                                    // Avoid self-loops
                                    if target_id == source_id { continue; }

                                    let edge_id = format!("{}_{}_{}",
                                        source_id, target_id, edge_type);
                                    let edge = crate::models::edge::Edge {
                                        id: edge_id.clone(),
                                        source: source_id,
                                        target: target_id,
                                        weight: *weight,
                                        edge_type: Some(edge_type.to_string()),
                                        owl_property_iri: Some(owl_iri.to_string()),
                                        metadata: None,
                                    };
                                    edges.insert(edge_id, edge);
                                    onto_edges_added += 1;
                                }
                            }

                            // Also create edges from other_relationships
                            for (rel_name, targets) in &block.other_relationships {
                                for target_name in targets {
                                    let clean_name = target_name
                                        .trim_start_matches("[[")
                                        .trim_end_matches("]]")
                                        .trim();
                                    if clean_name.is_empty() { continue; }

                                    let target_id = self.kg_parser.page_name_to_id(clean_name);
                                    if target_id == source_id { continue; }

                                    let edge_id = format!("{}_{}_{}",
                                        source_id, target_id, rel_name);
                                    let edge = crate::models::edge::Edge {
                                        id: edge_id.clone(),
                                        source: source_id,
                                        target: target_id,
                                        weight: 1.0,
                                        edge_type: Some("associative".to_string()),
                                        owl_property_iri: Some(
                                            format!("mv:{}", rel_name)),
                                        metadata: None,
                                    };
                                    edges.insert(edge_id, edge);
                                    onto_edges_added += 1;
                                }
                            }

                            if onto_edges_added > 0 {
                                let total_rels = block.is_subclass_of.len()
                                    + block.has_part.len() + block.is_part_of.len()
                                    + block.requires.len() + block.depends_on.len()
                                    + block.enables.len() + block.relates_to.len()
                                    + block.bridges_to.len() + block.bridges_from.len()
                                    + block.other_relationships.values()
                                        .map(|v| v.len()).sum::<usize>();
                                debug!("🔗 Created {} ontology edges from {} relationships in {}",
                                    onto_edges_added, total_rels, file.name);
                            }

                            // Also save ontology data to Neo4j (legacy path)
                            match self.onto_parser.parse(&content, &file.name) {
                                Ok(onto_data) => {
                                    debug!("🦉 Extracted from {}: {} classes, {} properties, {} axioms",
                                        file.name,
                                        onto_data.classes.len(),
                                        onto_data.properties.len(),
                                        onto_data.axioms.len());
                                    if let Err(e) = self.save_ontology_data(onto_data).await {
                                        error!("Failed to save ontology data from {}: {}", file.name, e);
                                    }
                                }
                                Err(e) => {
                                    debug!("Failed to convert ontology block to data: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            debug!("Failed to parse ontology block in {}: {}", file.name, e);
                            // Fallback: try legacy parse
                            if let Ok(onto_data) = self.onto_parser.parse(&content, &file.name) {
                                if let Err(e2) = self.save_ontology_data(onto_data).await {
                                    error!("Failed to save ontology data from {}: {}", file.name, e2);
                                }
                            }
                        }
                    }
                }

                Ok(())
            }
            FileType::Ontology => {
                // Process files with ontology blocks
                debug!("🦉 Processing ontology file {}", file.name);
                match self.onto_parser.parse(&content, &file.name) {
                    Ok(onto_data) => {
                        debug!("🦉 Extracted from {}: {} classes, {} properties, {} axioms",
                            file.name,
                            onto_data.classes.len(),
                            onto_data.properties.len(),
                            onto_data.axioms.len());

                        // Save ontology data immediately
                        if let Err(e) = self.save_ontology_data(onto_data).await {
                            error!("Failed to save ontology data from {}: {}", file.name, e);
                        } else {
                            debug!("✓ Saved ontology data from {}", file.name);
                        }
                    }
                    Err(e) => {
                        debug!("Failed to parse ontology file {}: {}", file.name, e);
                    }
                }
                Ok(())
            }
            FileType::Skip => {
                // Skip regular notes without public:: true or ontology blocks
                debug!("⏭️  Skipped regular note: {} (no public:: true or ontology block)", file.name);
                Ok(())
            }
        }
    }

    /// Filter linked pages
    #[allow(dead_code)]
    fn filter_linked_pages(
        &self,
        nodes: &mut std::collections::HashMap<u32, crate::models::node::Node>,
        public_pages: &std::collections::HashSet<String>,
    ) {
        let before = nodes.len();
        nodes.retain(|_, node| {
            match node.metadata.get("type").map(|s| s.as_str()) {
                Some("page") => true,
                Some("linked_page") => public_pages.contains(&node.metadata_id),
                _ => true,
            }
        });
        let filtered = before - nodes.len();
        if filtered > 0 {
            info!("🔍 Filtered {} linked_page nodes", filtered);
        }
    }

    /// Filter orphan edges
    #[allow(dead_code)]
    fn filter_orphan_edges(
        &self,
        edges: &mut std::collections::HashMap<String, crate::models::edge::Edge>,
        nodes: &std::collections::HashMap<u32, crate::models::node::Node>,
    ) {
        let before = edges.len();
        edges.retain(|_, edge| {
            nodes.contains_key(&edge.source) && nodes.contains_key(&edge.target)
        });
        let filtered = before - edges.len();
        if filtered > 0 {
            info!("🔍 Filtered {} orphan edges", filtered);
        }
    }

    /// SHA1-based filtering
    async fn filter_changed_files(
        &self,
        files: &[GitHubFileBasicMetadata],
    ) -> Result<Vec<GitHubFileBasicMetadata>, String> {
        let existing = self.get_existing_file_metadata().await?;

        Ok(files
            .iter()
            .filter(|file| {
                match existing.get(&file.name) {
                    Some(existing_sha) if existing_sha == &file.sha => false,
                    _ => true,
                }
            })
            .cloned()
            .collect())
    }

    /// Fetch all markdown files using Git Trees API (single API call).
    /// Falls back to recursive Contents API if Trees API fails.
    /// Retained for backward compat; prefer `fetch_markdown_files_for_path`.
    #[allow(dead_code)]
    async fn fetch_all_markdown_files(&self) -> Result<Vec<GitHubFileBasicMetadata>, String> {
        // Try the Trees API first — single call returns all file SHAs
        match self.content_api.list_markdown_files_via_tree().await {
            Ok(files) => {
                info!("📂 Trees API returned {} markdown files in a single call", files.len());
                Ok(files)
            }
            Err(e) => {
                warn!("Trees API failed ({}), falling back to recursive Contents API", e);
                self.content_api
                    .list_markdown_files("")
                    .await
                    .map_err(|e| format!("GitHub API error: {}", e))
            }
        }
    }

    async fn get_existing_file_metadata(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, String> {
        use neo4rs::query;

        info!("[GitHubSync][SHA1] Querying Neo4j for existing file SHA1 hashes...");

        // Query all FileMetadata nodes for filename -> sha1 mapping
        let query_str = "MATCH (f:FileMetadata) RETURN f.filename AS filename, f.sha1 AS sha1";

        let graph = self.onto_repo.graph();
        let mut result = graph.execute(query(query_str)).await
            .map_err(|e| format!("Failed to query file metadata: {}", e))?;

        let mut metadata = std::collections::HashMap::new();
        while let Some(row) = result.next().await.map_err(|e| format!("Row iteration error: {}", e))? {
            if let (Ok(filename), Ok(sha1)) = (
                row.get::<String>("filename"),
                row.get::<String>("sha1")
            ) {
                metadata.insert(filename, sha1);
            }
        }

        info!("[GitHubSync][SHA1] Found {} existing file SHA1 hashes in Neo4j", metadata.len());
        Ok(metadata)
    }

    async fn update_file_metadata(
        &self,
        files: &[GitHubFileBasicMetadata],
    ) -> Result<(), String> {
        use neo4rs::query;

        if files.is_empty() {
            return Ok(());
        }

        info!("[GitHubSync][SHA1] Updating {} file SHA1 hashes in Neo4j...", files.len());

        let graph = self.onto_repo.graph();

        // Ensure FileMetadata index exists (idempotent)
        let index_query = "CREATE INDEX file_metadata_filename IF NOT EXISTS FOR (f:FileMetadata) ON (f.filename)";
        if let Err(e) = graph.run(query(index_query)).await {
            warn!("[GitHubSync] Failed to create FileMetadata index (may already exist): {}", e);
        }

        // MERGE each file metadata (update if exists, create if not)
        for file in files {
            let merge_query = query(
                "MERGE (f:FileMetadata {filename: $filename})
                 SET f.sha1 = $sha1, f.last_synced = datetime()"
            )
            .param("filename", file.name.clone())
            .param("sha1", file.sha.clone());

            if let Err(e) = graph.run(merge_query).await {
                warn!("[GitHubSync] Failed to update metadata for {}: {}", file.name, e);
            }
        }

        info!("[GitHubSync][SHA1] Updated {} file SHA1 hashes", files.len());
        Ok(())
    }

    /// Detect if the set of base paths changed and selectively clear stale Neo4j
    /// data for graph sources that were removed.
    ///
    /// Stores the active graph source list in a `SyncConfig` node keyed by
    /// `github_graph_sources`. Returns true if any path changed (triggering
    /// a forced full sync for all remaining paths).
    async fn detect_and_handle_base_paths_change(&self, current_paths: &[String]) -> bool {
        use neo4rs::query;

        if current_paths.is_empty() {
            return false;
        }

        let current_sources: std::collections::HashSet<String> = current_paths
            .iter()
            .map(|p| GitHubConfig::graph_source_for_path(p))
            .collect();

        let graph = self.onto_repo.graph();

        // Read the previously stored graph sources from Neo4j
        let read_query = query(
            "MATCH (c:SyncConfig {key: 'github_graph_sources'}) RETURN c.value AS value"
        );
        let stored_sources: Option<std::collections::HashSet<String>> = match graph.execute(read_query).await {
            Ok(mut result) => {
                match result.next().await {
                    Ok(Some(row)) => {
                        row.get::<String>("value").ok().map(|v| {
                            v.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                    }
                    _ => None,
                }
            }
            Err(e) => {
                warn!("[GitHubSync] Failed to read SyncConfig graph_sources: {} (treating as first run)", e);
                None
            }
        };

        let changed = match &stored_sources {
            Some(stored) if stored == &current_sources => false,
            Some(stored) => {
                // Find removed graph sources and selectively wipe their data
                let removed: Vec<&String> = stored.difference(&current_sources).collect();
                if !removed.is_empty() {
                    info!("Graph sources removed: {:?} — clearing their Neo4j data", removed);
                    for source in &removed {
                        if let Err(e) = self.clear_graph_source_data(source).await {
                            error!("Failed to clear data for graph source '{}': {}", source, e);
                        }
                    }
                }
                let added: Vec<&String> = current_sources.difference(stored).collect();
                if !added.is_empty() {
                    info!("Graph sources added: {:?}", added);
                }
                true
            }
            None => {
                info!("First sync run — recording graph sources: {:?}", current_sources);
                false
            }
        };

        // Also check legacy single-path SyncConfig for backward compat migration
        if stored_sources.is_none() {
            let legacy_query = query(
                "MATCH (c:SyncConfig {key: 'github_base_path'}) RETURN c.value AS value"
            );
            if let Ok(mut result) = graph.execute(legacy_query).await {
                if let Ok(Some(row)) = result.next().await {
                    if let Ok(old_path) = row.get::<String>("value") {
                        let old_source = GitHubConfig::graph_source_for_path(&old_path);
                        if !current_sources.contains(&old_source) {
                            info!("Legacy base path '{}' (source '{}') not in current config — clearing",
                                old_path, old_source);
                            if let Err(e) = self.clear_graph_source_data(&old_source).await {
                                error!("Failed to clear legacy graph source '{}': {}", old_source, e);
                            }
                        }
                    }
                }
            }
        }

        // Update the stored graph sources
        let sources_str = current_sources.iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        let upsert_query = query(
            "MERGE (c:SyncConfig {key: 'github_graph_sources'})
             SET c.value = $value, c.updated_at = datetime()"
        ).param("value", sources_str);

        if let Err(e) = graph.run(upsert_query).await {
            warn!("[GitHubSync] Failed to save SyncConfig graph_sources: {}", e);
        }

        // Also update legacy key for backward compat (use first path)
        if let Some(first_path) = current_paths.first() {
            let legacy_upsert = query(
                "MERGE (c:SyncConfig {key: 'github_base_path'})
                 SET c.value = $value, c.updated_at = datetime()"
            ).param("value", first_path.clone());

            if let Err(e) = graph.run(legacy_upsert).await {
                warn!("[GitHubSync] Failed to save legacy SyncConfig: {}", e);
            }
        }

        changed
    }

    /// Clear Neo4j data for a specific graph source.
    /// Only removes KGNode and FileMetadata entries tagged with this graph source.
    /// Falls back to clearing all if no graph_source tag exists (legacy data).
    async fn clear_graph_source_data(&self, graph_source: &str) -> Result<(), String> {
        use neo4rs::query;
        let graph = self.onto_repo.graph();

        // Remove KGNodes with matching graph_source
        let kg_query = query(
            "MATCH (n:KGNode {graph_source: $source}) DETACH DELETE n"
        ).param("source", graph_source.to_string());

        match graph.run(kg_query).await {
            Ok(_) => info!("  Cleared KGNode nodes for graph source '{}'", graph_source),
            Err(e) => warn!("  Failed to clear KGNode for '{}': {}", graph_source, e),
        }

        // Remove FileMetadata for files from this graph source
        let fm_query = query(
            "MATCH (f:FileMetadata {graph_source: $source}) DETACH DELETE f"
        ).param("source", graph_source.to_string());

        match graph.run(fm_query).await {
            Ok(_) => info!("  Cleared FileMetadata for graph source '{}'", graph_source),
            Err(e) => warn!("  Failed to clear FileMetadata for '{}': {}", graph_source, e),
        }

        info!("Cleared stale data for graph source '{}'", graph_source);
        Ok(())
    }

    /// Clear all stale data from Neo4j (legacy full wipe).
    /// Used only when no graph_source tagging exists (backward compat).
    #[allow(dead_code)]
    async fn clear_stale_neo4j_data(&self) -> Result<(), String> {
        use neo4rs::query;
        let graph = self.onto_repo.graph();

        let queries = vec![
            ("KGNode",    "MATCH (n:KGNode) DETACH DELETE n"),
            ("FileMetadata", "MATCH (n:FileMetadata) DETACH DELETE n"),
            ("OwlClass",     "MATCH (n:OwlClass) DETACH DELETE n"),
            ("OwlProperty",  "MATCH (n:OwlProperty) DETACH DELETE n"),
            ("Axiom",        "MATCH (n:Axiom) DETACH DELETE n"),
        ];

        for (label, cypher) in queries {
            match graph.run(query(cypher)).await {
                Ok(_) => info!("  Cleared {} nodes", label),
                Err(e) => warn!("  Failed to clear {} nodes: {}", label, e),
            }
        }

        info!("Stale Neo4j data cleared for fresh ingest");
        Ok(())
    }

    fn detect_file_type(&self, content: &str) -> FileType {
        let content = content.trim_start_matches('\u{feff}');

        // `public:: true` = Logseq publishing flag (user-authored, top of page).
        //   This is the real publication gate.
        //
        // `public-access:: true` = an OWL PROPERTY authored INSIDE ### OntologyBlock
        //   that classifies the ontology class's access level. It is NOT a page-
        //   level publishing flag. Previously the parser conflated the two, sweeping
        //   ~2,000 ontology-auto-stub pages into the KG as page nodes when they
        //   should only populate the OwlClass reasoning layer.
        //
        // Match on a line-anchored Logseq property (bullet or unindented) inside
        // the first 80 lines only, so body mentions of the literal string don't
        // count as a flag.
        let has_public = content.lines().take(80).any(|line| {
            let trimmed = line.trim_start_matches(|c: char| c == '-' || c.is_whitespace());
            trimmed.starts_with("public:: true")
                && trimmed.trim_end() == "public:: true"
        });

        let has_ontology = content.contains("### OntologyBlock");

        // Files with public:: true are knowledge-graph nodes.
        // The KG branch also has secondary OntologyBlock handling
        // (process_fetched_file) so ontology data is still extracted
        // from public pages — but KG nodes + wikilink edges are also
        // created, which is essential for the force-directed layout.
        if has_public {
            return FileType::KnowledgeGraph;
        }

        // All remaining files are checked for ontology blocks.
        // Ontology-headed data is pulled from every file in the repo,
        // not just public:: true tagged ones.
        if has_ontology {
            return FileType::Ontology;
        }

        FileType::Skip
    }

    /// Detect the semantic file format for routing to the correct parser.
    ///
    /// Returns a `FileFormat` variant:
    /// - `VisionClawV2`: v2 ontology format (`iri::` header + `rdf-type:: owl:Class`)
    /// - `OntologyV4`: legacy OntologyBlock format (`### OntologyBlock`)
    /// - `PublicNote`: Logseq public page (`public:: true` in first 25 lines)
    /// - `PrivateNote`: anything else
    pub fn detect_file_format(content: &str) -> FileFormat {
        let content = content.trim_start_matches('\u{feff}');

        // v2 format: iri:: header property + rdf-type:: owl:Class
        if content.starts_with("iri::") && content.contains("rdf-type:: owl:Class") {
            return FileFormat::VisionClawV2;
        }

        // Legacy OntologyBlock format
        if content.contains("### OntologyBlock") {
            return FileFormat::OntologyV4;
        }

        // Public Logseq page
        if content.lines().take(25).any(|l| {
            let trimmed = l.trim_start_matches(|c: char| c == '-' || c.is_whitespace());
            trimmed.starts_with("public:: true") && trimmed.trim_end() == "public:: true"
        }) {
            return FileFormat::PublicNote;
        }

        FileFormat::PrivateNote
    }

    /// Save ontology data to Neo4j and trigger reasoning pipeline
    /// This method:
    /// 1. Saves OWL classes, properties, and axioms to Neo4jOntologyRepository
    /// 2. Triggers OntologyPipelineService for automatic reasoning
    /// 3. Pipeline generates semantic constraints and uploads to GPU
    /// The reasoning pipeline runs asynchronously to avoid blocking sync.
    async fn save_ontology_data(&self, onto_data: crate::services::parsers::ontology_parser::OntologyData) -> Result<(), String> {
        use crate::ports::ontology_repository::OntologyRepository;

        // Save all ontology data to Neo4j graph database
        self.onto_repo.save_ontology(&onto_data.classes, &onto_data.properties, &onto_data.axioms).await
            .map_err(|e| format!("Failed to save ontology data: {}", e))?;

        // Log class hierarchy
        for (subclass_iri, superclass_iri) in onto_data.class_hierarchy {
            debug!("Class hierarchy: {} -> {}", subclass_iri, superclass_iri);
        }

        // 🔥 TRIGGER REASONING PIPELINE if configured
        // This spawns an async task to run CustomReasoner inference, generate
        // semantic constraints, and upload to GPU without blocking GitHub sync
        if let Some(pipeline) = &self.pipeline_service {
            info!("🔄 Triggering ontology reasoning pipeline after ontology save");

            // Convert parsed ontology data to Ontology struct for reasoning
            let mut ontology = crate::reasoning::custom_reasoner::Ontology::default();

            // Add classes
            for class in &onto_data.classes {
                use crate::reasoning::custom_reasoner::OWLClass;
                ontology.classes.insert(
                    class.iri.clone(),
                    OWLClass {
                        iri: class.iri.clone(),
                        label: class.label.clone(),
                        parent_class_iri: None, // Will be populated from axioms
                    },
                );
            }

            // Add subclass relationships
            use crate::ports::ontology_repository::AxiomType;
            for axiom in &onto_data.axioms {
                if matches!(axiom.axiom_type, AxiomType::SubClassOf) {
                    ontology.subclass_of
                        .entry(axiom.subject.clone())
                        .or_insert_with(std::collections::HashSet::new)
                        .insert(axiom.object.clone());
                }
            }

            // Trigger the pipeline asynchronously
            let ontology_id = 1; // Using default ontology ID - multi-ontology support deferred
            let pipeline_clone = Arc::clone(pipeline);

            tokio::spawn(async move {
                match pipeline_clone.on_ontology_modified(ontology_id, ontology).await {
                    Ok(stats) => {
                        info!(
                            "✅ Ontology pipeline complete: {} axioms inferred, {} constraints generated, GPU upload: {}",
                            stats.inferred_axioms_count,
                            stats.constraints_generated,
                            stats.gpu_upload_success
                        );
                    }
                    Err(e) => {
                        error!("❌ Ontology pipeline failed: {}", e);
                    }
                }
            });
        }

        Ok(())
    }
}
