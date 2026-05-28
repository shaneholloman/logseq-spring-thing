// src/services/github_sync_service.rs
//! GitHub Sync Service
//!
//! Synchronizes markdown files from GitHub repository to Oxigraph.
//! - Parses public:: true pages as knowledge graph nodes (KnowledgeGraphRepository)
//! - Extracts ```json-ld``` blocks and ingests quads via OxigraphOntologyRepository
//! - Enriches graph nodes with owl_class_iri metadata via OntologyEnrichmentService
//! - Uses SHA1 filtering to process only changed files (unless FORCE_FULL_SYNC=1)
//! - Batch processing (50 files) to avoid memory issues with large repositories

use crate::adapters::oxigraph_ontology_repository::{OxigraphOntologyRepository, GRAPH_ONTOLOGY};
use crate::adapters::whelk_inference_engine::WhelkInferenceEngine;
use crate::adapters::SqliteSettingsRepository;
use visionclaw_domain::models::edge::Edge;
use visionclaw_domain::ports::inference_engine::InferenceEngine;
use crate::ports::knowledge_graph_repository::KnowledgeGraphRepository;
use visionclaw_domain::ports::ontology_repository::{AxiomType, OntologyRepository, OwlAxiom};
use crate::services::edge_classifier::EdgeClassifier;
use crate::services::github::content_enhanced::EnhancedContentAPI;
use crate::services::github::types::GitHubFileBasicMetadata;
use crate::services::jsonld_ingest::{self, IngestOutcome, PageMetadata};
use crate::services::ontology_enrichment_service::OntologyEnrichmentService;
use crate::services::ontology_reasoner::OntologyReasoner;
use crate::services::parsers::KnowledgeGraphParser;
use crate::services::semantic_type_registry::SEMANTIC_TYPE_REGISTRY;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{debug, error, info, warn};
use oxigraph::model::{Quad, Subject};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const BATCH_SIZE: usize = 50;

// Predicate IRI constants for JSON-LD quad routing.
// Expanded forms (vc: prefix = https://narrativegoldmine.com/ns/v1#).
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const IRI_REQUIRES: &str = "https://narrativegoldmine.com/ns/v1#requires";
const IRI_ENABLES: &str = "https://narrativegoldmine.com/ns/v1#enables";
const IRI_DEPENDS_ON: &str = "https://narrativegoldmine.com/ns/v1#dependsOn";
const IRI_HAS_PART: &str = "https://narrativegoldmine.com/ns/v1#hasPart";
const IRI_IS_PART_OF: &str = "https://narrativegoldmine.com/ns/v1#isPartOf";
const IRI_RELATES_TO: &str = "https://narrativegoldmine.com/ns/v1#relatesTo";
const IRI_BRIDGES_TO: &str = "https://narrativegoldmine.com/ns/v1#bridgesTo";
const IRI_BRIDGES_FROM: &str = "https://narrativegoldmine.com/ns/v1#bridgesFrom";
const IRI_IMPLEMENTS: &str = "https://narrativegoldmine.com/ns/v1#implements";
const IRI_ENHANCES: &str = "https://narrativegoldmine.com/ns/v1#enhances";
const IRI_OPTIMIZES: &str = "https://narrativegoldmine.com/ns/v1#optimizes";
const IRI_SECURES: &str = "https://narrativegoldmine.com/ns/v1#secures";
const IRI_VALIDATES: &str = "https://narrativegoldmine.com/ns/v1#validates";
const IRI_WIKILINK: &str = "https://narrativegoldmine.com/ns/v1#wikilink";

// OWL2 / RDFS / PROV predicates
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const PROV_WAS_DERIVED_FROM: &str = "http://www.w3.org/ns/prov#wasDerivedFrom";
const PROV_WAS_ATTRIBUTED_TO: &str = "http://www.w3.org/ns/prov#wasAttributedTo";
const PROV_WAS_GENERATED_BY: &str = "http://www.w3.org/ns/prov#wasGeneratedBy";
const IRI_ACHIEVES_OBJECTIVE: &str = "https://narrativegoldmine.com/ns/v1#achievesObjective";
const IRI_TRACKED_ON: &str = "https://narrativegoldmine.com/ns/v1#trackedOn";
const IRI_SIMILAR_TO: &str = "https://narrativegoldmine.com/ns/v1#similarTo";
const IRI_SIMULATED_IN: &str = "https://narrativegoldmine.com/ns/v1#simulatedIn";

// New predicates in the NGM schema.
const IRI_USES: &str = "https://narrativegoldmine.com/ns/v1#uses";
const IRI_SUPPORTS: &str = "https://narrativegoldmine.com/ns/v1#supports";
const IRI_CONTRASTS_WITH: &str = "https://narrativegoldmine.com/ns/v1#contrastsWith";
const IRI_STANDARDIZED_BY: &str = "https://narrativegoldmine.com/ns/v1#standardizedBy";
const IRI_APPLIES_TO: &str = "https://narrativegoldmine.com/ns/v1#appliesTo";
const IRI_RELATED_TO: &str = "https://narrativegoldmine.com/ns/v1#relatedTo";
const IRI_PART_OF: &str = "https://narrativegoldmine.com/ns/v1#partOf";
const IRI_INSTANCE_OF: &str = "https://narrativegoldmine.com/ns/v1#instanceOf";
const IRI_NGM_SAME_AS: &str = "https://narrativegoldmine.com/ns/v1#sameAs";
const IRI_DEFINED_IN: &str = "https://narrativegoldmine.com/ns/v1#definedIn";
const IRI_ENABLED_BY: &str = "https://narrativegoldmine.com/ns/v1#enabledBy";
const IRI_UTILISES: &str = "https://narrativegoldmine.com/ns/v1#utilises";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

// Entity metadata IRI constants for JSON-LD node enrichment.
const VC_SOURCE_DOMAIN: &str = "https://narrativegoldmine.com/ns/v1#sourceDomain";
const VC_MATURITY: &str = "https://narrativegoldmine.com/ns/v1#maturity";
const VC_QUALITY_SCORE: &str = "https://narrativegoldmine.com/ns/v1#qualityScore";
const VC_DEFINITION: &str = "https://narrativegoldmine.com/ns/v1#definition";
const VC_SLUG: &str = "https://narrativegoldmine.com/ns/v1#slug";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const OWL_CLASS_IRI: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";

#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    KnowledgeGraph,
    Ontology,
    Skip,
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
    kg_repo: Arc<dyn KnowledgeGraphRepository>,
    onto_repo: Arc<OxigraphOntologyRepository>,
    enrichment_service: Arc<OntologyEnrichmentService>,
    inference_engine: Arc<RwLock<WhelkInferenceEngine>>,
    sync_db: Arc<SqliteSettingsRepository>,
}

impl GitHubSyncService {
    pub fn new(
        content_api: Arc<EnhancedContentAPI>,
        kg_repo: Arc<dyn KnowledgeGraphRepository>,
        onto_repo: Arc<OxigraphOntologyRepository>,
        sync_db: Arc<SqliteSettingsRepository>,
    ) -> Self {
        let reasoner = Arc::new(OntologyReasoner::new(
            Arc::new(WhelkInferenceEngine::new()),
            onto_repo.clone() as Arc<dyn OntologyRepository>,
        ));
        let classifier = Arc::new(EdgeClassifier::new());
        let enrichment_service = Arc::new(OntologyEnrichmentService::new(reasoner, classifier));

        Self {
            content_api,
            kg_parser: Arc::new(KnowledgeGraphParser::new()),
            kg_repo,
            onto_repo,
            enrichment_service,
            inference_engine: Arc::new(RwLock::new(WhelkInferenceEngine::new())),
            sync_db,
        }
    }

    /// Synchronize graphs from GitHub — processes in batches with progress logging.
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

        let base_path_changed = self.detect_and_handle_base_path_change().await;

        let files = match self.fetch_all_markdown_files().await {
            Ok(files) => {
                info!("Found {} markdown files", files.len());
                files
            }
            Err(e) => {
                let error_msg = format!("Failed to fetch files: {}", e);
                error!("{}", error_msg);
                stats.duration = start_time.elapsed();
                return Err(format!("GitHub sync failed: {}", error_msg));
            }
        };

        stats.total_files = files.len();

        let force_full_sync = base_path_changed
            || std::env::var("FORCE_FULL_SYNC")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false);

        let files_to_process = if force_full_sync {
            info!(
                "Full sync — processing ALL {} files (bypassing SHA1 filter)",
                files.len()
            );
            files.clone()
        } else {
            match self.filter_changed_files(&files).await {
                Ok(filtered) => {
                    info!(
                        "Processing {} changed files ({} unchanged)",
                        filtered.len(),
                        files.len() - filtered.len()
                    );
                    stats.skipped_files = files.len() - filtered.len();
                    filtered
                }
                Err(e) => {
                    error!("SHA1 filter failed: {}", e);
                    files.clone()
                }
            }
        };

        let all_files_to_process = files_to_process.clone();

        // Clear the graph only on a full sync. On an incremental sync (SHA1
        // filter narrowed the file list), existing data must remain — otherwise
        // an unchanged corpus leaves the store empty after the clear + no-op
        // batch loop, wiping out the previous good state.
        if force_full_sync {
            if let Err(e) = self.kg_repo.clear_graph().await {
                error!("Failed to clear graph before sync: {}", e);
                stats.errors.push(format!("clear_graph: {}", e));
            }
        }

        // Collect all deferred (cross-graph bridge) edges across every batch.
        // These reference nodes that may live in different batches, so we write
        // them in a final pass after every node is in the store.
        let mut deferred_edges: Vec<Edge> = Vec::new();

        for (batch_idx, batch) in files_to_process.chunks(BATCH_SIZE).enumerate() {
            let batch_start = Instant::now();
            info!(
                "Processing batch {}/{} ({} files)",
                batch_idx + 1,
                (files_to_process.len() + BATCH_SIZE - 1) / BATCH_SIZE,
                batch.len()
            );

            match self
                .process_batch_incremental(batch, &mut stats, &mut deferred_edges)
                .await
            {
                Ok(_) => {
                    info!(
                        "Batch {} completed in {:?}",
                        batch_idx + 1,
                        batch_start.elapsed()
                    );
                }
                Err(e) => {
                    error!("Batch {} failed: {}", batch_idx + 1, e);
                    stats.errors.push(format!("Batch {}: {}", batch_idx + 1, e));
                }
            }
        }

        // Final pass: write all deferred bridge edges now that every node is present.
        if !deferred_edges.is_empty() {
            info!(
                "Writing {} deferred bridge edges (final pass)",
                deferred_edges.len()
            );
            match self.kg_repo.batch_add_edges(deferred_edges.clone()).await {
                Ok(ids) => {
                    info!("Successfully wrote {} bridge edges", ids.len());
                    stats.total_edges += ids.len();
                }
                Err(e) => {
                    error!("Deferred bridge edges failed: {}", e);
                    stats.errors.push(format!("bridge edges: {}", e));
                }
            }
        }

        // Materialise domain root nodes and hierarchical edges to members.
        match self.materialise_domain_roots(&mut stats).await {
            Ok(n) => info!("Materialised {} domain root nodes with edges", n),
            Err(e) => {
                warn!("Domain root materialisation failed (non-fatal): {}", e);
                stats.errors.push(format!("domain_roots: {}", e));
            }
        }

        // Post-sync: run Whelk EL++ reasoning over the full ontology graph.
        match self.run_post_sync_reasoning(&mut stats).await {
            Ok(inferred) => info!("Post-sync reasoning produced {} inferred edges", inferred),
            Err(e) => {
                warn!("Post-sync reasoning failed (non-fatal): {}", e);
                stats.errors.push(format!("reasoning: {}", e));
            }
        }

        if let Err(e) = self.update_file_metadata(&all_files_to_process).await {
            warn!("Failed to update file_metadata: {}", e);
        }

        stats.duration = start_time.elapsed();
        info!(
            "Sync complete: {} nodes, {} edges in {:?}",
            stats.total_nodes, stats.total_edges, stats.duration
        );

        Ok(stats)
    }

    /// Create domain root nodes for the 6 NarrativeGoldmine domains and
    /// hierarchical edges from each node whose `group` matches a domain.
    async fn materialise_domain_roots(&self, stats: &mut SyncStatistics) -> Result<usize, String> {
        const DOMAINS: &[(&str, &str)] = &[
            ("spatial-computing", "Spatial Computing"),
            ("artificial-intelligence", "Artificial Intelligence"),
            ("infrastructure", "Infrastructure"),
            ("blockchain", "Blockchain"),
            ("robotics", "Robotics"),
            ("distributed-collaboration", "Distributed Collaboration"),
        ];

        let graph = self
            .kg_repo
            .load_graph()
            .await
            .map_err(|e| format!("load_graph: {}", e))?;

        // Collect domain → member node IDs from existing nodes.
        let mut domain_members: std::collections::HashMap<&str, Vec<u32>> =
            std::collections::HashMap::new();
        for node in &graph.nodes {
            if let Some(ref group) = node.group {
                for &(slug, _) in DOMAINS {
                    if group == slug {
                        domain_members.entry(slug).or_default().push(node.id);
                    }
                }
            }
        }

        let mut domain_nodes = Vec::new();
        let mut domain_edges = Vec::new();
        let mut created = 0;

        for &(slug, label) in DOMAINS {
            let members = match domain_members.get(slug) {
                Some(m) if !m.is_empty() => m,
                _ => continue,
            };

            let mut root = visionclaw_domain::models::node::Node::default();
            root.label = label.to_string();
            root.metadata_id = format!("domain-root-{}", slug);
            root.node_type = Some("domain_root".to_string());
            root.group = Some(slug.to_string());
            root.size = Some(3.0);
            root.weight = Some(1.0);
            root.owl_class_iri = Some(format!("urn:ngm:domain:{}", slug));
            root.metadata
                .insert("type".to_string(), "domain_root".to_string());
            domain_nodes.push(root);
        }

        if domain_nodes.is_empty() {
            return Ok(0);
        }

        let root_ids = self
            .kg_repo
            .batch_add_nodes(domain_nodes)
            .await
            .map_err(|e| format!("batch_add_nodes domain roots: {}", e))?;

        // Map slug → assigned root ID.
        let domain_slugs: Vec<&str> = DOMAINS
            .iter()
            .filter(|(slug, _)| domain_members.contains_key(slug))
            .map(|(slug, _)| *slug)
            .collect();

        for (idx, &root_id) in root_ids.iter().enumerate() {
            let slug = domain_slugs[idx];
            if let Some(members) = domain_members.get(slug) {
                for &member_id in members {
                    let edge = Edge {
                        id: format!("domain_{}_{}", root_id, member_id),
                        source: root_id,
                        target: member_id,
                        weight: 1.5,
                        edge_type: Some("hierarchical".to_string()),
                        owl_property_iri: None,
                        metadata: None,
                    };
                    domain_edges.push(edge);
                }
            }
            created += 1;
        }

        if !domain_edges.is_empty() {
            match self.kg_repo.batch_add_edges(domain_edges.clone()).await {
                Ok(ids) => {
                    stats.total_edges += ids.len();
                    info!(
                        "Created {} domain root edges for {} domains",
                        ids.len(),
                        created
                    );
                }
                Err(e) => warn!("Failed to write domain root edges: {}", e),
            }
        }

        stats.total_nodes += created;
        Ok(created)
    }

    /// Run Whelk EL++ reasoning after all files have been synced.
    /// Loads OWL classes + axioms from Oxigraph, adds the NarrativeGoldmine
    /// property hierarchy, runs inference, stores results, and creates
    /// inferred edges in the knowledge graph.
    async fn run_post_sync_reasoning(&self, stats: &mut SyncStatistics) -> Result<usize, String> {
        let reasoning_start = Instant::now();

        let classes = self
            .onto_repo
            .get_classes()
            .await
            .map_err(|e| format!("Failed to load OWL classes: {}", e))?;
        let mut axioms = self
            .onto_repo
            .get_axioms()
            .await
            .map_err(|e| format!("Failed to load OWL axioms: {}", e))?;

        if classes.is_empty() {
            info!("No OWL classes in store — skipping reasoning");
            return Ok(0);
        }

        axioms.extend(Self::ngm_property_hierarchy_axioms());

        info!(
            "Loading {} classes and {} axioms into Whelk",
            classes.len(),
            axioms.len()
        );

        let mut engine = self.inference_engine.write().await;
        engine
            .load_ontology(classes, axioms)
            .await
            .map_err(|e| format!("Whelk load_ontology: {}", e))?;

        let results = engine
            .infer()
            .await
            .map_err(|e| format!("Whelk infer: {}", e))?;

        info!(
            "Whelk produced {} inferred axioms in {}ms",
            results.inferred_axioms.len(),
            results.inference_time_ms
        );

        if let Err(e) = self.onto_repo.store_inference_results(&results).await {
            warn!("Failed to persist inference results: {}", e);
        }

        // Build IRI→node_id map from the current graph to resolve inferred edges.
        let mut iri_to_node_id: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        if let Ok(graph) = self.kg_repo.load_graph().await {
            for node in &graph.nodes {
                if let Some(ref iri) = node.owl_class_iri {
                    iri_to_node_id.insert(iri.clone(), node.id);
                }
                if let Some(iri) = node.metadata.get("owl_class_iri") {
                    iri_to_node_id.insert(iri.clone(), node.id);
                }
            }
        }

        let mut inferred_edge_count = 0;
        let mut inferred_edges = Vec::new();

        for axiom in &results.inferred_axioms {
            if axiom.axiom_type == AxiomType::SubClassOf
                && !axiom.subject.contains("owl#Nothing")
                && !axiom.object.contains("owl#Thing")
                && axiom.subject != axiom.object
            {
                if let (Some(&src_id), Some(&tgt_id)) = (
                    iri_to_node_id.get(&axiom.subject),
                    iri_to_node_id.get(&axiom.object),
                ) {
                    let edge_id = format!("inferred_{}_{}", src_id, tgt_id);
                    let mut edge_meta = std::collections::HashMap::new();
                    edge_meta.insert("source_iri".to_string(), axiom.subject.clone());
                    edge_meta.insert("target_iri".to_string(), axiom.object.clone());
                    edge_meta.insert("axiom_type".to_string(), "SubClassOf".to_string());
                    let edge = Edge {
                        id: edge_id,
                        source: src_id,
                        target: tgt_id,
                        weight: 0.4,
                        edge_type: Some("inferred".to_string()),
                        owl_property_iri: None,
                        metadata: Some(edge_meta),
                    };
                    inferred_edges.push(edge);
                }
            }
        }

        if !inferred_edges.is_empty() {
            info!(
                "Creating {} inferred edges ({} axioms had no matching nodes)",
                inferred_edges.len(),
                results.inferred_axioms.len() - inferred_edges.len()
            );
            match self.kg_repo.batch_add_edges(inferred_edges.clone()).await {
                Ok(ids) => {
                    inferred_edge_count = ids.len();
                    stats.total_edges += inferred_edge_count;
                }
                Err(e) => warn!("Failed to write inferred edges: {}", e),
            }
        }

        info!(
            "Post-sync reasoning complete in {:?}: {} inferred edges",
            reasoning_start.elapsed(),
            inferred_edge_count
        );
        Ok(inferred_edge_count)
    }

    /// NarrativeGoldmine property hierarchy axioms for Whelk reasoning.
    /// Declares: requires subPropertyOf dependsOn,
    /// uses/supports/implements subPropertyOf utilises,
    /// hasPart/isPartOf transitive, relatesTo/similarTo symmetric,
    /// hasPart inverseOf isPartOf, enables inverseOf enabledBy.
    fn ngm_property_hierarchy_axioms() -> Vec<OwlAxiom> {
        let sub_prop = |sub: &str, sup: &str| OwlAxiom {
            id: None,
            axiom_type: AxiomType::SubPropertyOf,
            subject: format!("https://narrativegoldmine.com/ns/v1#{sub}"),
            object: format!("https://narrativegoldmine.com/ns/v1#{sup}"),
            annotations: std::collections::HashMap::new(),
        };
        let transitive = |prop: &str| OwlAxiom {
            id: None,
            axiom_type: AxiomType::TransitiveProperty,
            subject: format!("https://narrativegoldmine.com/ns/v1#{prop}"),
            object: String::new(),
            annotations: std::collections::HashMap::new(),
        };
        let symmetric = |prop: &str| OwlAxiom {
            id: None,
            axiom_type: AxiomType::SymmetricProperty,
            subject: format!("https://narrativegoldmine.com/ns/v1#{prop}"),
            object: String::new(),
            annotations: std::collections::HashMap::new(),
        };
        let inverse = |p1: &str, p2: &str| OwlAxiom {
            id: None,
            axiom_type: AxiomType::InverseProperties,
            subject: format!("https://narrativegoldmine.com/ns/v1#{p1}"),
            object: format!("https://narrativegoldmine.com/ns/v1#{p2}"),
            annotations: std::collections::HashMap::new(),
        };

        vec![
            // Property hierarchy: requires subPropertyOf dependsOn
            sub_prop("requires", "dependsOn"),
            // uses, supports, implements subPropertyOf utilises
            sub_prop("uses", "utilises"),
            sub_prop("supports", "utilises"),
            sub_prop("implements", "utilises"),
            // Transitive properties
            transitive("hasPart"),
            transitive("isPartOf"),
            transitive("dependsOn"),
            // Symmetric properties
            symmetric("relatesTo"),
            symmetric("similarTo"),
            // Inverse property pairs
            inverse("hasPart", "isPartOf"),
            inverse("enables", "enabledBy"),
            inverse("implements", "implementedBy"),
        ]
    }

    /// Process a batch of files incrementally — adds nodes/edges to an
    /// already-cleared store without wiping previous batches. Bridge edges
    /// (cross-graph, e.g. agent↔knowledge) are collected into `deferred_edges`
    /// for a final pass after all nodes from every batch are present.
    async fn process_batch_incremental(
        &self,
        files: &[GitHubFileBasicMetadata],
        stats: &mut SyncStatistics,
        deferred_edges: &mut Vec<Edge>,
    ) -> Result<(), String> {
        let mut batch_nodes = std::collections::HashMap::new();
        let mut batch_edges = std::collections::HashMap::new();
        let mut public_pages = std::collections::HashSet::new();

        const PARALLEL_FETCHES: usize = 8;

        fn create_fetch_future(
            content_api: Arc<EnhancedContentAPI>,
            file: GitHubFileBasicMetadata,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = (GitHubFileBasicMetadata, Result<String, String>)>
                    + Send,
            >,
        > {
            let download_url = file.download_url.clone();
            Box::pin(async move {
                let result = content_api
                    .fetch_file_content(&download_url)
                    .await
                    .map_err(|e| format!("Failed to fetch content: {}", e));
                (file, result)
            })
        }

        let mut fetch_futures: FuturesUnordered<_> = FuturesUnordered::new();
        let mut fetched_contents: Vec<(GitHubFileBasicMetadata, Result<String, String>)> =
            Vec::with_capacity(files.len());
        let mut file_iter = files.iter().cloned().peekable();

        while fetch_futures.len() < PARALLEL_FETCHES {
            if let Some(file) = file_iter.next() {
                fetch_futures.push(create_fetch_future(Arc::clone(&self.content_api), file));
            } else {
                break;
            }
        }

        while let Some((file, content_result)) = fetch_futures.next().await {
            fetched_contents.push((file, content_result));
            if let Some(file) = file_iter.next() {
                fetch_futures.push(create_fetch_future(Arc::clone(&self.content_api), file));
            }
        }

        for (idx, (file, content_result)) in fetched_contents.into_iter().enumerate() {
            if idx % 10 == 0 && idx > 0 {
                info!(
                    "  Progress: {}/{} files (nodes: {}, edges: {})",
                    idx,
                    files.len(),
                    batch_nodes.len(),
                    batch_edges.len()
                );
            }

            match content_result {
                Ok(content) => {
                    match self
                        .process_fetched_file(
                            &file,
                            &content,
                            &mut batch_nodes,
                            &mut batch_edges,
                            &mut public_pages,
                        )
                        .await
                    {
                        Ok(()) => {
                            stats.kg_files_processed += 1;
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

        if !batch_nodes.is_empty() {
            let node_vec: Vec<_> = batch_nodes.into_values().collect();
            let all_edges: Vec<_> = batch_edges.into_values().collect();

            info!(
                "Adding batch: {} nodes, {} edges",
                node_vec.len(),
                all_edges.len()
            );

            // Collect node IDs in this batch for bridge-edge detection.
            let batch_node_ids: std::collections::HashSet<u32> =
                node_vec.iter().map(|n| n.id).collect();

            match self.kg_repo.batch_add_nodes(node_vec.clone()).await {
                Ok(ids) => {
                    stats.total_nodes += ids.len();
                    info!("  Wrote {} nodes", ids.len());
                }
                Err(e) => {
                    error!("batch_add_nodes failed: {}", e);
                    return Err(format!("batch_add_nodes: {}", e));
                }
            }

            // Partition edges: same-batch edges (both endpoints in this batch)
            // are written immediately; cross-batch edges are deferred.
            let mut immediate_edges = Vec::new();
            for edge in all_edges {
                if batch_node_ids.contains(&edge.source) && batch_node_ids.contains(&edge.target) {
                    immediate_edges.push(edge);
                } else {
                    deferred_edges.push(edge);
                }
            }

            if !immediate_edges.is_empty() {
                match self.kg_repo.batch_add_edges(immediate_edges.clone()).await {
                    Ok(ids) => {
                        stats.total_edges += ids.len();
                        info!(
                            "  Wrote {} same-batch edges ({} deferred)",
                            ids.len(),
                            deferred_edges.len()
                        );
                    }
                    Err(e) => {
                        warn!("batch_add_edges (same-batch) failed: {} — deferring all", e);
                        deferred_edges.extend(immediate_edges);
                    }
                }
            }
        } else {
            warn!("Batch is empty after processing — nothing to save");
        }

        Ok(())
    }

    /// Process one pre-fetched file, populating nodes/edges in-place.
    async fn process_fetched_file(
        &self,
        file: &GitHubFileBasicMetadata,
        content: &str,
        nodes: &mut std::collections::HashMap<u32, visionclaw_domain::models::node::Node>,
        edges: &mut std::collections::HashMap<String, Edge>,
        public_pages: &mut std::collections::HashSet<String>,
    ) -> Result<(), String> {
        debug!("Processing file: {} ({} bytes)", file.name, content.len());

        let file_type = self.detect_file_type(content);
        debug!("Detected {:?} for {}", file_type, file.name);

        let page_name = file.name.trim_end_matches(".md");

        match file_type {
            FileType::KnowledgeGraph => {
                // Parse wikilinks and KG node structure.
                let mut parsed = self
                    .kg_parser
                    .parse(content, &file.name)
                    .map_err(|e| format!("Parse error: {}", e))?;

                info!(
                    "Parsed {}: {} nodes, {} edges",
                    file.name,
                    parsed.nodes.len(),
                    parsed.edges.len()
                );

                // Enrich with ontology IRI metadata.
                match self
                    .enrichment_service
                    .enrich_graph(&mut parsed, &file.path, content)
                    .await
                {
                    Ok((n, e)) => {
                        debug!(
                            "Enriched {}: {} nodes, {} edges with OWL IRIs",
                            file.name, n, e
                        );
                    }
                    Err(e) => {
                        warn!("Enrichment failed for {}: {} (continuing)", file.name, e);
                    }
                }

                public_pages.insert(page_name.to_string());

                for node in parsed.nodes {
                    nodes.insert(node.id, node);
                }
                for edge in parsed.edges {
                    edges.insert(edge.id.clone(), edge);
                }

                // Extract JSON-LD blocks and ingest quads into Oxigraph.
                let source_id = self.kg_parser.page_name_to_id(page_name);
                let metadata = PageMetadata::new(&file.path);
                match jsonld_ingest::ingest_page(content, &metadata).await {
                    Ok(outcome) => {
                        let onto_edges = self.process_jsonld_outcome(&outcome, source_id);
                        let edges_added = onto_edges.len();
                        for edge in onto_edges {
                            // Create ontology node for targets with class/individual IRIs,
                            // otherwise fall back to linked_page stub.
                            let target_iri = edge
                                .metadata
                                .as_ref()
                                .and_then(|m| m.get("target_iri"))
                                .map(|s| s.as_str());
                            if let Some(iri) = target_iri {
                                if iri.contains(":class:")
                                    || iri.contains("/class/")
                                    || iri.contains(":individual:")
                                    || iri.contains("/individual/")
                                {
                                    self.ensure_ontology_node(edge.target, iri, nodes);
                                } else {
                                    self.ensure_linked_page_node(edge.target, target_iri, nodes);
                                }
                            } else {
                                self.ensure_linked_page_node(edge.target, target_iri, nodes);
                            }
                            edges.insert(edge.id.clone(), edge);
                        }
                        if edges_added > 0 {
                            debug!(
                                "Created {} semantic edges from JSON-LD in {}",
                                edges_added, file.name
                            );
                        }
                        // Persist quads to the Oxigraph store.
                        if !outcome.quads.is_empty() {
                            if let Err(e) = self.insert_quads_to_store(&outcome.quads).await {
                                warn!("Failed to insert quads from {}: {}", file.name, e);
                            }
                        }

                        // Enrich source node with entity metadata from JSON-LD quads.
                        if let Some(node) = nodes.get_mut(&source_id) {
                            Self::enrich_node_from_quads(node, &outcome.quads, page_name);
                        }
                    }
                    Err(e) => {
                        debug!("No JSON-LD blocks in {}: {}", file.name, e);
                    }
                }

                Ok(())
            }

            FileType::Ontology => {
                // Files with JSON-LD but no public:: true — ontology-only ingest.
                debug!("Ontology-only ingest for {}", file.name);
                let metadata = PageMetadata::new(&file.path);
                match jsonld_ingest::ingest_page(content, &metadata).await {
                    Ok(outcome) => {
                        debug!(
                            "Ingested {} quads from {} JSON-LD block(s) in {}",
                            outcome.quad_count, outcome.block_count, file.name
                        );
                        if !outcome.quads.is_empty() {
                            if let Err(e) = self.insert_quads_to_store(&outcome.quads).await {
                                error!("Failed to insert quads from {}: {}", file.name, e);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("JSON-LD ingest failed for {}: {}", file.name, e);
                    }
                }
                Ok(())
            }

            FileType::Skip => {
                debug!(
                    "Skipped: {} (no public:: true or json-ld blocks)",
                    file.name
                );
                Ok(())
            }
        }
    }

    /// Map an IngestOutcome's quads to Edge structs for the force-directed graph.
    ///
    /// Only object-property triples with named-node objects produce graph edges.
    /// Literal-valued triples (labels, descriptions, SHA1s, etc.) are skipped.
    ///
    /// Target node IDs are derived from the IRI local name via the same
    /// `KnowledgeGraphParser::page_name_to_id` hash so IDs are consistent with
    /// nodes created by the KG parser branch.
    fn process_jsonld_outcome(&self, outcome: &IngestOutcome, source_id: u32) -> Vec<Edge> {
        let mut result = Vec::new();
        let mut unmapped_count: usize = 0;
        let mut unmapped_samples: Vec<String> = Vec::new();

        for quad in &outcome.quads {
            // Subject must be a named node.
            let _subj_iri = match &quad.subject {
                Subject::NamedNode(n) => n.as_str(),
                _ => continue,
            };

            let predicate_iri = quad.predicate.as_str();

            // Object must be a named node (relationship target).
            let object_iri = match &quad.object {
                oxigraph::model::Term::NamedNode(n) => n.as_str().to_string(),
                _ => continue,
            };

            let edge_type = predicate_to_edge_type(predicate_iri);
            if edge_type.is_empty() {
                unmapped_count += 1;
                if unmapped_samples.len() < 5 {
                    let iri_str = predicate_iri.to_string();
                    if !unmapped_samples.contains(&iri_str) {
                        unmapped_samples.push(iri_str);
                    }
                }
                continue;
            }

            // Extract the local name fragment from the object IRI and resolve to
            // a numeric node ID via the KG parser's hash — matching existing node IDs.
            let local_name = object_iri
                .rsplit_once(':')
                .map(|(_, r)| r)
                .unwrap_or(&object_iri);
            let target_id = self.kg_parser.page_name_to_id(local_name);
            if target_id == source_id {
                continue;
            }

            let reg_id = SEMANTIC_TYPE_REGISTRY.get_or_register_id(edge_type);
            let weight = SEMANTIC_TYPE_REGISTRY
                .get_config(reg_id)
                .map(|c| c.strength * 2.0) // normalise registry 0-1 to spring 0-2 range
                .unwrap_or(1.0);
            let edge_id = format!("{}_{}_{}", source_id, target_id, edge_type);
            let mut edge_meta = std::collections::HashMap::new();
            edge_meta.insert("target_iri".to_string(), object_iri.clone());
            let edge = Edge {
                id: edge_id.clone(),
                source: source_id,
                target: target_id,
                weight,
                edge_type: Some(edge_type.to_string()),
                owl_property_iri: Some(predicate_iri.to_string()),
                metadata: Some(edge_meta),
            };
            result.push(edge);
        }

        if unmapped_count > 0 {
            warn!(
                "process_jsonld_outcome: {} unmapped predicate(s) for source_id={}, samples: {:?}",
                unmapped_count, source_id, unmapped_samples
            );
        }

        result
    }

    /// Insert quads into the Oxigraph store via spawn_blocking.
    async fn insert_quads_to_store(&self, quads: &[Quad]) -> Result<(), String> {
        let store = Arc::clone(self.onto_repo.store());
        let quads_owned: Vec<Quad> = quads.to_vec();
        tokio::task::spawn_blocking(move || {
            store
                .transaction(|mut tx| {
                    for quad in &quads_owned {
                        tx.insert(quad)?;
                    }
                    Ok(()) as Result<(), oxigraph::store::StorageError>
                })
                .map_err(|e| format!("Oxigraph transaction error: {}", e))
        })
        .await
        .map_err(|e| format!("spawn_blocking join error: {}", e))?
    }

    /// Ensure a node exists in the batch map as a linked_page (stub).
    ///
    /// `target_iri` is the IRI the link points at (when available — e.g.
    /// from a JSON-LD vc:wikilink edge). When provided we derive a
    /// human-readable label from its local-name segment, so the resulting
    /// node shows up in the UI as "Backdoor Attack" instead of
    /// "node_672356712531". Falls back to "node_<id>" only when the caller
    /// has nothing better.
    fn ensure_linked_page_node(
        &self,
        id: u32,
        target_iri: Option<&str>,
        nodes: &mut std::collections::HashMap<u32, visionclaw_domain::models::node::Node>,
    ) {
        if nodes.contains_key(&id) {
            return;
        }
        let mut node = visionclaw_domain::models::node::Node::default();
        node.id = id;
        let (label, metadata_id) = match target_iri {
            Some(iri) => {
                let local_name = iri.rsplit_once(':').map(|(_, r)| r).unwrap_or(iri);
                let local_name = local_name.rsplit_once('/').map(|(_, r)| r).unwrap_or(local_name);
                (local_name.replace('-', " "), local_name.to_string())
            }
            None => (format!("node_{}", id), format!("node_{}", id)),
        };
        node.label = label;
        node.metadata_id = metadata_id;
        node.node_type = Some("linked_page".to_string());
        node.metadata
            .insert("type".to_string(), "linked_page".to_string());
        nodes.insert(id, node);
    }

    /// Ensure an ontology node exists in the batch map for a Class/Individual IRI.
    /// Separate from KG page nodes — these form the ontology graph layer.
    fn ensure_ontology_node(
        &self,
        id: u32,
        iri: &str,
        nodes: &mut std::collections::HashMap<u32, visionclaw_domain::models::node::Node>,
    ) {
        if let Some(existing) = nodes.get_mut(&id) {
            // Upgrade linked_page stubs to ontology type if IRI indicates it.
            // Also refresh the human-readable label from the IRI's local-name
            // segment — without this, stubs originally created without a
            // target_iri (so labelled "node_<id>") keep that label even after
            // we know the canonical class IRI here.
            if existing.node_type.as_deref() == Some("linked_page") {
                let onto_type = if iri.contains(":individual:") || iri.contains("/individual/") {
                    "owl_individual"
                } else {
                    "owl_class"
                };
                existing.node_type = Some(onto_type.to_string());
                existing
                    .metadata
                    .insert("type".to_string(), onto_type.to_string());
                existing.owl_class_iri = Some(iri.to_string());
                let local_name = iri.rsplit_once(':').map(|(_, r)| r).unwrap_or(iri);
                let local_name = local_name.rsplit_once('/').map(|(_, r)| r).unwrap_or(local_name);
                if existing.label.starts_with("node_") {
                    existing.label = local_name.replace('-', " ");
                }
                if existing.metadata_id.starts_with("node_") {
                    existing.metadata_id = local_name.to_string();
                }
            }
            return;
        }
        let mut node = visionclaw_domain::models::node::Node::default();
        node.id = id;
        let local_name = iri.rsplit_once(':').map(|(_, r)| r).unwrap_or(iri);
        node.label = local_name.replace('-', " ");
        node.metadata_id = local_name.to_string();
        node.owl_class_iri = Some(iri.to_string());
        let onto_type = if iri.contains(":individual:") || iri.contains("/individual/") {
            "owl_individual"
        } else {
            "owl_class"
        };
        node.node_type = Some(onto_type.to_string());
        node.metadata
            .insert("type".to_string(), onto_type.to_string());
        nodes.insert(id, node);
    }

    /// Enrich a graph node with metadata extracted from JSON-LD quads.
    /// Reads rdf:type, domain, maturity, qualityScore, label, and definition
    /// from literal-valued quads whose subject matches the entity IRI.
    fn enrich_node_from_quads(
        node: &mut visionclaw_domain::models::node::Node,
        quads: &[Quad],
        page_name: &str,
    ) {
        // Find the entity IRI — look for any quad whose subject contains
        // the page slug as a class or individual IRI.
        let slug = page_name.to_lowercase().replace(' ', "-");
        let entity_iri = quads.iter().find_map(|q| {
            if let Subject::NamedNode(n) = &q.subject {
                let iri = n.as_str();
                if iri.contains(&slug)
                    && (iri.starts_with("urn:ngm:")
                        || iri.contains("/class/")
                        || iri.contains("/individual/"))
                {
                    return Some(iri.to_string());
                }
            }
            None
        });

        let entity_iri = match entity_iri {
            Some(iri) => iri,
            None => return,
        };

        // Set owl_class_iri to the entity's IRI.
        node.owl_class_iri = Some(entity_iri.clone());

        for quad in quads {
            let subj_iri = match &quad.subject {
                Subject::NamedNode(n) => n.as_str(),
                _ => continue,
            };
            if subj_iri != entity_iri {
                continue;
            }

            let pred = quad.predicate.as_str();

            // Record entity OWL type as metadata but do NOT change node_type.
            // KG pages stay as "page" nodes (Gem geometry); ontology nodes
            // are separate (CrystalOrb). The owl_class_iri link bridges them.
            if pred == RDF_TYPE {
                if let oxigraph::model::Term::NamedNode(n) = &quad.object {
                    let type_iri = n.as_str();
                    if type_iri == OWL_CLASS_IRI {
                        node.metadata
                            .insert("owl_type".to_string(), "Class".to_string());
                    } else if type_iri == OWL_NAMED_INDIVIDUAL {
                        node.metadata
                            .insert("owl_type".to_string(), "Individual".to_string());
                    }
                }
                continue;
            }

            // Extract literal values for metadata.
            let literal_value = match &quad.object {
                oxigraph::model::Term::Literal(lit) => lit.value().to_string(),
                _ => continue,
            };

            match pred {
                p if p == VC_SOURCE_DOMAIN => {
                    node.metadata
                        .insert("domain".to_string(), literal_value.clone());
                    node.group = Some(literal_value);
                }
                p if p == VC_MATURITY => {
                    node.metadata.insert("maturity".to_string(), literal_value);
                }
                p if p == VC_QUALITY_SCORE => {
                    node.metadata
                        .insert("qualityScore".to_string(), literal_value.clone());
                    if let Ok(score) = literal_value.parse::<f32>() {
                        node.size = Some(0.5 + score * 1.5); // range 0.5-2.0
                        node.weight = Some(score);
                    }
                }
                p if p == RDFS_LABEL => {
                    if !literal_value.is_empty() {
                        node.label = literal_value;
                    }
                }
                p if p == RDFS_COMMENT || p == VC_DEFINITION => {
                    node.metadata
                        .insert("definition".to_string(), literal_value);
                }
                p if p == VC_SLUG => {
                    node.metadata.insert("slug".to_string(), literal_value);
                }
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------
    // File type detection
    // ------------------------------------------------------------------

    fn detect_file_type(&self, content: &str) -> FileType {
        let content = content.trim_start_matches('\u{feff}');

        let has_jsonld = content.contains("```json-ld");
        let has_public = content.contains("public:: true")
            || content.contains("public-access:: true")
            || (has_jsonld
                && (content.contains("\"vc:public\": true")
                    || content.contains("\"vc:public\":true")));

        if has_public {
            // KG branch — JSON-LD extraction also runs here if blocks are present.
            return FileType::KnowledgeGraph;
        }

        if has_jsonld {
            return FileType::Ontology;
        }

        FileType::Skip
    }

    // ------------------------------------------------------------------
    // File listing + SHA1 change detection
    // ------------------------------------------------------------------

    async fn fetch_all_markdown_files(&self) -> Result<Vec<GitHubFileBasicMetadata>, String> {
        match self.content_api.list_markdown_files_via_tree().await {
            Ok(files) => {
                info!("Trees API returned {} markdown files", files.len());
                Ok(files)
            }
            Err(e) => {
                warn!("Trees API failed ({}), falling back to Contents API", e);
                self.content_api
                    .list_markdown_files("")
                    .await
                    .map_err(|e| format!("GitHub API error: {}", e))
            }
        }
    }

    async fn filter_changed_files(
        &self,
        files: &[GitHubFileBasicMetadata],
    ) -> Result<Vec<GitHubFileBasicMetadata>, String> {
        let existing = self.get_existing_file_metadata().await?;

        Ok(files
            .iter()
            .filter(|f| match existing.get(&f.name) {
                Some(sha) if sha == &f.sha => false,
                _ => true,
            })
            .cloned()
            .collect())
    }

    // ------------------------------------------------------------------
    // SHA1 / SyncConfig persistence via SQLite
    // ------------------------------------------------------------------

    async fn get_existing_file_metadata(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, String> {
        info!("[SHA1] Querying SQLite for existing file SHA1 hashes");

        let map = self
            .sync_db
            .get_file_sha1s()
            .await
            .map_err(|e| format!("SQLite query error: {}", e))?;

        info!("[SHA1] Found {} existing SHA1 hashes", map.len());
        Ok(map)
    }

    async fn update_file_metadata(&self, files: &[GitHubFileBasicMetadata]) -> Result<(), String> {
        if files.is_empty() {
            return Ok(());
        }

        info!("[SHA1] Updating {} file SHA1 hashes in SQLite", files.len());

        let pairs: Vec<(String, String)> = files
            .iter()
            .map(|f| (f.name.clone(), f.sha.clone()))
            .collect();

        self.sync_db
            .upsert_file_sha1s(&pairs)
            .await
            .map_err(|e| format!("SQLite update error: {}", e))
    }

    /// Detect GITHUB_BASE_PATH change; clear stale data if it changed.
    /// Returns true when a change was detected (triggers forced full sync).
    async fn detect_and_handle_base_path_change(&self) -> bool {
        let current_base_path = std::env::var("GITHUB_BASE_PATH").unwrap_or_default();
        if current_base_path.is_empty() {
            return false;
        }

        // Read previously stored base path from SQLite.
        let stored_base_path = match self.sync_db.get_sync_config("github_base_path").await {
            Ok(val) => val,
            Err(e) => {
                warn!("Failed to read sync config: {}", e);
                None
            }
        };

        let changed = match &stored_base_path {
            Some(stored) if stored == &current_base_path => false,
            Some(stored) => {
                info!(
                    "GITHUB_BASE_PATH changed: '{}' -> '{}' — clearing stale data",
                    stored, current_base_path
                );
                true
            }
            None => {
                info!(
                    "First sync run — recording base path '{}'",
                    current_base_path
                );
                false
            }
        };

        if changed {
            if let Err(e) = self.clear_stale_data().await {
                error!("Failed to clear stale data: {}", e);
            }
        }

        // Upsert the current base path in SQLite.
        if let Err(e) = self
            .sync_db
            .set_sync_config("github_base_path", &current_base_path)
            .await
        {
            warn!("Failed to save SyncConfig base path: {}", e);
        }

        changed
    }

    /// Clear all stale data when switching to a new GitHub base path.
    /// Clears Oxigraph ontology graph (actual RDF data) and SQLite sync metadata.
    async fn clear_stale_data(&self) -> Result<(), String> {
        info!("Clearing stale data for fresh ingest");

        // Clear Oxigraph ontology graph (real RDF data, not metadata).
        let update = format!("CLEAR GRAPH <{GRAPH_ONTOLOGY}>");
        let store = Arc::clone(self.onto_repo.store());
        tokio::task::spawn_blocking(move || {
            store
                .update(&update)
                .map_err(|e| format!("SPARQL clear error: {}", e))
        })
        .await
        .map_err(|e| format!("join error: {}", e))??;

        // Clear SQLite sync metadata (file hashes + config).
        self.sync_db
            .clear_sync_metadata()
            .await
            .map_err(|e| format!("SQLite clear error: {}", e))
    }

    // ------------------------------------------------------------------
    // Dead-code-safe filter helpers (kept for future use)
    // ------------------------------------------------------------------

    #[allow(dead_code)]
    fn filter_linked_pages(
        &self,
        nodes: &mut std::collections::HashMap<u32, visionclaw_domain::models::node::Node>,
        public_pages: &std::collections::HashSet<String>,
    ) {
        let before = nodes.len();
        nodes.retain(
            |_, node| match node.metadata.get("type").map(|s| s.as_str()) {
                Some("page") => true,
                Some("linked_page") => public_pages.contains(&node.metadata_id),
                _ => true,
            },
        );
        let filtered = before - nodes.len();
        if filtered > 0 {
            info!("Filtered {} linked_page nodes", filtered);
        }
    }

    #[allow(dead_code)]
    fn filter_orphan_edges(
        &self,
        edges: &mut std::collections::HashMap<String, Edge>,
        nodes: &std::collections::HashMap<u32, visionclaw_domain::models::node::Node>,
    ) {
        let before = edges.len();
        edges
            .retain(|_, edge| nodes.contains_key(&edge.source) && nodes.contains_key(&edge.target));
        let filtered = before - edges.len();
        if filtered > 0 {
            info!("Filtered {} orphan edges", filtered);
        }
    }
}

// ------------------------------------------------------------------
// Free functions
// ------------------------------------------------------------------

/// Map a fully-expanded predicate IRI to a canonical edge-type label.
/// Returns `""` for predicates that should not create graph edges.
/// The label is looked up in `SEMANTIC_TYPE_REGISTRY` for force config;
/// unknown IRIs auto-register with defaults via `get_or_register_id`.
fn predicate_to_edge_type(iri: &str) -> &'static str {
    match iri {
        RDFS_SUBCLASS_OF => "hierarchical",
        IRI_REQUIRES | IRI_ENABLES | IRI_DEPENDS_ON => "dependency",
        IRI_HAS_PART | IRI_IS_PART_OF => "structural",
        IRI_RELATES_TO => "associative",
        IRI_BRIDGES_TO | IRI_BRIDGES_FROM => "bridge",
        IRI_IMPLEMENTS => "implements",
        IRI_ENHANCES | IRI_OPTIMIZES => "enhancement",
        IRI_SECURES | IRI_VALIDATES => "security",
        OWL_EQUIVALENT_CLASS | OWL_SAME_AS => "hierarchical",
        OWL_DISJOINT_WITH => "bridge",
        OWL_INVERSE_OF => "associative",
        RDFS_DOMAIN | RDFS_RANGE => "structural",
        RDFS_SUB_PROPERTY_OF => "hierarchical",
        PROV_WAS_DERIVED_FROM | PROV_WAS_ATTRIBUTED_TO | PROV_WAS_GENERATED_BY => "provenance",
        IRI_ACHIEVES_OBJECTIVE => "goal",
        IRI_TRACKED_ON => "tracking",
        IRI_SIMILAR_TO | IRI_SIMULATED_IN => "similarity",
        IRI_WIKILINK => "explicit_link",
        IRI_USES | IRI_SUPPORTS | IRI_UTILISES => "utilisation",
        IRI_ENABLED_BY => "dependency",
        IRI_CONTRASTS_WITH => "bridge",
        IRI_STANDARDIZED_BY => "standardisation",
        IRI_APPLIES_TO | IRI_RELATED_TO => "associative",
        IRI_PART_OF => "structural",
        IRI_INSTANCE_OF => "hierarchical",
        IRI_NGM_SAME_AS => "hierarchical",
        IRI_DEFINED_IN => "structural",
        RDF_TYPE => "",
        _ => "",
    }
}
