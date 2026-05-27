//! VisionFlow Adapters Crate
//!
//! Adapter implementations per ADR-090 Phase 2 (Hexagonal Crate Modularisation).
//! This crate implements the port traits defined in `visionflow-domain` using
//! real infrastructure: Oxigraph (RDF/SPARQL) and Whelk (EL reasoning).
//!
//! ## What lives here
//! - `oxigraph_ontology_repository` — OWL ontology persistence over Oxigraph quad-store
//! - `whelk_inference_engine` — EL reasoning via horned-owl + whelk-rs
//! - `messages` — Actix actor message types for GPU adapter bridging
//!
//! ## What stayed in webxr (Phase 2 — TODO in later phases)
//! - `actor_graph_repository` — depends on `crate::actors::graph_state_actor`
//! - `actix_physics_adapter` — depends on `crate::actors::physics_orchestrator_actor`
//! - `actix_semantic_adapter` — depends on `crate::actors::semantic_processor_actor`
//! - `gpu_semantic_analyzer` — depends on `crate::utils::unified_gpu_compute`
//! - `oxigraph_graph_repository` — depends on `crate::actors::graph_actor` + socket_flow_messages
//! - `physics_orchestrator_adapter` — depends on `crate::actors::*` + `crate::utils::socket_flow_messages`
//! - `sqlite_settings_repository` — depends on `crate::config::AppFullSettings` (not in domain yet)
//!
//! DAG position: `contracts → domain → adapters`

// Phase 2 actually-moved adapters (the only ones that have zero deps on
// webxr-local types like crate::actors::*, crate::config::AppFullSettings,
// or crate::utils::socket_flow_messages — see "what stayed in webxr" above).
pub mod whelk_inference_engine;
pub use whelk_inference_engine::WhelkInferenceEngine;

// ADR-090 Phase A1+ — adapters unblocked by Phase 1b (domain now owns
// GraphData/Node/Edge, GpuPhysicsAdapter, GpuSemanticAnalyzer, OntologyRepository).
pub mod messages;
pub mod oxigraph_ontology_repository;
pub use oxigraph_ontology_repository::OxigraphOntologyRepository;
