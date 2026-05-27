//! Workspace model — re-exported from `visionflow-domain` per ADR-090.
//!
//! All struct definitions and inherent impls live in the domain crate now.
//! The handlers/services layer keeps working unchanged because
//! `crate::models::workspace::Workspace` resolves through this shim.

pub use visionflow_domain::models::workspace::*;
