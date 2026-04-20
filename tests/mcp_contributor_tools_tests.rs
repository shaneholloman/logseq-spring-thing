//! ADR-057 Contributor Studio + Skill Dojo MCP tool tests.
//!
//! Scope: schema validity (structural) + dispatcher routing (every canonical
//! tool dispatches to a stub that reports the right owner slice, and unknown /
//! malformed invocations are refused).

use serde_json::{json, Value};
use webxr::mcp::{
    contributor_tool_registry, ContributorToolRegistry, ToolDispatchError, ToolInvocation,
    ToolOutcome,
};

/// The canonical tool surface ADR-057 requires us to register.
const CANONICAL_TOOLS: &[&str] = &[
    "skill_publish",
    "skill_install",
    "skill_evals_run",
    "studio_context_assemble",
    "sensei_nudge",
    "share_intent_create",
    "automation_schedule",
    "inbox_ack",
    "studio_run_skill",
];

fn default_args_for(tool: &str) -> Value {
    match tool {
        "skill_publish" => json!({
            "name": "market-analysis-brief",
            "description": "Generate an evidence-grounded brief.",
            "category": "research.finance",
            "tool_sequence": [{"step": 1}],
            "eval_suite": [
                {"id": "t01", "prompt": "test", "assertions": []},
                {"id": "t02", "prompt": "test", "assertions": []},
                {"id": "t03", "prompt": "test", "assertions": []}
            ]
        }),
        "skill_install" => json!({
            "skill_uri": "https://alice.pods.visionclaw.org/public/skills/market-analysis-brief/"
        }),
        "skill_evals_run" | "skill_eval_run" => json!({
            "skill_id": "market-analysis-brief",
            "mode": "baseline"
        }),
        "studio_context_assemble" => json!({
            "workspace_id": "wsp-abc"
        }),
        "sensei_nudge" => json!({
            "workspace_id": "wsp-abc",
            "current_focus": { "kind": "artifact", "ref": "urn:artifact:1" }
        }),
        "share_intent_create" => json!({
            "artifact_ref": { "uri": "urn:skill:1", "kind": "skill" },
            "target_scope": { "share_state": "Team", "team_slug": "research" },
            "rationale": "Proven over two weeks."
        }),
        "automation_schedule" => json!({
            "routine_spec": {
                "name": "morning-brief",
                "schedule": { "cron": "0 8 * * *" },
                "target_skill": "market-analysis-brief"
            }
        }),
        "inbox_ack" => json!({
            "brief_id": "brf-1",
            "disposition": "accept"
        }),
        "studio_run_skill" => json!({
            "workspace_id": "wsp-abc",
            "skill_id": "market-analysis-brief"
        }),
        other => panic!("no fixture for {}", other),
    }
}

fn assert_schema_well_formed(schema: &Value, tool: &str, kind: &str) {
    assert!(
        schema.is_object(),
        "{tool}.{kind}_schema must be a JSON object"
    );
    assert_eq!(
        schema.get("type").and_then(|t| t.as_str()),
        Some("object"),
        "{tool}.{kind}_schema top-level type must be 'object'"
    );
    assert!(
        schema.get("properties").is_some(),
        "{tool}.{kind}_schema must declare properties"
    );
}

fn registry() -> ContributorToolRegistry {
    ContributorToolRegistry::new()
}

#[test]
fn registry_registers_every_canonical_tool() {
    let reg = registry();
    assert_eq!(
        reg.canonical_len(),
        CANONICAL_TOOLS.len(),
        "unique tool count must equal {}",
        CANONICAL_TOOLS.len()
    );
    for name in CANONICAL_TOOLS {
        assert!(
            reg.get(name).is_some(),
            "canonical tool `{name}` must be registered"
        );
    }
}

#[test]
fn aliases_resolve_to_canonical_tools() {
    let reg = registry();
    // Singular eval alias from ADR-057 table.
    assert!(reg.get("skill_eval_run").is_some());
    assert_eq!(reg.get("skill_eval_run").unwrap().name, "skill_evals_run");
    // Legacy name surfaced in open questions.
    assert_eq!(
        reg.get("skill_registry_publish").unwrap().name,
        "skill_publish"
    );
    // Studio alias for sensei_nudge.
    assert_eq!(reg.get("studio_nudge").unwrap().name, "sensei_nudge");
}

#[test]
fn every_tool_has_well_formed_input_and_output_schemas() {
    let reg = registry();
    for name in CANONICAL_TOOLS {
        let def = reg.get(name).unwrap();
        assert_schema_well_formed(&def.input_schema, name, "input");
        assert_schema_well_formed(&def.output_schema, name, "output");
        // required array must be present and non-empty for every tool on the
        // surface — we want deterministic stub errors for missing fields.
        let required = def
            .input_schema
            .get("required")
            .and_then(|r| r.as_array())
            .unwrap_or_else(|| panic!("{name}.input_schema must declare a required[] array"));
        assert!(
            !required.is_empty(),
            "{name}.input_schema.required must not be empty"
        );
    }
}

#[test]
fn tools_list_projection_emits_nine_entries() {
    let reg = registry();
    let list = reg.as_tools_list();
    let array = list.as_array().expect("tools_list must be a JSON array");
    assert_eq!(array.len(), CANONICAL_TOOLS.len());
    for entry in array {
        assert!(entry.get("name").is_some());
        assert!(entry.get("inputSchema").is_some());
        assert!(entry.get("outputSchema").is_some());
        assert!(entry.get("x-owner-slice").is_some());
    }
}

#[test]
fn dispatch_returns_not_implemented_for_each_tool() {
    let reg = registry();
    for name in CANONICAL_TOOLS {
        let inv = ToolInvocation {
            tool: (*name).to_string(),
            arguments: default_args_for(name),
        };
        match reg.dispatch(&inv).expect("dispatch must succeed") {
            ToolOutcome::NotImplemented {
                owner_slice,
                message,
            } => {
                assert!(
                    !owner_slice.is_empty(),
                    "{name} owner_slice must be populated"
                );
                assert!(
                    message.contains(name),
                    "{name} not_implemented message must mention the tool name"
                );
            }
            ToolOutcome::Ok { .. } => panic!(
                "{name} must still be a stub — no service is wired in this scaffold"
            ),
        }
    }
}

#[test]
fn dispatch_rejects_unknown_tool() {
    let reg = registry();
    let inv = ToolInvocation {
        tool: "definitely_not_a_tool".into(),
        arguments: json!({}),
    };
    let err = reg.dispatch(&inv).unwrap_err();
    assert!(matches!(err, ToolDispatchError::UnknownTool(_)));
}

#[test]
fn dispatch_rejects_non_object_arguments() {
    let reg = registry();
    let inv = ToolInvocation {
        tool: "skill_publish".into(),
        arguments: json!("not-an-object"),
    };
    let err = reg.dispatch(&inv).unwrap_err();
    assert!(matches!(err, ToolDispatchError::NotAnObject(_)));
}

#[test]
fn dispatch_rejects_missing_required_field() {
    let reg = registry();
    let inv = ToolInvocation {
        tool: "skill_install".into(),
        arguments: json!({}),
    };
    let err = reg.dispatch(&inv).unwrap_err();
    assert!(matches!(
        err,
        ToolDispatchError::SchemaInvalid { .. }
    ));
}

#[test]
fn convenience_constructor_is_shared_registry() {
    let reg = contributor_tool_registry();
    assert_eq!(reg.canonical_len(), CANONICAL_TOOLS.len());
    for name in CANONICAL_TOOLS {
        assert!(reg.get(name).is_some());
    }
}

// Per-tool dispatch tests — one per canonical tool, asserting the owner_slice
// label that will be carried in the MCP response. These make refactors
// against the slice boundary obvious.

macro_rules! per_tool_dispatch_test {
    ($test_name:ident, $tool:expr, $expected_owner_prefix:expr) => {
        #[test]
        fn $test_name() {
            let reg = registry();
            let inv = ToolInvocation {
                tool: $tool.to_string(),
                arguments: default_args_for($tool),
            };
            match reg.dispatch(&inv).unwrap() {
                ToolOutcome::NotImplemented { owner_slice, .. } => {
                    assert!(
                        owner_slice.starts_with($expected_owner_prefix),
                        "{} owner_slice={} must start with {}",
                        $tool,
                        owner_slice,
                        $expected_owner_prefix
                    );
                }
                ToolOutcome::Ok { .. } => panic!("{} is still a stub", $tool),
            }
        }
    };
}

per_tool_dispatch_test!(dispatch_skill_publish, "skill_publish", "C2:");
per_tool_dispatch_test!(dispatch_skill_install, "skill_install", "C2:");
per_tool_dispatch_test!(dispatch_skill_evals_run, "skill_evals_run", "C2:");
per_tool_dispatch_test!(
    dispatch_studio_context_assemble,
    "studio_context_assemble",
    "C1:"
);
per_tool_dispatch_test!(dispatch_sensei_nudge, "sensei_nudge", "C1:");
per_tool_dispatch_test!(dispatch_share_intent_create, "share_intent_create", "C4:");
per_tool_dispatch_test!(dispatch_automation_schedule, "automation_schedule", "C5:");
per_tool_dispatch_test!(dispatch_inbox_ack, "inbox_ack", "C5:");
per_tool_dispatch_test!(dispatch_studio_run_skill, "studio_run_skill", "C1:");
