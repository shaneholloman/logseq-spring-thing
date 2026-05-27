//! Domain port traits — ADR-090 Phase 1b.
//!
//! Phase 2 promoted OWL types and InferenceEngine.
//! Phase 1b promotes GpuPhysicsAdapter, GpuSemanticAnalyzer, and
//! OntologyRepository now that GraphData is a canonical domain type.
//!
//! Remaining ports (`GraphRepository`, `SettingsRepository`) stay in
//! `src/ports/` of the webxr crate because they depend on webxr-only types
//! (`PhysicsState`, `AutoBalanceNotification`, `AppFullSettings`).

pub mod gpu_physics_adapter;
pub mod gpu_semantic_analyzer;
pub mod inference_engine;
pub mod ontology_repository;
pub mod owl_types;

// Convenience re-exports
pub use gpu_physics_adapter::{
    GpuDeviceInfo, GpuPhysicsAdapter, GpuPhysicsAdapterError, NodeForce, PhysicsParameters,
    PhysicsStatistics, PhysicsStepResult,
};
pub use gpu_semantic_analyzer::{
    ClusteringAlgorithm, CommunityDetectionResult, GpuSemanticAnalyzer, GpuSemanticAnalyzerError,
    ImportanceAlgorithm, OptimizationResult, PathfindingResult, SemanticConstraintConfig,
    SemanticStatistics,
};
pub use inference_engine::{InferenceEngine, InferenceEngineError, InferenceStatistics};
pub use ontology_repository::{
    OntologyMetrics, OntologyRepository, OntologyRepositoryError, PathfindingCacheEntry,
    ValidationReport,
};
pub use owl_types::{
    AxiomType, InferenceResults, OwlAxiom, OwlClass, OwlCrossReference, OwlProperty,
    OwlRelationship, PropertyType,
};
