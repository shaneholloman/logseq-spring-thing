//! Domain-safe message types for the visionflow actor system.
//!
//! These messages depend only on `visionflow-domain` and standard crates.
//! Webxr-internal message types (those referencing `config::AppFullSettings`,
//! `gpu::visual_analytics`, `utils::socket_flow_messages`, `handlers::*`,
//! `services::*`) remain in `webxr::src::actors::messages::*`.

pub mod graph_messages;

pub use graph_messages::{
    AddEdge, AddNode, AddNodesFromMetadata, ArchiveWorkspace, AutoBalanceNotification,
    BuildGraphFromMetadata, CreateWorkspace, DeleteWorkspace, GetAutoBalanceNotifications,
    GetGraphData, GetMetadata, GetNodeIdMapping, GetNodeMap, GetNodePositions,
    GetNodeTypeArrays, GetPositionFrameSnapshot, GetWorkspace, GetWorkspaceCount,
    GetWorkspaces, InitializeActor, LoadWorkspaces, NodeIdMapping, NodeTypeArrays,
    PositionFrameSnapshot, PositionRow, RefreshMetadata, ReloadGraphFromDatabase, RemoveEdge,
    RemoveNode, RemoveNodeByMetadata, SaveWorkspaces, ToggleFavoriteWorkspace,
    UpdateGraphData, UpdateMetadata, UpdateNodeFromMetadata, UpdateNodePosition,
    UpdateNodePositions, UpdateNodeTypeArrays, UpdateWorkspace, WorkspaceChangeType,
    WorkspaceStateChanged,
};
