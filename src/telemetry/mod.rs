//! Telemetry module for comprehensive system monitoring and logging
//!
//! Provides structured logging, metrics collection, and observability
//! for the WebXR graph visualization system.
//!
//! Pure-data types are canonical in `visionclaw_domain::telemetry`.
//! This module re-exports them so existing callers `use crate::telemetry::*` are unaffected.

pub mod agent_telemetry;

// Infrastructure items from this crate
pub use agent_telemetry::{get_telemetry_logger, init_telemetry_logger, AgentTelemetryLogger};

// Pure-data types — canonical home is the domain crate; re-exported here for backward compat
pub use visionclaw_domain::telemetry::{CorrelationId, LogLevel, Position3D, TelemetryEvent};
