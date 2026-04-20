//! MCP (Model Context Protocol) tool registry for locally-served tools.
//!
//! The existing `src/handlers/mcp_relay_handler.rs` actor is a pure WebSocket
//! relay that forwards traffic between Studio clients and the external
//! orchestrator. That relay stays responsible for transport. This module owns
//! the **locally-dispatched** tool surface — tools whose request/response
//! semantics the VisionClaw backend itself answers (Contributor Studio,
//! Skill Dojo, Share Orchestrator, Automation Orchestrator, Inbox).
//!
//! ADR-057 §"MCP Tool Additions" enumerates the contributor-enablement surface.
//! Design doc `docs/design/2026-04-20-contributor-studio/02-skill-dojo-and-evals.md`
//! §5.1 fixes the canonical JSON schemas for the skill-lifecycle subset.
//!
//! Tool handlers in this module are **stubs**: they validate the incoming
//! payload against the declared schema and delegate to the relevant service.
//! Where a service is still being built by another swarm slice (C1–C5), the
//! stub returns a structured `ToolError::NotImplemented` carrying the
//! responsible slice, so downstream clients can degrade gracefully.

pub mod contributor_tools;

pub use contributor_tools::{
    contributor_tool_registry, dispatch_contributor_tool, ContributorToolRegistry,
    ToolDefinition, ToolDispatchError, ToolInvocation, ToolOutcome,
};
