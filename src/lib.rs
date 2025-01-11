#![recursion_limit = "256"]

extern crate log;

// Declare modules
pub mod utils;
pub mod app_state;
pub mod config;
pub mod handlers;
pub mod models;
pub mod services;
pub mod types;

// Re-export standard logging if needed
pub use log::{debug, error, info, warn};

// Re-export GPU compute
pub use crate::utils::gpu_compute::GPUCompute;

// Re-export socket flow handler
pub use crate::handlers::socket_flow_handler::{SocketFlowServer, socket_flow_handler};

// Public re-exports
pub use app_state::AppState;
pub use config::Settings;
pub use models::position_update::PositionUpdate;
pub use models::metadata::MetadataStore;
pub use models::simulation_params::SimulationParams;
pub use models::graph::GraphData;
pub use services::graph_service::GraphService;
pub use services::file_service::{RealGitHubService, FileService};
pub use services::perplexity_service::PerplexityService;
pub use services::ragflow_service::{RAGFlowService, RAGFlowError};
pub use services::github_service::RealGitHubPRService;

// Re-export handlers
pub use handlers::file_handler;
pub use handlers::graph_handler;
pub use handlers::pages_handler;
pub use handlers::perplexity_handler;
pub use handlers::ragflow_handler;
pub use handlers::visualization_handler;
pub use handlers::settings_handler;
