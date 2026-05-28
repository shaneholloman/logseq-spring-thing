//! Domain-safe broadcast message types.
//!
//! Blocked in webxr (stay in src/actors/messages/broadcast_messages.rs):
//!   - `RegisterBroadcastClient` — refs `Recipient<SendToClientBinary>` where
//!     `SendToClientBinary` is a webxr-local type.
//!
//! Everything else is pure data or uses only actix primitives.

use actix::prelude::*;

/// Opaque identifier for a connected WebSocket client.
pub type ClientId = usize;

/// Remove a client from the broadcast registry.
#[derive(Message)]
#[rtype(result = "()")]
pub struct UnregisterBroadcastClient {
    pub client_id: ClientId,
}

/// Physics emitted `LayoutStarted` — a new layout epoch is beginning.
#[derive(Message)]
#[rtype(result = "()")]
pub struct OnLayoutStarted;

/// Physics emitted `LayoutSettled` — the layout has converged.
#[derive(Message)]
#[rtype(result = "()")]
pub struct OnLayoutSettled;

/// Physics emitted `LayoutDestabilised` — the layout has been perturbed.
#[derive(Message)]
#[rtype(result = "()")]
pub struct OnLayoutDestabilised;

/// Physics emitted `PhysicsClamped` — informational only; no protocol effect.
#[derive(Message)]
#[rtype(result = "()")]
pub struct OnPhysicsClamped;

/// Internal timer message: wall-clock heartbeat fired while in SETTLED state.
#[derive(Message)]
#[rtype(result = "()")]
pub struct TriggerHeartbeat;

/// Internal timer message: 100ms poll fired while in ACTIVE state.
#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastTick;

/// Graceful shutdown — cancel timers, drain, stop the actor.
#[derive(Message)]
#[rtype(result = "()")]
pub struct ShutdownBroadcastActor;

/// Diagnostic query: return a snapshot of the actor's current observable state.
#[derive(Message)]
#[rtype(result = "BroadcastActorStatus")]
pub struct GetBroadcastActorStatus;

#[derive(Debug, Clone, PartialEq)]
pub enum BroadcastState {
    Active,
    Settled,
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct BroadcastActorStatus {
    pub state: BroadcastState,
    pub client_count: usize,
    /// Total number of V3 frames emitted to all clients combined.
    pub frames_sent_total: u64,
    /// Total V3 frames dropped due to per-client backpressure.
    pub frames_dropped_total: u64,
    /// Total wall-clock heartbeats that fired in SETTLED state.
    pub heartbeats_fired_total: u64,
}
