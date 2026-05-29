//! Agent-event ingest (ADR-059, VisionClaw side).
//!
//! The canonical agent-action wire envelope, mirrored from the agentbox
//! schema source (`management-api/utils/agent-event-publisher.js`). Phase 1
//! lands the schema; Phase 2 adds the `/wss/agent-events` transport that
//! consumes it. agentbox ADR-014 is the producer half of this contract.
//!
//! Phase 2 (this increment): [`ingest`] is the authenticated `/wss/agent-events`
//! handler that parses + validates inbound `notifications/agent_action` and
//! publishes each envelope to the process-global [`hub`]. The beam + gluon GPU
//! render actor (ADR-059 §4, Phase 2b) subscribes to the hub — that render path
//! and the `:9500` state-poll cutover are scoped follow-ons.

pub mod hub;
pub mod ingest;
pub mod schema;

pub use ingest::agent_events_ws;
pub use schema::{AgentActionEnvelope, AgentActionNotification, AgentActionParams};
