//! Physics configuration — re-exported from `visionclaw-domain`.
//!
//! Per ADR-090, the canonical struct definitions live in the domain crate.
//! This shim keeps existing `use crate::config::PhysicsSettings;` imports
//! working unchanged while letting changes to the struct shapes recompile
//! only the small domain crate (~3s) instead of the full monolith.

pub use visionclaw_domain::types::physics_config::{
    AutoBalanceConfig, AutoPauseConfig, ClusteringConfiguration, ConstraintSystem,
    LegacyConstraintData, PhysicsSettings, PhysicsUpdate,
};
