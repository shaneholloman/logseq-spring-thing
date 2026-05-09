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
            // Wiring assessment: SkillRegistrySupervisor (src/actors/skill_registry_supervisor.rs)
            // has RegisterPackage for in-memory registration, but skill_publish requires:
            //   1. Pod write to /private/skills/{name}/ (SKILL.md + skill.jsonld + skill.evals.jsonl)
            //      via solid-pod-rs (PodContributorPort production adapter not yet authored)
            //   2. Public Type Index registration (urn:solid:AgentSkill entry)
            //   3. NIP-98 content-hash signing
            //   4. SkillPublished domain event emission
            // OntologyGuidanceActor (src/actors/ontology_guidance_actor.rs) does not expose a
            // publish method — it is read-only (ComposeNudge). The ontology_agent_handler
            // (src/handlers/ontology_agent_handler.rs) has propose/validate endpoints but these
            // operate on ontology terms, not skill packages.
            Ok(not_implemented_stub(
                OwnerSlice::C2SkillRegistry,
                "skill_publish",
                "Requires: (1) PodContributorPort production adapter (src/adapters/) to write \
                 SKILL.md + skill.jsonld + skill.evals.jsonl atomically to \
                 /private/skills/{name}/ via solid-pod-rs; (2) TypeIndexWriter to register the \
                 skill in the contributor's publicTypeIndex.jsonld as urn:solid:AgentSkill; \
                 (3) NIP-98 signing service for content-hash attestation; (4) async dispatcher \
                 upgrade (current ToolDispatcher is sync but SkillRegistrySupervisor::RegisterPackage \
                 requires Actix actor send). ADR-029 Phase 3 tracks this work.",
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
            // Wiring assessment: SkillRegistrySupervisor has RegisterPackage (in-memory),
            // but skill_install requires:
            //   1. HTTP fetch of the skill_uri to retrieve the peer's skill.jsonld
            //   2. Pod write to /private/skills/{cloned-name}/ with version pin
            //   3. Compatibility scan via SkillCompatibilityScanner (actor exists,
            //      message ScanAllInstalled at src/actors/skill_compatibility_scanner.rs)
            //   4. Optional local calibration benchmark via SkillEvaluationActor
            //   5. SkillInstalled domain event emission
            // DojoDiscoveryActor (src/actors/dojo_discovery_actor.rs) handles peer crawling
            // but does not expose a single-skill fetch path.
            Ok(not_implemented_stub(
                OwnerSlice::C2SkillRegistry,
                "skill_install",
                "Requires: (1) PodClient-based skill_uri fetcher to retrieve peer skill.jsonld \
                 and SKILL.md from the target URI; (2) PodContributorPort write adapter to clone \
                 the skill into /private/skills/ with version pin; (3) SkillCompatibilityScanner \
                 integration (actor exists at src/actors/skill_compatibility_scanner.rs, message \
                 ScanAllInstalled); (4) optional local eval via SkillEvaluationActor \
                 (src/actors/skill_evaluation_actor.rs, message SubmitEvalRun); (5) async \
                 dispatcher upgrade. DojoDiscoveryActor handles periodic crawls but not \
                 single-skill fetch.",
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
            // Wiring assessment: SkillRegistrySupervisor (src/actors/skill_registry_supervisor.rs)
            // exposes RunSkillEval which forwards to SkillEvaluationActor (SubmitEvalRun).
            // The actor exists and the FSM is scaffolded (Idle -> Allocating -> Running ->
            // Grading -> Analysing -> Recording -> Idle). However:
            //   1. The ToolDispatcher type is sync; RunSkillEval requires async actor send
            //   2. The eval suite needs to be constructed from the tool payload (skill_id lookup
            //      in the SkillRegistrySupervisor's in-memory package map)
            //   3. The response needs a benchmark_uri (pod write path) which doesn't exist yet
            //
            // Pre-validation: extract and echo back the skill_id and mode so callers can
            // verify payload round-trips correctly while the async wiring is pending.
            let args = inv.arguments.as_object();
            let skill_id = args
                .and_then(|a| a.get("skill_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let mode = args
                .and_then(|a| a.get("mode"))
                .and_then(|v| v.as_str())
                .unwrap_or("baseline");
            let model_tier = args
                .and_then(|a| a.get("model_tier"))
                .and_then(|v| v.as_u64())
                .unwrap_or(2);

            log::info!(
                "[skill_evals_run] accepted payload: skill_id={}, mode={}, model_tier={}; \
                 backing actor exists (SkillEvaluationActor) but async dispatcher wiring pending",
                skill_id,
                mode,
                model_tier
            );

            Ok(not_implemented_stub(
                OwnerSlice::C2SkillRegistry,
                "skill_evals_run",
                &format!(
                    "Backing actor EXISTS: SkillEvaluationActor (src/actors/skill_evaluation_actor.rs) \
                     accepts SubmitEvalRun via SkillRegistrySupervisor::RunSkillEval. Payload \
                     validated (skill_id={skill_id}, mode={mode}, tier={model_tier}). Blocked on: \
                     (1) ToolDispatcher type must become async (Future<Output=Result<ToolOutcome, \
                     ToolDispatchError>>) so actor mailbox sends are possible; (2) SkillEvalSuite \
                     construction from the tool payload's suite_id (requires GetPackage lookup in \
                     SkillRegistrySupervisor); (3) benchmark_uri pod write path for completed results.",
                    skill_id = skill_id,
                    mode = mode,
                    model_tier = model_tier,
                ),
                inv,
            ))
        }),
    }
}
