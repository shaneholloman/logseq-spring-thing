//! Shim: ontology module moved to `visionclaw-ontology` crate (ADR-090 Phase A4).
//! Sub-modules with webxr-internal deps (actors, physics) remain empty stubs.
pub use visionclaw_ontology::ontology::*;

/// These sub-modules originally lived here but only had empty stubs.
/// Re-exported for any existing `crate::ontology::actors::` paths.
pub mod actors {
    // Empty — actor types live in src/actors/ontology_actor.rs
}
pub mod physics {
    // Empty — physics types live in src/physics/ontology_constraints.rs
}
