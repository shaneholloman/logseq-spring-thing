pub mod agent_visualization_processor;
pub mod agent_visualization_protocol;
pub mod bots_client;
pub mod file_service;
pub mod github;
pub mod github_sync_service;
pub mod local_file_sync_service;
pub mod management_api_client;
pub mod multi_mcp_agent_discovery;
pub mod natural_language_query_service;
pub mod parsers;
pub mod graph_serialization;
pub mod mcp_relay_manager;
pub mod nostr_service;
pub mod server_identity;
pub mod owl_validator;
pub mod perplexity_service;
pub mod ragflow_service;
pub mod schema_service;
pub mod semantic_analyzer;
pub mod semantic_pathfinding_service;
pub mod audio_router;
pub mod speech_service;
pub mod speech_voice_integration;
pub mod voice_context_manager;
pub mod voice_tag_manager;
pub mod ontology_converter;
pub mod edge_classifier;
pub mod ontology_reasoner;
pub mod ontology_enrichment_service;
pub mod ontology_reasoning_service;
pub mod ontology_pipeline_service;
pub mod ontology_content_analyzer;
pub mod ontology_file_cache;
pub mod pathfinding;
pub mod semantic_type_registry;
pub mod ontology_query_service;
pub mod ontology_mutation_service;
pub mod github_pr_service;
pub mod briefing_service;
pub mod bead_types;
pub mod bead_store;
pub mod bead_lifecycle;
pub mod nostr_bead_publisher;
pub mod nostr_bridge;
pub mod policy_evaluation_service;
pub mod kpi_computation_service;
pub mod pod_client;
pub mod ingest_saga;

// BC18 Contributor Studio — share funnel (agent C4, ADR-052 double-gate).
pub mod share_policy;
pub mod share_orchestrator;
pub mod wac_mutator;

// ADR-054: URN-Solid + solid-schema + Solid-Apps ecosystem alignment
pub mod urn_solid_mapping;

// ADR-051 BRIDGE_TO promotion + orphan retraction
pub mod bridge_edge;
pub mod orphan_retraction;

// Prometheus / OpenMetrics registry (task #18)
pub mod metrics;

pub use bridge_edge::{
    bridge_edge_enabled, sigmoid_confidence, BridgeEdgeService, CandidateStatus,
    MigrationCandidate, SignalVector,
};
pub use orphan_retraction::{OrphanRetractionTask, RetractionReport};

// Re-export semantic type registry types for convenience
pub use semantic_type_registry::{
    DynamicForceConfigGPU, RelationshipForceConfig, SemanticTypeRegistry,
    SEMANTIC_TYPE_REGISTRY,
};
