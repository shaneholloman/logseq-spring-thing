//! Minimal `GraphRepository` trait shim for the JSON-LD ingest pipeline.
//!
//! The full `ports::graph_repository::GraphRepository` in webxr depends on
//! `actors::graph_actor` and cannot move to this crate yet (ADR-090 Phase A4).
//! The ingest pipeline only *holds* an `Arc<dyn GraphRepository>` as an M2
//! placeholder — it never calls any methods on it. This shim satisfies the
//! type system without pulling in webxr-internal deps.
//!
//! When `ports::graph_repository` graduates to `visionclaw-domain`, replace
//! this shim with the real trait and remove this file.

/// Marker trait — the real method surface lives in webxr's
/// `ports::graph_repository::GraphRepository`.  The ingest pipeline
/// holds an `Arc<dyn GraphRepository>` purely as an M2 placeholder.
pub trait GraphRepository: Send + Sync {}
