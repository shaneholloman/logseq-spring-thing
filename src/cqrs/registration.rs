// src/cqrs/registration.rs
//! CQRS Handler Registration
//!
//! Registers all command and query handlers on the CQRS buses.
//! Called once during AppState initialization after repositories are constructed.

use crate::cqrs::bus::{CommandBus, QueryBus};
use crate::cqrs::commands::*;
use crate::cqrs::handlers::{
    GraphCommandHandler, GraphQueryHandler, OntologyCommandHandler, OntologyQueryHandler,
    PhysicsCommandHandler, PhysicsQueryHandler,
    SettingsCommandHandler, SettingsQueryHandler,
};
use crate::cqrs::queries::*;
use crate::ports::{GpuPhysicsAdapter, KnowledgeGraphRepository, OntologyRepository, SettingsRepository};
use log::info;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Register all graph domain command handlers on the command bus.
///
/// Registers 14 command handlers covering node CRUD, edge CRUD,
/// graph persistence, and position updates.
async fn register_graph_commands(
    bus: &CommandBus,
    repo: Arc<dyn KnowledgeGraphRepository>,
) {
    let h = Arc::new(GraphCommandHandler::new(repo));

    bus.register::<AddNodeCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<AddNodesCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdateNodeCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdateNodesCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<RemoveNodeCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<RemoveNodesCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<AddEdgeCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<AddEdgesCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdateEdgeCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<RemoveEdgeCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<RemoveEdgesCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<SaveGraphCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<ClearGraphCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdatePositionsCommand>(Box::new(GraphCommandHandlerAdapter(h.clone()))).await;
}

/// Register all graph domain query handlers on the query bus.
///
/// Registers 14 query handlers covering node lookups, searches,
/// edge queries, statistics, and health checks.
async fn register_graph_queries(
    bus: &QueryBus,
    repo: Arc<dyn KnowledgeGraphRepository>,
) {
    let h = Arc::new(GraphQueryHandler::new(repo));

    bus.register::<GetNodeQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetNodesQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetAllNodesQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<SearchNodesQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetNodesByMetadataQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetNodeEdgesQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetEdgesBetweenQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetNeighborsQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<CountNodesQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<CountEdgesQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetGraphStatsQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<LoadGraphQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<QueryNodesQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GraphHealthCheckQuery>(Box::new(GraphQueryHandlerAdapter(h.clone()))).await;
}

/// Register all ontology domain command handlers on the command bus.
///
/// Registers 14 command handlers covering OWL class/property/axiom CRUD,
/// ontology persistence, inference results, and pathfinding caches.
async fn register_ontology_commands(
    bus: &CommandBus,
    repo: Arc<dyn OntologyRepository>,
) {
    let h = Arc::new(OntologyCommandHandler::new(repo));

    bus.register::<AddClassCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdateClassCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<RemoveClassCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<AddPropertyCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdatePropertyCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<RemovePropertyCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<AddAxiomCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<RemoveAxiomCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<SaveOntologyCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<SaveOntologyGraphCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<StoreInferenceResultsCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<ImportOntologyCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<CacheSsspResultCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<CacheApspResultCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
    bus.register::<InvalidatePathfindingCachesCommand>(Box::new(OntologyCommandHandlerAdapter(h.clone()))).await;
}

/// Register all ontology domain query handlers on the query bus.
///
/// Registers 14 query handlers covering OWL class/property/axiom lookups,
/// inference results, validation, metrics, and pathfinding caches.
async fn register_ontology_queries(
    bus: &QueryBus,
    repo: Arc<dyn OntologyRepository>,
) {
    let h = Arc::new(OntologyQueryHandler::new(repo));

    bus.register::<GetClassQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<ListClassesQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetClassHierarchyQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetPropertyQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<ListPropertiesQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetAxiomsForClassQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetInferenceResultsQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<ValidateOntologyQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<QueryOntologyQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetOntologyMetricsQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<LoadOntologyGraphQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<ExportOntologyQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetCachedSsspQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetCachedApspQuery>(Box::new(OntologyQueryHandlerAdapter(h.clone()))).await;
}

/// Register all settings domain command handlers on the command bus.
///
/// Registers 8 command handlers covering setting CRUD, physics profiles,
/// import, and cache management.
async fn register_settings_commands(
    bus: &CommandBus,
    repo: Arc<dyn SettingsRepository>,
) {
    let h = Arc::new(SettingsCommandHandler::new(repo));

    bus.register::<UpdateSettingCommand>(Box::new(SettingsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdateBatchSettingsCommand>(Box::new(SettingsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<DeleteSettingCommand>(Box::new(SettingsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<SaveAllSettingsCommand>(Box::new(SettingsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<SavePhysicsSettingsCommand>(Box::new(SettingsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<DeletePhysicsProfileCommand>(Box::new(SettingsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<ImportSettingsCommand>(Box::new(SettingsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<ClearSettingsCacheCommand>(Box::new(SettingsCommandHandlerAdapter(h.clone()))).await;
}

/// Register all settings domain query handlers on the query bus.
///
/// Registers 9 query handlers covering setting lookups, listing,
/// physics profiles, export, and health checks.
async fn register_settings_queries(
    bus: &QueryBus,
    repo: Arc<dyn SettingsRepository>,
) {
    let h = Arc::new(SettingsQueryHandler::new(repo));

    bus.register::<GetSettingQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetBatchSettingsQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetAllSettingsQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<ListSettingsQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<HasSettingQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetPhysicsSettingsQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<ListPhysicsProfilesQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<ExportSettingsQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<SettingsHealthCheckQuery>(Box::new(SettingsQueryHandlerAdapter(h.clone()))).await;
}

/// Register all physics domain command handlers on the command bus.
///
/// Registers 8 command handlers covering physics initialization, parameter updates,
/// graph data updates, external forces, node pinning, reset, and cleanup.
async fn register_physics_commands(
    bus: &CommandBus,
    adapter: Arc<Mutex<dyn GpuPhysicsAdapter>>,
) {
    let h = Arc::new(PhysicsCommandHandler::new(adapter));

    bus.register::<InitializePhysicsCommand>(Box::new(PhysicsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdatePhysicsParametersCommand>(Box::new(PhysicsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UpdateGraphDataCommand>(Box::new(PhysicsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<ApplyExternalForcesCommand>(Box::new(PhysicsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<PinNodesCommand>(Box::new(PhysicsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<UnpinNodesCommand>(Box::new(PhysicsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<ResetPhysicsCommand>(Box::new(PhysicsCommandHandlerAdapter(h.clone()))).await;
    bus.register::<CleanupPhysicsCommand>(Box::new(PhysicsCommandHandlerAdapter(h.clone()))).await;
}

/// Register all physics domain query handlers on the query bus.
///
/// Registers 5 query handlers covering GPU status, physics statistics,
/// device listing, performance metrics, and GPU availability checks.
async fn register_physics_queries(
    bus: &QueryBus,
    adapter: Arc<Mutex<dyn GpuPhysicsAdapter>>,
) {
    let h = Arc::new(PhysicsQueryHandler::new(adapter));

    bus.register::<GetGpuStatusQuery>(Box::new(PhysicsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetPhysicsStatisticsQuery>(Box::new(PhysicsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<ListGpuDevicesQuery>(Box::new(PhysicsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<GetPerformanceMetricsQuery>(Box::new(PhysicsQueryHandlerAdapter(h.clone()))).await;
    bus.register::<IsGpuAvailableQuery>(Box::new(PhysicsQueryHandlerAdapter(h.clone()))).await;
}

/// Register physics CQRS handlers after GPU initialization completes.
///
/// Called from the async GPU-ready callback in AppState once the
/// `PhysicsOrchestratorActor` is available. This completes the deferred
/// registration from `register_all_handlers`.
///
/// # Handler count
/// - Commands: 8 (InitializePhysics, UpdatePhysicsParameters, UpdateGraphData,
///   ApplyExternalForces, PinNodes, UnpinNodes, ResetPhysics, CleanupPhysics)
/// - Queries: 5 (GetGpuStatus, GetPhysicsStatistics, ListGpuDevices,
///   GetPerformanceMetrics, IsGpuAvailable)
/// - Total: 13 handlers
pub async fn register_physics_handlers(
    command_bus: &CommandBus,
    query_bus: &QueryBus,
    physics_adapter: Arc<Mutex<dyn GpuPhysicsAdapter>>,
) {
    register_physics_commands(command_bus, physics_adapter.clone()).await;
    register_physics_queries(query_bus, physics_adapter).await;

    let cmd_count = command_bus.handler_count().await;
    let query_count = query_bus.handler_count().await;
    info!(
        "[CQRS Registration] Physics handlers registered (8 commands, 5 queries). Bus totals: {} commands, {} queries",
        cmd_count, query_count
    );
}

/// Register all available CQRS handlers on the command and query buses.
///
/// This wires graph, ontology, and settings handlers. Physics handlers are
/// registered separately via `register_physics_handlers()` after GPU init.
///
/// # Handler count
/// - Commands: 14 graph + 15 ontology + 8 settings = 37
/// - Queries: 14 graph + 14 ontology + 9 settings = 37
/// - Total: 74 handlers (physics deferred: 8 commands + 5 queries = 13)
pub async fn register_all_handlers(
    command_bus: &CommandBus,
    query_bus: &QueryBus,
    graph_repo: Arc<dyn KnowledgeGraphRepository>,
    ontology_repo: Arc<dyn OntologyRepository>,
    settings_repo: Arc<dyn SettingsRepository>,
) {
    // Graph domain: 14 commands + 14 queries
    register_graph_commands(command_bus, graph_repo.clone()).await;
    register_graph_queries(query_bus, graph_repo).await;
    info!("[CQRS Registration] Graph handlers registered (14 commands, 14 queries)");

    // Ontology domain: 15 commands + 14 queries
    register_ontology_commands(command_bus, ontology_repo.clone()).await;
    register_ontology_queries(query_bus, ontology_repo).await;
    info!("[CQRS Registration] Ontology handlers registered (15 commands, 14 queries)");

    // Settings domain: 8 commands + 9 queries
    register_settings_commands(command_bus, settings_repo.clone()).await;
    register_settings_queries(query_bus, settings_repo).await;
    info!("[CQRS Registration] Settings handlers registered (8 commands, 9 queries)");

    // Physics domain: registered separately via register_physics_handlers()
    // after asynchronous GPU initialization completes.
    info!("[CQRS Registration] Physics handlers deferred (registered after GPU init)");

    let cmd_count = command_bus.handler_count().await;
    let query_count = query_bus.handler_count().await;
    info!(
        "CQRS bus: {} commands registered, {} queries registered",
        cmd_count, query_count
    );
}

// ---------------------------------------------------------------------------
// Adapter wrappers
// ---------------------------------------------------------------------------
// The bus expects `Box<dyn CommandHandler<C>>` for each concrete command type.
// Our domain handlers (e.g. `GraphCommandHandler`) implement `CommandHandler<T>`
// for multiple `T`. We use thin newtype adapters that hold an `Arc` to the
// shared handler and delegate the `handle` call, satisfying the bus's type
// erasure requirements without cloning the handler for each command type.

use async_trait::async_trait;
use crate::cqrs::types::{Command, CommandHandler, Query, QueryHandler, Result};

// -- Graph command adapter --------------------------------------------------

struct GraphCommandHandlerAdapter(Arc<GraphCommandHandler>);

macro_rules! impl_graph_cmd {
    ($cmd:ty) => {
        #[async_trait]
        impl CommandHandler<$cmd> for GraphCommandHandlerAdapter {
            async fn handle(&self, command: $cmd) -> Result<<$cmd as Command>::Result> {
                self.0.handle(command).await
            }
        }
    };
}

impl_graph_cmd!(AddNodeCommand);
impl_graph_cmd!(AddNodesCommand);
impl_graph_cmd!(UpdateNodeCommand);
impl_graph_cmd!(UpdateNodesCommand);
impl_graph_cmd!(RemoveNodeCommand);
impl_graph_cmd!(RemoveNodesCommand);
impl_graph_cmd!(AddEdgeCommand);
impl_graph_cmd!(AddEdgesCommand);
impl_graph_cmd!(UpdateEdgeCommand);
impl_graph_cmd!(RemoveEdgeCommand);
impl_graph_cmd!(RemoveEdgesCommand);
impl_graph_cmd!(SaveGraphCommand);
impl_graph_cmd!(ClearGraphCommand);
impl_graph_cmd!(UpdatePositionsCommand);

// -- Graph query adapter ----------------------------------------------------

struct GraphQueryHandlerAdapter(Arc<GraphQueryHandler>);

macro_rules! impl_graph_query {
    ($query:ty) => {
        #[async_trait]
        impl QueryHandler<$query> for GraphQueryHandlerAdapter {
            async fn handle(&self, query: $query) -> Result<<$query as Query>::Result> {
                self.0.handle(query).await
            }
        }
    };
}

impl_graph_query!(GetNodeQuery);
impl_graph_query!(GetNodesQuery);
impl_graph_query!(GetAllNodesQuery);
impl_graph_query!(SearchNodesQuery);
impl_graph_query!(GetNodesByMetadataQuery);
impl_graph_query!(GetNodeEdgesQuery);
impl_graph_query!(GetEdgesBetweenQuery);
impl_graph_query!(GetNeighborsQuery);
impl_graph_query!(CountNodesQuery);
impl_graph_query!(CountEdgesQuery);
impl_graph_query!(GetGraphStatsQuery);
impl_graph_query!(LoadGraphQuery);
impl_graph_query!(QueryNodesQuery);
impl_graph_query!(GraphHealthCheckQuery);

// -- Ontology command adapter -----------------------------------------------

struct OntologyCommandHandlerAdapter(Arc<OntologyCommandHandler>);

macro_rules! impl_onto_cmd {
    ($cmd:ty) => {
        #[async_trait]
        impl CommandHandler<$cmd> for OntologyCommandHandlerAdapter {
            async fn handle(&self, command: $cmd) -> Result<<$cmd as Command>::Result> {
                self.0.handle(command).await
            }
        }
    };
}

impl_onto_cmd!(AddClassCommand);
impl_onto_cmd!(UpdateClassCommand);
impl_onto_cmd!(RemoveClassCommand);
impl_onto_cmd!(AddPropertyCommand);
impl_onto_cmd!(UpdatePropertyCommand);
impl_onto_cmd!(RemovePropertyCommand);
impl_onto_cmd!(AddAxiomCommand);
impl_onto_cmd!(RemoveAxiomCommand);
impl_onto_cmd!(SaveOntologyCommand);
impl_onto_cmd!(SaveOntologyGraphCommand);
impl_onto_cmd!(StoreInferenceResultsCommand);
impl_onto_cmd!(ImportOntologyCommand);
impl_onto_cmd!(CacheSsspResultCommand);
impl_onto_cmd!(CacheApspResultCommand);
impl_onto_cmd!(InvalidatePathfindingCachesCommand);

// -- Ontology query adapter -------------------------------------------------

struct OntologyQueryHandlerAdapter(Arc<OntologyQueryHandler>);

macro_rules! impl_onto_query {
    ($query:ty) => {
        #[async_trait]
        impl QueryHandler<$query> for OntologyQueryHandlerAdapter {
            async fn handle(&self, query: $query) -> Result<<$query as Query>::Result> {
                self.0.handle(query).await
            }
        }
    };
}

impl_onto_query!(GetClassQuery);
impl_onto_query!(ListClassesQuery);
impl_onto_query!(GetClassHierarchyQuery);
impl_onto_query!(GetPropertyQuery);
impl_onto_query!(ListPropertiesQuery);
impl_onto_query!(GetAxiomsForClassQuery);
impl_onto_query!(GetInferenceResultsQuery);
impl_onto_query!(ValidateOntologyQuery);
impl_onto_query!(QueryOntologyQuery);
impl_onto_query!(GetOntologyMetricsQuery);
impl_onto_query!(LoadOntologyGraphQuery);
impl_onto_query!(ExportOntologyQuery);
impl_onto_query!(GetCachedSsspQuery);
impl_onto_query!(GetCachedApspQuery);

// -- Settings command adapter -----------------------------------------------

struct SettingsCommandHandlerAdapter(Arc<SettingsCommandHandler>);

macro_rules! impl_settings_cmd {
    ($cmd:ty) => {
        #[async_trait]
        impl CommandHandler<$cmd> for SettingsCommandHandlerAdapter {
            async fn handle(&self, command: $cmd) -> Result<<$cmd as Command>::Result> {
                self.0.handle(command).await
            }
        }
    };
}

impl_settings_cmd!(UpdateSettingCommand);
impl_settings_cmd!(UpdateBatchSettingsCommand);
impl_settings_cmd!(DeleteSettingCommand);
impl_settings_cmd!(SaveAllSettingsCommand);
impl_settings_cmd!(SavePhysicsSettingsCommand);
impl_settings_cmd!(DeletePhysicsProfileCommand);
impl_settings_cmd!(ImportSettingsCommand);
impl_settings_cmd!(ClearSettingsCacheCommand);

// -- Settings query adapter -------------------------------------------------

struct SettingsQueryHandlerAdapter(Arc<SettingsQueryHandler>);

macro_rules! impl_settings_query {
    ($query:ty) => {
        #[async_trait]
        impl QueryHandler<$query> for SettingsQueryHandlerAdapter {
            async fn handle(&self, query: $query) -> Result<<$query as Query>::Result> {
                self.0.handle(query).await
            }
        }
    };
}

impl_settings_query!(GetSettingQuery);
impl_settings_query!(GetBatchSettingsQuery);
impl_settings_query!(GetAllSettingsQuery);
impl_settings_query!(ListSettingsQuery);
impl_settings_query!(HasSettingQuery);
impl_settings_query!(GetPhysicsSettingsQuery);
impl_settings_query!(ListPhysicsProfilesQuery);
impl_settings_query!(ExportSettingsQuery);
impl_settings_query!(SettingsHealthCheckQuery);

// -- Physics command adapter -------------------------------------------------

struct PhysicsCommandHandlerAdapter(Arc<PhysicsCommandHandler>);

macro_rules! impl_physics_cmd {
    ($cmd:ty) => {
        #[async_trait]
        impl CommandHandler<$cmd> for PhysicsCommandHandlerAdapter {
            async fn handle(&self, command: $cmd) -> Result<<$cmd as Command>::Result> {
                self.0.handle(command).await
            }
        }
    };
}

impl_physics_cmd!(InitializePhysicsCommand);
impl_physics_cmd!(UpdatePhysicsParametersCommand);
impl_physics_cmd!(UpdateGraphDataCommand);
impl_physics_cmd!(ApplyExternalForcesCommand);
impl_physics_cmd!(PinNodesCommand);
impl_physics_cmd!(UnpinNodesCommand);
impl_physics_cmd!(ResetPhysicsCommand);
impl_physics_cmd!(CleanupPhysicsCommand);

// -- Physics query adapter ---------------------------------------------------

struct PhysicsQueryHandlerAdapter(Arc<PhysicsQueryHandler>);

macro_rules! impl_physics_query {
    ($query:ty) => {
        #[async_trait]
        impl QueryHandler<$query> for PhysicsQueryHandlerAdapter {
            async fn handle(&self, query: $query) -> Result<<$query as Query>::Result> {
                self.0.handle(query).await
            }
        }
    };
}

impl_physics_query!(GetGpuStatusQuery);
impl_physics_query!(GetPhysicsStatisticsQuery);
impl_physics_query!(ListGpuDevicesQuery);
impl_physics_query!(GetPerformanceMetricsQuery);
impl_physics_query!(IsGpuAvailableQuery);
