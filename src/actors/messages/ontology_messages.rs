//! Ontology-domain messages: OWL validation, axiom loading, inference,
//! ontology-physics constraint integration, and legacy ontology actor messages.
//!
//! Domain-safe types have been moved to `visionclaw_actors::messages::ontology_messages`.
//! This file re-exports them and defines the webxr-internal types that cannot move.

// ---------------------------------------------------------------------------
// Re-export domain-safe types from the domain crate
// ---------------------------------------------------------------------------

pub use visionclaw_actors::messages::ontology_messages::{
    ApplyOntologyConstraints, CachedOntologyInfo, ClearOntologyCaches,
    ConstraintMergeMode, ConstraintStats, GetCachedOntologies, GetConstraintStats,
    GetOntologyConstraintStats, GetOntologyHealth, GetOntologyHealthLegacy, GetValidationReport,
    LoadOntologyAxioms, OntologyConstraintStats, OntologyHealth,
    SetConstraintGroupActive, ValidateGraph, ValidationMode,
};

// ---------------------------------------------------------------------------
// Webxr-internal types (cannot move to domain crate)
// ---------------------------------------------------------------------------

use actix::prelude::*;

use crate::ontology::parser::parser::LogseqPage;

/// Update the ontology validation config.
/// Blocked: references `services::owl_validator::ValidationConfig`.
#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct UpdateOntologyMapping {
    pub config: crate::services::owl_validator::ValidationConfig,
}

/// Validate an ontology against a property graph.
/// Blocked: references `services::owl_validator::{PropertyGraph, ValidationReport}`.
#[derive(Message)]
#[rtype(result = "Result<crate::services::owl_validator::ValidationReport, String>")]
pub struct ValidateOntology {
    pub ontology_id: String,
    pub graph_data: crate::services::owl_validator::PropertyGraph,
    pub mode: ValidationMode,
}

/// Apply OWL inference rules to a set of RDF triples.
/// Blocked: references `services::owl_validator::RdfTriple`.
#[derive(Message)]
#[rtype(result = "Result<Vec<crate::services::owl_validator::RdfTriple>, String>")]
pub struct ApplyInferences {
    pub rdf_triples: Vec<crate::services::owl_validator::RdfTriple>,
    pub max_depth: Option<usize>,
}

/// Retrieve a cached ontology validation report.
/// Blocked: references `services::owl_validator::ValidationReport`.
#[derive(Message)]
#[rtype(result = "Result<Option<crate::services::owl_validator::ValidationReport>, String>")]
pub struct GetOntologyReport {
    pub report_id: Option<String>,
}

/// Process raw Logseq ontology pages into the actor's internal state.
/// Blocked: references `ontology::parser::parser::LogseqPage`.
#[derive(Message)]
#[rtype(result = "Result<(), String>")]
pub struct ProcessOntologyData {
    pub pages: Vec<LogseqPage>,
}
