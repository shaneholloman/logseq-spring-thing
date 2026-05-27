//! Layout types — re-exported from `visionflow-domain` per ADR-090.
//!
//! Originally defined here; moved to the domain crate so changes to the
//! `LayoutMode` enum (etc.) only recompile the domain crate plus the linker.

pub use visionflow_domain::types::layout::{
    ConstraintZone, LayoutMode, LayoutModeConfig, LayoutStatus,
};
