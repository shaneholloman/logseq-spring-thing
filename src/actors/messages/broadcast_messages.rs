//! Messages for the Phase 3 `BroadcastActor`.
//!
//! Domain-safe types have been moved to `visionflow_actors::messages::broadcast_messages`.
//! This file re-exports them and defines the webxr-internal types that cannot move.

// ---------------------------------------------------------------------------
// Re-export domain-safe types from the domain crate
// ---------------------------------------------------------------------------

pub use visionflow_actors::messages::broadcast_messages::{
    BroadcastActorStatus, BroadcastState, BroadcastTick, ClientId,
    GetBroadcastActorStatus, OnLayoutDestabilised, OnLayoutSettled, OnLayoutStarted,
    OnPhysicsClamped, ShutdownBroadcastActor, TriggerHeartbeat, UnregisterBroadcastClient,
};

// ---------------------------------------------------------------------------
// Webxr-internal types (cannot move to domain crate)
// ---------------------------------------------------------------------------

use actix::prelude::*;
use actix::Recipient;

use crate::actors::messages::SendToClientBinary;

/// Register a new client with the broadcast actor.
///
/// Blocked: references `Recipient<SendToClientBinary>` where `SendToClientBinary`
/// is re-exported from the domain crate but `Recipient<T>` binds to the concrete
/// webxr message type used in actor handler impls here.
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterBroadcastClient {
    pub client_id: ClientId,
    pub recipient: Recipient<SendToClientBinary>,
}
