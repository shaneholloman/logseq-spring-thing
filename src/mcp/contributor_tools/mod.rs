//! Contributor Studio + Skill Dojo MCP tool registry.
//!
//! Exposes the 9 tools proposed by ADR-057 §MCP Tool Additions and
//! `docs/design/2026-04-20-contributor-studio/02-skill-dojo-and-evals.md`
//! §5.1:
//!
//! - `skill_publish`
//! - `skill_install`
//! - `skill_evals_run` (canonical; alias `skill_eval_run` also accepted)
//! - `studio_context_assemble`
//! - `sensei_nudge`
//! - `share_intent_create`
//! - `automation_schedule`
//! - `inbox_ack`
//! - `studio_run_skill`
//!
//! Each tool carries an input schema + output schema (both JSON Schema
//! Draft-07 structures, kept as `serde_json::Value` so we can emit them
//! verbatim over the MCP wire format). Each tool has a thin handler stub
//! that validates the envelope and forwards to its owning service.
//!
//! This file is the **registration point**. The dispatcher is consumed by
//! `crate::handlers::mcp_relay_handler` when a locally-dispatched tool call
//! is detected (i.e. the orchestrator is unreachable or the tool is flagged
//! `local_first`).

pub mod automation_tools;
pub mod skill_tools;
pub mod studio_tools;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;

/// A single MCP tool definition. Mirrors the MCP `tools/list` shape but
/// carries our internal dispatcher alongside the schemas.
pub struct ToolDefinition {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    /// JSON Schema Draft-07 for the request payload.
    pub input_schema: Value,
    /// JSON Schema Draft-07 for the successful response.
    pub output_schema: Value,
    /// Which service slice owns the real implementation. Used by stubs to
    /// return a meaningful `NotImplemented` when the slice is not yet wired.
    pub owner_slice: OwnerSlice,
    /// Stub dispatcher. Always succeeds in validating + logging, and returns
    /// a `ToolOutcome::NotImplemented` when the owner slice is still unbuilt.
    pub dispatcher: ToolDispatcher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OwnerSlice {
    /// C1 — ContextAssemblyActor / ContributorStudioSupervisor.
    C1ContributorStudio,
    /// C2 — SkillRegistrySupervisor / SkillEvaluationActor.
    C2SkillRegistry,
    /// C3 — Studio React surface (client-only; backend path is routing-only).
    C3StudioSurface,
    /// C4 — ShareOrchestratorActor.
    C4ShareOrchestrator,
    /// C5 — AutomationOrchestratorActor + /inbox writer.
    C5Automation,
}

impl OwnerSlice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::C1ContributorStudio => "C1:ContributorStudioSupervisor",
            Self::C2SkillRegistry => "C2:SkillRegistrySupervisor",
            Self::C3StudioSurface => "C3:StudioSurface",
            Self::C4ShareOrchestrator => "C4:ShareOrchestratorActor",
            Self::C5Automation => "C5:AutomationOrchestratorActor",
        }
    }
}

pub type ToolDispatcher =
    Arc<dyn Fn(&ToolInvocation) -> Result<ToolOutcome, ToolDispatchError> + Send + Sync>;

/// An incoming MCP `tools/call` envelope, normalised.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolInvocation {
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
}

/// A tool dispatch outcome. The stub path always returns `NotImplemented`
/// with a clear `owner_slice` pointer; when the owning service is wired it
/// will instead return `Ok(value)`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolOutcome {
    Ok {
        value: Value,
    },
    NotImplemented {
        owner_slice: &'static str,
        message: String,
    },
}

#[derive(Debug, Error)]
pub enum ToolDispatchError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("schema validation failed for tool {tool}: {reason}")]
    SchemaInvalid { tool: String, reason: String },
    #[error("argument envelope must be a JSON object for tool {0}")]
    NotAnObject(String),
}

/// The read-only registry snapshot. Built once; cloneable via `Arc`.
pub struct ContributorToolRegistry {
    by_name: BTreeMap<String, Arc<ToolDefinition>>,
}

impl ContributorToolRegistry {
    pub fn new() -> Self {
        let defs: Vec<ToolDefinition> = vec![
            skill_tools::skill_publish_definition(),
            skill_tools::skill_install_definition(),
            skill_tools::skill_evals_run_definition(),
            studio_tools::studio_context_assemble_definition(),
            studio_tools::sensei_nudge_definition(),
            studio_tools::share_intent_create_definition(),
            studio_tools::studio_run_skill_definition(),
            automation_tools::automation_schedule_definition(),
            automation_tools::inbox_ack_definition(),
        ];

        let mut by_name: BTreeMap<String, Arc<ToolDefinition>> = BTreeMap::new();
        for def in defs {
            let arc = Arc::new(def);
            by_name.insert(arc.name.to_string(), arc.clone());
            for alias in arc.aliases {
                by_name.insert((*alias).to_string(), arc.clone());
            }
        }
        Self { by_name }
    }

    /// Unique canonical tool count (aliases excluded).
    pub fn canonical_len(&self) -> usize {
        let mut seen = std::collections::BTreeSet::new();
        for def in self.by_name.values() {
            seen.insert(def.name);
        }
        seen.len()
    }

    /// Total registered names including aliases.
    pub fn total_registrations(&self) -> usize {
        self.by_name.len()
    }

    pub fn get(&self, name: &str) -> Option<Arc<ToolDefinition>> {
        self.by_name.get(name).cloned()
    }

    pub fn iter_unique(&self) -> impl Iterator<Item = &Arc<ToolDefinition>> {
        let mut seen = std::collections::BTreeSet::new();
        self.by_name
            .values()
            .filter(move |def| seen.insert(def.name))
    }

    /// Emit an MCP `tools/list`-shaped JSON array for the registered tools.
    pub fn as_tools_list(&self) -> Value {
        let tools: Vec<Value> = self
            .iter_unique()
            .map(|def| {
                serde_json::json!({
                    "name": def.name,
                    "description": def.description,
                    "inputSchema": def.input_schema.clone(),
                    "outputSchema": def.output_schema.clone(),
                    "x-owner-slice": def.owner_slice.as_str(),
                    "x-aliases": def.aliases,
                })
            })
            .collect();
        Value::Array(tools)
    }

    pub fn dispatch(
        &self,
        invocation: &ToolInvocation,
    ) -> Result<ToolOutcome, ToolDispatchError> {
        let def = self
            .get(&invocation.tool)
            .ok_or_else(|| ToolDispatchError::UnknownTool(invocation.tool.clone()))?;

        // Minimum envelope check — full JSON Schema validation is deferred to
        // the caller's MCP layer (the relay validates against the published
        // schema before it reaches us). We still insist on an object envelope
        // so downstream service code can rely on `arguments.as_object()`.
        if !invocation.arguments.is_object() {
            return Err(ToolDispatchError::NotAnObject(def.name.to_string()));
        }

        // Required-fields shallow check — belt-and-braces even though MCP
        // runtime validates. Keeps the stub honest during integration tests.
        if let Some(required) = def
            .input_schema
            .get("required")
            .and_then(|r| r.as_array())
        {
            let obj = invocation.arguments.as_object().unwrap();
            for req in required {
                if let Some(key) = req.as_str() {
                    if !obj.contains_key(key) {
                        return Err(ToolDispatchError::SchemaInvalid {
                            tool: def.name.to_string(),
                            reason: format!("missing required field: {}", key),
                        });
                    }
                }
            }
        }

        (def.dispatcher)(invocation)
    }
}

impl Default for ContributorToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience constructor used by the MCP relay at startup.
pub fn contributor_tool_registry() -> Arc<ContributorToolRegistry> {
    Arc::new(ContributorToolRegistry::new())
}

/// Free-function dispatcher that takes a registry + invocation.
pub fn dispatch_contributor_tool(
    registry: &ContributorToolRegistry,
    invocation: &ToolInvocation,
) -> Result<ToolOutcome, ToolDispatchError> {
    registry.dispatch(invocation)
}

/// Builds a `ToolOutcome::NotImplemented` carrying the owning slice. Used by
/// every stub until C1–C5 wire real services.
pub(crate) fn not_implemented_stub(
    owner_slice: OwnerSlice,
    tool_name: &str,
    invocation: &ToolInvocation,
) -> ToolOutcome {
    log::warn!(
        "[mcp::contributor_tools] stub dispatch for `{tool}` (owner {owner}); \
         payload keys={keys:?}",
        tool = tool_name,
        owner = owner_slice.as_str(),
        keys = invocation
            .arguments
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    ToolOutcome::NotImplemented {
        owner_slice: owner_slice.as_str(),
        message: format!(
            "Tool `{}` registered but backing service ({}) not yet wired. \
             Schema validated; payload accepted.",
            tool_name,
            owner_slice.as_str()
        ),
    }
}
