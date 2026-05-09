//! BC18 Contributor Enablement MCP tool definitions.
//!
//! Owned by C1 (ContextAssemblyActor / ContributorStudioSupervisor), C3
//! (client Studio surface routing), and C4 (ShareOrchestratorActor).

use serde_json::json;
use std::sync::Arc;

use super::{not_implemented_stub, OwnerSlice, ToolDefinition, ToolDispatchError, ToolInvocation, ToolOutcome};

pub fn studio_context_assemble_definition() -> ToolDefinition {
    ToolDefinition {
        name: "studio_context_assemble",
        aliases: &[],
        description:
            "Return the Pod-assembled context for a contributor workspace: active artefact, \
             ontology neighbours, recent edits, collaborators. Cached; invalidated on Solid \
             Notification CREATE/UPDATE events for subscribed containers.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["workspace_id"],
            "properties": {
                "workspace_id": { "type": "string" },
                "depth": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 3,
                    "default": 2,
                    "description": "Ontology traversal depth for neighbour expansion."
                },
                "include_history": { "type": "boolean", "default": true }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["workspace_id", "active_artifact", "ontology_neighbours"],
            "properties": {
                "workspace_id": { "type": "string" },
                "active_artifact": {
                    "type": "object",
                    "properties": {
                        "uri": { "type": "string", "format": "uri" },
                        "kind": { "type": "string" }
                    }
                },
                "ontology_neighbours": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "iri": { "type": "string" },
                            "label": { "type": "string" },
                            "distance": { "type": "number" }
                        }
                    }
                },
                "recent_edits": { "type": "array", "items": { "type": "object" } },
                "collaborators": { "type": "array", "items": { "type": "object" } },
                "assembled_at": { "type": "string", "format": "date-time" }
            }
        }),
        owner_slice: OwnerSlice::C1ContributorStudio,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            // Wiring assessment: ContextAssemblyActor (src/actors/context_assembly_actor.rs)
            // handles AssembleContext{webid, workspace_id, episode_limit} and returns
            // WorkspaceFocus{project_ref, graph_selection, ontology_context, recent_episodes}.
            // The actor is supervised by ContributorStudioSupervisor and is live with stub
            // ports (StubPodContributorAdapter, StubGraphSelectionAdapter, etc.).
            //
            // Wiring path:
            //   1. ToolDispatcher must become async to send AssembleContext to the actor
            //   2. The tool payload's workspace_id maps directly to AssembleContext::workspace_id
            //   3. depth param needs OntologyNeighbourPort to support hop-depth (currently flat)
            //   4. include_history maps to episode_limit (true -> 20, false -> 0)
            //   5. Response must map WorkspaceFocus fields to the output schema
            //   6. WebID must come from the MCP session's authenticated identity (not in payload)
            //
            // Pre-validation: extract workspace_id for round-trip verification.
            let args = inv.arguments.as_object();
            let workspace_id = args
                .and_then(|a| a.get("workspace_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let depth = args
                .and_then(|a| a.get("depth"))
                .and_then(|v| v.as_u64())
                .unwrap_or(2);
            let include_history = args
                .and_then(|a| a.get("include_history"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            log::info!(
                "[studio_context_assemble] payload accepted: workspace_id={}, depth={}, \
                 include_history={}; backing actor exists (ContextAssemblyActor) but async wiring pending",
                workspace_id, depth, include_history
            );

            Ok(not_implemented_stub(
                OwnerSlice::C1ContributorStudio,
                "studio_context_assemble",
                &format!(
                    "Backing actor EXISTS: ContextAssemblyActor \
                     (src/actors/context_assembly_actor.rs) accepts AssembleContext \
                     {{webid, workspace_id, episode_limit}} and returns WorkspaceFocus. \
                     Supervised by ContributorStudioSupervisor; live with stub ports. Payload \
                     validated (workspace_id={workspace_id}, depth={depth}, \
                     include_history={include_history}). Blocked on: (1) ToolDispatcher must \
                     become async to send to actor mailbox; (2) WebID must be extracted from \
                     MCP session authentication context (not present in tool payload); \
                     (3) OntologyNeighbourPort needs hop-depth support for the depth parameter; \
                     (4) Production adapters must replace stubs (PodContributorPort, \
                     GraphSelectionPort, OntologyNeighbourPort, EpisodicMemoryPort).",
                    workspace_id = workspace_id,
                    depth = depth,
                    include_history = include_history,
                ),
                inv,
            ))
        }),
    }
}

pub fn sensei_nudge_definition() -> ToolDefinition {
    ToolDefinition {
        name: "sensei_nudge",
        aliases: &["studio_nudge"],
        description:
            "Return three recommended skills, three related pages and one ontology tension for \
             the current workspace focus. Tier-2 Haiku by default per ADR-026.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["workspace_id", "current_focus"],
            "properties": {
                "workspace_id": { "type": "string" },
                "current_focus": {
                    "type": "object",
                    "required": ["kind"],
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["artifact", "ontology_term", "skill", "free_text"]
                        },
                        "ref": { "type": "string" },
                        "text": { "type": "string" }
                    }
                },
                "max_suggestions": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 10,
                    "default": 3
                }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["skills", "pages", "tension"],
            "properties": {
                "skills": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "skill_uri": { "type": "string", "format": "uri" },
                            "name": { "type": "string" },
                            "rationale": { "type": "string" }
                        }
                    }
                },
                "pages": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "uri": { "type": "string", "format": "uri" },
                            "title": { "type": "string" }
                        }
                    }
                },
                "tension": {
                    "type": "object",
                    "properties": {
                        "summary": { "type": "string" },
                        "ontology_term": { "type": "string" }
                    }
                }
            }
        }),
        owner_slice: OwnerSlice::C1ContributorStudio,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            // Wiring assessment: OntologyGuidanceActor (src/actors/ontology_guidance_actor.rs)
            // handles ComposeNudge{session_id, focus: WorkspaceFocus} and returns
            // NudgeEnvelope{suggestions: [GuidanceSuggestion; 3], session_id, dismissable}.
            // The actor is live (supervised by ContributorStudioSupervisor) and produces
            // deterministic stub nudges with three suggestion kinds: CanonicalTerm,
            // PrecedentRef, SkillRef.
            //
            // Wiring path:
            //   1. ToolDispatcher must become async
            //   2. Tool payload current_focus must be converted to WorkspaceFocus:
            //      - current_focus.kind + current_focus.ref -> WorkspaceFocus::project_ref
            //      - current_focus.text -> seed for ontology_context
            //   3. workspace_id must first call studio_context_assemble (or reuse cached focus)
            //      to build the WorkspaceFocus that ComposeNudge expects
            //   4. Response must map NudgeEnvelope.suggestions to {skills, pages, tension} shape
            //   5. session_id should come from the MCP session or be generated per workspace
            //
            // Pre-validation: extract focus kind for round-trip verification.
            let args = inv.arguments.as_object();
            let workspace_id = args
                .and_then(|a| a.get("workspace_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let focus_kind = args
                .and_then(|a| a.get("current_focus"))
                .and_then(|f| f.as_object())
                .and_then(|f| f.get("kind"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            log::info!(
                "[sensei_nudge] payload accepted: workspace_id={}, focus_kind={}; \
                 backing actor exists (OntologyGuidanceActor) but async wiring pending",
                workspace_id, focus_kind
            );

            Ok(not_implemented_stub(
                OwnerSlice::C1ContributorStudio,
                "sensei_nudge",
                &format!(
                    "Backing actor EXISTS: OntologyGuidanceActor \
                     (src/actors/ontology_guidance_actor.rs) accepts ComposeNudge \
                     {{session_id, focus: WorkspaceFocus}} and returns NudgeEnvelope with \
                     exactly 3 GuidanceSuggestions (CanonicalTerm, PrecedentRef, SkillRef). \
                     Supervised by ContributorStudioSupervisor; live with stub composer. \
                     Payload validated (workspace_id={workspace_id}, focus_kind={focus_kind}). \
                     Blocked on: (1) ToolDispatcher must become async; (2) current_focus must \
                     be mapped to WorkspaceFocus (requires ContextAssemblyActor lookup or cached \
                     focus from studio_context_assemble); (3) NudgeEnvelope response must be \
                     reshaped to the output schema (skills/pages/tension triple); (4) real \
                     NudgeComposer must replace the stub (query ontology_discover / ontology_read / \
                     ontology_traverse MCP tools per ADR-057).",
                    workspace_id = workspace_id,
                    focus_kind = focus_kind,
                ),
                inv,
            ))
        }),
    }
}

pub fn share_intent_create_definition() -> ToolDefinition {
    ToolDefinition {
        name: "share_intent_create",
        aliases: &[],
        description:
            "Open a ShareIntent via ShareOrchestratorActor — drives the Private → Team → Mesh \
             transition, invokes BC17 policy evaluation, mutates ADR-052 WAC on success. \
             Mesh-target intents open a BrokerCase of category contributor_mesh_share.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["artifact_ref", "target_scope", "rationale"],
            "properties": {
                "artifact_ref": {
                    "type": "object",
                    "required": ["uri", "kind"],
                    "properties": {
                        "uri": { "type": "string", "format": "uri" },
                        "kind": {
                            "type": "string",
                            "enum": ["skill", "work_artifact", "ontology_term", "workflow", "graph_view"]
                        }
                    }
                },
                "target_scope": {
                    "type": "object",
                    "required": ["share_state"],
                    "properties": {
                        "share_state": {
                            "type": "string",
                            "enum": ["Private", "Team", "Mesh"],
                            "description": "Canonical BC18 ShareState."
                        },
                        "team_slug": {
                            "type": "string",
                            "description": "Required iff share_state=Team."
                        },
                        "distribution_hint": {
                            "type": "string",
                            "enum": ["personal", "team", "company", "public"],
                            "description": "Pod-layout refinement per Dojo UX."
                        }
                    }
                },
                "rationale": { "type": "string", "maxLength": 2000 }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["share_intent_id", "status"],
            "properties": {
                "share_intent_id": { "type": "string" },
                "status": {
                    "type": "string",
                    "enum": ["pending_policy", "approved", "mesh_candidate_broker_review", "rejected"]
                },
                "broker_case_id": {
                    "type": "string",
                    "description": "Populated iff target is Mesh."
                },
                "policy_trail": { "type": "array", "items": { "type": "object" } }
            }
        }),
        owner_slice: OwnerSlice::C4ShareOrchestrator,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            // Wiring assessment: ShareOrchestratorActor (src/actors/share_orchestrator_actor.rs)
            // handles RouteShareIntent{intent: ShareIntent, extras: ShareContextExtras} and
            // returns ShareOutcome (Rejected | TeamApproved | MeshCandidate | ...).
            // The ShareOrchestrator service (src/services/share_orchestrator.rs) implements
            // the full transition pipeline: classify -> policy evaluate -> WAC mutate ->
            // audit log. The domain ShareIntent aggregate
            // (src/domain/contributor/share_intent.rs) has ShareIntent::open() which validates
            // monotonic state transitions.
            //
            // Wiring path:
            //   1. ToolDispatcher must become async
            //   2. Tool payload must be mapped to domain types:
            //      - artifact_ref.uri + artifact_ref.kind -> ArtifactRef
            //      - target_scope.share_state -> ShareState enum
            //      - rationale -> String
            //   3. ShareIntent::open() must be called to create the domain aggregate
            //   4. ShareContextExtras must be populated (history, preferences, delegation_cap,
            //      mesh_eligible — most require session context not in the tool payload)
            //   5. RouteShareIntent sent to ShareOrchestratorActor
            //   6. ShareOutcome mapped to the output schema (share_intent_id, status, broker_case_id)
            let args = inv.arguments.as_object();
            let artifact_uri = args
                .and_then(|a| a.get("artifact_ref"))
                .and_then(|r| r.as_object())
                .and_then(|r| r.get("uri"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let target_share_state = args
                .and_then(|a| a.get("target_scope"))
                .and_then(|t| t.as_object())
                .and_then(|t| t.get("share_state"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            log::info!(
                "[share_intent_create] payload accepted: artifact_uri={}, target={}; \
                 backing actor exists (ShareOrchestratorActor) but async wiring pending",
                artifact_uri, target_share_state
            );

            Ok(not_implemented_stub(
                OwnerSlice::C4ShareOrchestrator,
                "share_intent_create",
                &format!(
                    "Backing actor EXISTS: ShareOrchestratorActor \
                     (src/actors/share_orchestrator_actor.rs) accepts RouteShareIntent \
                     {{intent: ShareIntent, extras: ShareContextExtras}} and returns \
                     ShareOutcome. Full pipeline implemented: classify transition -> policy \
                     evaluate -> WAC mutate -> audit log. Domain aggregate ShareIntent::open() \
                     validates monotonic state transitions. Payload validated \
                     (artifact_uri={artifact_uri}, target={target}). Blocked on: (1) \
                     ToolDispatcher must become async; (2) tool payload mapping to domain \
                     ShareIntent::open(ArtifactRef, from_state, to_state, rationale, session_id) \
                     — from_state must be fetched from the artifact's current share state in the \
                     pod; (3) ShareContextExtras population requires session context (history, \
                     delegation_cap_valid, mesh_eligible) not available in the tool payload; \
                     (4) BC17 policy engine must be configured with real evaluation rules.",
                    artifact_uri = artifact_uri,
                    target = target_share_state,
                ),
                inv,
            ))
        }),
    }
}

pub fn studio_run_skill_definition() -> ToolDefinition {
    ToolDefinition {
        name: "studio_run_skill",
        aliases: &[],
        description:
            "Execute an installed skill against the caller's workspace. Routes the call through \
             PartnerOrchestrationActor using the skill's declared min_model_tier.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["workspace_id", "skill_id"],
            "properties": {
                "workspace_id": { "type": "string" },
                "skill_id": { "type": "string" },
                "variables": {
                    "type": "object",
                    "description": "Variables bound into the skill's tool sequence."
                },
                "override_tier": {
                    "type": "integer",
                    "enum": [1, 2, 3],
                    "description": "Refuses to go below the skill's min_model_tier at dispatch."
                },
                "dry_run": { "type": "boolean", "default": false }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["run_id", "status"],
            "properties": {
                "run_id": { "type": "string" },
                "status": {
                    "type": "string",
                    "enum": ["queued", "running", "completed", "failed", "refused"]
                },
                "result": {
                    "type": "object",
                    "description": "Skill output document; present when status=completed."
                },
                "policy_trail": { "type": "array", "items": { "type": "object" } }
            }
        }),
        owner_slice: OwnerSlice::C1ContributorStudio,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            // Wiring assessment: studio_run_skill routes through PartnerOrchestrationActor
            // which does NOT exist yet (noted as "future, wired by C4/C5 sprints" in
            // ContributorStudioSupervisor). The execution flow requires:
            //   1. Look up skill_id in SkillRegistrySupervisor (GetPackage) to get the
            //      skill's declared tool_sequence and min_model_tier
            //   2. Validate override_tier >= min_model_tier
            //   3. Assemble workspace context via ContextAssemblyActor
            //   4. Bind variables into the tool sequence
            //   5. Execute the tool sequence via PartnerOrchestrationActor (uses ADR-026
            //      model routing for tier selection)
            //   6. Collect results and write to pod
            let args = inv.arguments.as_object();
            let workspace_id = args
                .and_then(|a| a.get("workspace_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let skill_id = args
                .and_then(|a| a.get("skill_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let dry_run = args
                .and_then(|a| a.get("dry_run"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            log::info!(
                "[studio_run_skill] payload accepted: workspace_id={}, skill_id={}, dry_run={}; \
                 PartnerOrchestrationActor does not exist yet",
                workspace_id, skill_id, dry_run
            );

            Ok(not_implemented_stub(
                OwnerSlice::C1ContributorStudio,
                "studio_run_skill",
                &format!(
                    "PartnerOrchestrationActor does NOT exist yet (planned for C4/C5 sprints, \
                     noted in ContributorStudioSupervisor). Payload validated \
                     (workspace_id={workspace_id}, skill_id={skill_id}, dry_run={dry_run}). \
                     Requires: (1) PartnerOrchestrationActor to execute tool sequences with \
                     ADR-026 model-tier routing; (2) SkillRegistrySupervisor::GetPackage to \
                     look up the skill's tool_sequence and min_model_tier; \
                     (3) ContextAssemblyActor integration to provide workspace context as \
                     execution input; (4) variable binding engine for the skill's tool sequence; \
                     (5) result collection and pod write; (6) async ToolDispatcher.",
                    workspace_id = workspace_id,
                    skill_id = skill_id,
                    dry_run = dry_run,
                ),
                inv,
            ))
        }),
    }
}
