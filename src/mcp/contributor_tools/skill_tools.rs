//! BC19 Skill Lifecycle MCP tool definitions.
//!
//! Canonical schemas from `02-skill-dojo-and-evals.md` §5.1 (`skill_publish`)
//! and §7 (`skill_install`), plus the `skill_evals_run` dispatcher from §8.
//!
//! Handlers are stubs that delegate to `SkillRegistrySupervisor` (C2).

use serde_json::json;
use std::sync::Arc;

use super::{not_implemented_stub, OwnerSlice, ToolDefinition, ToolInvocation, ToolOutcome};

pub fn skill_publish_definition() -> ToolDefinition {
    ToolDefinition {
        name: "skill_publish",
        aliases: &["skill_registry_publish"],
        description:
            "Publish a new or updated skill to the contributor's Pod and register it in the \
             public Type Index. Writes SKILL.md + skill.jsonld + skill.evals.jsonl atomically, \
             NIP-98 signs the content hash, and emits SkillPublished.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["name", "description", "tool_sequence", "eval_suite", "category"],
            "properties": {
                "name": {
                    "type": "string",
                    "pattern": "^[a-z0-9][a-z0-9-]{1,62}[a-z0-9]$",
                    "description": "Stable slug; must match SKILL.md frontmatter name."
                },
                "version": {
                    "type": "string",
                    "pattern": "^\\d+\\.\\d+\\.\\d+(-[A-Za-z0-9.-]+)?$",
                    "description": "Semver. Must be strictly greater than any currently-published version."
                },
                "description": { "type": "string", "maxLength": 600 },
                "category": {
                    "type": "string",
                    "enum": [
                        "research.finance", "research.general", "authoring.docs",
                        "authoring.slides", "ops.ci", "ops.data", "ops.security",
                        "analysis.ontology", "preference.style", "capability.extraction",
                        "capability.media"
                    ]
                },
                "tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Ordered MCP tool names the skill invokes."
                },
                "tool_sequence": { "type": "array", "items": { "type": "object" } },
                "min_model_tier": { "type": "integer", "enum": [1, 2, 3] },
                "prerequisites": { "type": "array", "items": { "type": "string" } },
                "license": { "type": "string" },
                "eval_suite": {
                    "type": "array",
                    "items": { "type": "object" },
                    "minItems": 3,
                    "description": "Eval cases per §8.1 — prompts + assertions."
                },
                "distribution": {
                    "type": "string",
                    "enum": ["personal", "team", "company", "public"],
                    "default": "personal",
                    "description": "Pod-layout refinement of the BC18 ShareState \
                                   (personal→Private, team/company→Team, public→Mesh)."
                },
                "team_slug": {
                    "type": "string",
                    "description": "Required iff distribution=team."
                },
                "target_scope": {
                    "type": "string",
                    "description": "ADR-057 alias for distribution; accepted for back-compat."
                }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["skill_uri", "skill_md_uri"],
            "properties": {
                "skill_uri": { "type": "string", "format": "uri" },
                "skill_md_uri": { "type": "string", "format": "uri" },
                "type_index_entry": { "type": "string", "format": "uri" },
                "signature": {
                    "type": "object",
                    "properties": {
                        "algorithm": { "const": "nip98-ed25519" },
                        "content_hash": { "type": "string" },
                        "signed_by": { "type": "string" },
                        "signed_at": { "type": "string", "format": "date-time" }
                    }
                },
                "baseline_benchmark": { "type": "string", "format": "uri" }
            }
        }),
        owner_slice: OwnerSlice::C2SkillRegistry,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            Ok(not_implemented_stub(
                OwnerSlice::C2SkillRegistry,
                "skill_publish",
                inv,
            ))
        }),
    }
}

pub fn skill_install_definition() -> ToolDefinition {
    ToolDefinition {
        name: "skill_install",
        aliases: &[],
        description:
            "Clone a peer skill into /private/skills/ with version pin, run the compatibility \
             scan and (optionally) a local calibration benchmark. Emits SkillInstalled.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["skill_uri"],
            "properties": {
                "skill_uri": { "type": "string", "format": "uri" },
                "version": {
                    "type": "string",
                    "description": "Alias for pin_version (ADR-057 original spelling)."
                },
                "pin_version": {
                    "type": "string",
                    "description": "Defaults to the version at the target URI."
                },
                "run_local_evals": { "type": "boolean", "default": true }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["install_uri", "pinned_version"],
            "properties": {
                "install_uri": { "type": "string", "format": "uri" },
                "pinned_version": { "type": "string" },
                "local_eval_benchmark_uri": { "type": "string", "format": "uri" }
            }
        }),
        owner_slice: OwnerSlice::C2SkillRegistry,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            Ok(not_implemented_stub(
                OwnerSlice::C2SkillRegistry,
                "skill_install",
                inv,
            ))
        }),
    }
}

pub fn skill_evals_run_definition() -> ToolDefinition {
    // `skill_evals_run` is preferred per design 02 §8; ADR-057 table uses the
    // singular `skill_eval_run`. Both are registered; the canonical name is
    // plural, the singular is an alias.
    ToolDefinition {
        name: "skill_evals_run",
        aliases: &["skill_eval_run"],
        description:
            "Dispatch SkillEvaluationActor to run an eval suite against a skill version. Returns \
             a benchmark handle; SkillBenchmarkCompleted is emitted on the bus when ready.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["skill_id"],
            "properties": {
                "skill_id": { "type": "string" },
                "suite_id": {
                    "type": "string",
                    "description": "Optional — defaults to the latest suite on the skill."
                },
                "mode": {
                    "type": "string",
                    "enum": ["baseline", "team-gate", "mesh-gate", "drift", "calibration"],
                    "default": "baseline"
                },
                "model_tier": { "type": "integer", "enum": [1, 2, 3] }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["benchmark_id", "status"],
            "properties": {
                "benchmark_id": { "type": "string" },
                "status": {
                    "type": "string",
                    "enum": ["queued", "running", "grading", "completed", "failed"]
                },
                "benchmark_uri": { "type": "string", "format": "uri" }
            }
        }),
        owner_slice: OwnerSlice::C2SkillRegistry,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            Ok(not_implemented_stub(
                OwnerSlice::C2SkillRegistry,
                "skill_evals_run",
                inv,
            ))
        }),
    }
}
