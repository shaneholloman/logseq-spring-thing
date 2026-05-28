//! Client-domain messages: WebSocket client registration, broadcast,
//! authentication, filtering, initial graph load, and position streaming.
//!
//! Domain-safe types have been moved to `visionclaw_actors::messages::client_messages`.
//! This file re-exports them and defines the webxr-internal types that cannot move.

// ---------------------------------------------------------------------------
// Re-export domain-safe types from the domain crate
// ---------------------------------------------------------------------------

pub use visionclaw_actors::messages::client_messages::{
    AuthenticateClient, BroadcastMessage, BroadcastNodePositions,
    ClientBroadcastAck, ForcePositionBroadcast, GetClientCount, InitialClientSync,
    SendToClientBinary, SendToClientText, UnregisterClient, UpdateClientFilter,
};

// ---------------------------------------------------------------------------
// Webxr-internal types (cannot move to domain crate)
// ---------------------------------------------------------------------------

use actix::prelude::*;

use crate::utils::socket_flow_messages::{InitialEdgeData, InitialNodeData};

/// Register a new WebSocket client with the client manager.
/// Blocked: references `Addr<handlers::socket_flow_handler::SocketFlowServer>`.
#[derive(Message)]
#[rtype(result = "Result<usize, String>")]
pub struct RegisterClient {
    pub addr: actix::Addr<crate::handlers::socket_flow_handler::SocketFlowServer>,
}

/// Broadcast positions to all connected clients.
/// Blocked: references `utils::socket_flow_messages::BinaryNodeDataClient`.
#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastPositions {
    pub positions: Vec<crate::utils::socket_flow_messages::BinaryNodeDataClient>,
}

/// Set the graph service supervisor address in client manager.
/// Blocked: references `Addr<actors::GraphServiceSupervisor>`.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetGraphServiceAddress {
    pub addr: actix::Addr<crate::actors::GraphServiceSupervisor>,
}

/// WebSocket protocol: Initial graph load.
/// Blocked: references `utils::socket_flow_messages::{InitialNodeData, InitialEdgeData}`.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendInitialGraphLoad {
    pub nodes: Vec<InitialNodeData>,
    pub edges: Vec<InitialEdgeData>,
}

/// WebSocket protocol: Streamed position updates indexed by node ID.
/// Blocked: field types from socket_flow_messages context.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendPositionUpdate {
    pub node_id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
}
