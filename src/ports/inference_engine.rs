// src/ports/inference_engine.rs
//! ADR-090 Phase 2 shim — InferenceEngine trait now lives in visionflow-domain.
//! Re-exported here so existing `use crate::ports::inference_engine::*`
//! callers in webxr need no changes.
pub use visionflow_domain::ports::inference_engine::*;
