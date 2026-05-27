// ADR-090 Phase 1b: OntologyRepository promoted to visionflow-domain.
// This file is a pure re-export shim — callers need no changes.
//
// OWL types (OwlClass, OwlAxiom, etc.) are still re-exported here via the
// domain re-export chain, so `use crate::ports::ontology_repository::OwlClass`
// continues to work unchanged.
pub use visionflow_domain::ports::ontology_repository::*;
