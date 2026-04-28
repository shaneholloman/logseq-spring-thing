pub mod actors;
pub mod adapters;
pub mod agent_events;
pub mod layout;
pub mod app_state;
pub mod application;
pub mod client;
pub mod config;
pub mod constraints;
pub mod cqrs;
pub mod domain;
pub mod errors;
pub mod events;
pub mod gpu;
pub mod handlers;
pub mod inference;
/// Locally-dispatched MCP tool registry (ADR-057 Contributor Studio + Skill Dojo).
///
/// The existing `handlers::mcp_relay_handler` handles WebSocket transport;
/// this module owns the tool *surface* for contributor-enablement tools
/// that the backend answers itself.
pub mod mcp;
pub mod middleware;
// pub mod migrations; // Removed in Phase 3 - Neo4j migration complete
pub mod models;
pub mod ontology;
pub mod openapi;
pub mod reasoning;
pub mod physics;
pub mod ports;
pub mod repositories;
pub mod services;
pub mod settings;
pub mod sovereign;
pub mod telemetry;
pub mod types;
pub mod uri;

// Import utils with macro_use to make response macros available everywhere
#[macro_use]
pub mod utils;
pub mod validation;

// #[cfg(test)]
// pub mod test_settings_fix;

pub mod test_helpers;

pub use actors::{
    ClientCoordinatorActor, MetadataActor, OptimizedSettingsActor,
};
pub use app_state::AppState;
pub use models::metadata::MetadataStore;
pub use models::protected_settings::ProtectedSettings;
pub use models::simulation_params::SimulationParams;
// pub use models::ui_settings::UISettings;
pub use models::user_settings::UserSettings;

// Re-export commonly used utilities for easier access
pub use utils::json::{to_json, from_json};
pub use utils::result_helpers::safe_json_number;
pub use utils::time;
// Re-export HandlerResponse trait for response macros
pub use utils::handler_commons::HandlerResponse;
