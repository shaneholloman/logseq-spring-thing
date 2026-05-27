//! Domain port traits — ADR-090 Phase 2.
//!
//! Phase 2 promotes the OWL data types and InferenceEngine trait here.
//! These are the only ports that have zero dependency on `GraphData`
//! (which is still a webxr-local type pending Phase 1b model unification).
//!
//! Remaining ports (`OntologyRepository`, `GpuPhysicsAdapter`,
//! `GpuSemanticAnalyzer`, `GraphRepository`, `SettingsRepository`) stay in
//! `src/ports/` of the webxr crate until Phase 1b unifies `GraphData`.

pub mod inference_engine;
pub mod owl_types;

// Convenience re-exports
pub use inference_engine::{
    InferenceEngine, InferenceEngineError, InferenceStatistics,
};
pub use owl_types::{
    AxiomType, InferenceResults, OwlAxiom, OwlClass, OwlCrossReference, OwlProperty,
    OwlRelationship, PropertyType,
};
