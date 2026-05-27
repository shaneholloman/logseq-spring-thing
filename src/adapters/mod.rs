// src/adapters/mod.rs
//! Hexagonal Architecture Adapters
//!
//! Per ADR-090 Phase 2: `whelk_inference_engine` has been extracted to
//! `crates/visionflow-adapters/`. All other adapters remain here pending
//! Phase 1b (model unification) and Phase 3 (actors extraction).
//!
//! Existing `use crate::adapters::X` callers need no changes — shims below
//! keep the public surface identical.

pub mod actor_graph_repository;

#[cfg(feature = "gpu")]
pub mod gpu_semantic_analyzer;

/// ADR-090 Phase 2 shim — canonical source is visionflow-adapters.
pub mod whelk_inference_engine {
    pub use visionflow_adapters::whelk_inference_engine::*;
}

#[cfg(feature = "gpu")]
pub mod actix_physics_adapter;
pub mod actix_semantic_adapter;
pub mod messages;

pub mod physics_orchestrator_adapter;

pub use actor_graph_repository::ActorGraphRepository;

#[cfg(feature = "gpu")]
pub use gpu_semantic_analyzer::GpuSemanticAnalyzerAdapter;

// Canonical persistence adapters (ADR-11)
pub mod oxigraph_ontology_repository;
pub mod oxigraph_graph_repository;
pub mod sqlite_settings_repository;

pub use oxigraph_ontology_repository::OxigraphOntologyRepository;
pub use oxigraph_graph_repository::OxigraphGraphRepository;
pub use sqlite_settings_repository::SqliteSettingsRepository;

pub use whelk_inference_engine::WhelkInferenceEngine;

#[cfg(feature = "gpu")]
pub use actix_physics_adapter::ActixPhysicsAdapter;
pub use actix_semantic_adapter::ActixSemanticAdapter;

#[cfg(test)]
mod tests;
