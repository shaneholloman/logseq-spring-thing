//! Agent-event ingest (ADR-059, VisionClaw side).
//!
//! The canonical agent-action wire envelope, mirrored from the agentbox
//! schema source (`management-api/utils/agent-event-publisher.js`). Phase 1
//! lands the schema; Phase 2 adds the `/wss/agent-events` transport that
//! consumes it. agentbox ADR-014 is the producer half of this contract.

pub mod schema;

pub use schema::{AgentActionEnvelope, AgentActionNotification, AgentActionParams};
