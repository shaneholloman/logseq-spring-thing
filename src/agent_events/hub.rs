//! Process-global pub/sub seam for inbound `agent_action` events (ADR-059 Phase 2).
//!
//! The `/wss/agent-events` ingest handler (`super::ingest`) publishes every
//! validated [`AgentActionEnvelope`] here. The future beam + gluon render actor
//! (ADR-059 §4) subscribes via [`subscribe`] without the ingest path needing to
//! know it exists — this is the clean attach point that keeps the verifiable
//! ingest seam decoupled from the (still latent) GPU render substrate.
//!
//! Keeping the hub process-global — one bus per server process — mirrors the
//! existing `GLOBAL_CUDA_ERROR_HANDLER` / `WEBSOCKET_RATE_LIMITER` singletons
//! rather than threading a new field through the large `AppState` constructor.

use once_cell::sync::Lazy;
use tokio::sync::broadcast;

use super::schema::AgentActionEnvelope;

/// Bounded fan-out buffer. Per ADR-059 open-question 3 the visual is
/// duration-based / last-write-wins, so dropping the oldest frame under
/// backpressure is acceptable — `broadcast` does exactly that (lagged receivers
/// observe `RecvError::Lagged` and resync on the next frame).
const HUB_CAPACITY: usize = 256;

static AGENT_EVENT_HUB: Lazy<broadcast::Sender<AgentActionEnvelope>> =
    Lazy::new(|| broadcast::channel(HUB_CAPACITY).0);

/// Publish a validated inbound envelope to all subscribers.
///
/// Returns the number of live receivers the frame reached. `0` is the expected
/// steady state until the Phase 2b render actor subscribes — the ingest path is
/// intentionally useful (parse + validate + buffer) before render exists.
pub fn publish(event: AgentActionEnvelope) -> usize {
    AGENT_EVENT_HUB.send(event).unwrap_or(0)
}

/// Subscribe to the inbound `agent_action` stream. Consumed by the beam + gluon
/// render actor (ADR-059 §4, Phase 2b).
pub fn subscribe() -> broadcast::Receiver<AgentActionEnvelope> {
    AGENT_EVENT_HUB.subscribe()
}
