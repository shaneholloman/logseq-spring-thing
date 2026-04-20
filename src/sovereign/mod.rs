//! Sovereign data-plane transitions (ADR-050, ADR-051, ADR-052).
//!
//! This module owns the logic for flipping a knowledge-graph node's
//! visibility between owner-sovereign (`./private/kg/…`) and globally readable
//! (`./public/kg/…`) storage. The transition is orchestrated as a Pod-first-
//! Neo4j-second saga (ADR-051) with an audit tail and an HTTP 410 Gone
//! tombstone for unpublish.
//!
//! Feature-flagged via `VISIBILITY_TRANSITIONS=true`. When disabled, all
//! public entry points return [`visibility::VisibilityError::NotEnabled`] and
//! perform no side-effects.

pub mod visibility;

pub use visibility::{
    visibility_transitions_enabled, PublishRequest, UnpublishRequest, VisibilityError,
    VisibilityNeo4jOps, VisibilityTransitionService,
};
