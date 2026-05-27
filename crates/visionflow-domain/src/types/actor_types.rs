//! Actor-layer domain types.
//!
//! Domain representations of types used by ports and actors.
//! The actor layer wraps these with Actix Message derives.

use serde::{Deserialize, Serialize};

/// Physics simulation state — domain representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsState {
    pub is_running: bool,
    pub params: crate::models::simulation_params::SimulationParams,
}

impl Default for PhysicsState {
    fn default() -> Self {
        Self {
            is_running: false,
            params: crate::models::simulation_params::SimulationParams::default(),
        }
    }
}

/// Auto-balance notification — domain representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBalanceNotification {
    pub message: String,
    pub timestamp: i64,
    pub severity: String,
}
