//! BC18 Contributor Enablement MCP tool definitions.
//!
//! Owned by C1 (ContextAssemblyActor / ContributorStudioSupervisor), C3
//! (client Studio surface routing), and C4 (ShareOrchestratorActor).

use serde_json::json;
use std::sync::Arc;

use super::{not_implemented_stub, OwnerSlice, ToolDefinition, ToolInvocation, ToolOutcome};

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
            Ok(not_implemented_stub(
                OwnerSlice::C1ContributorStudio,
                "studio_context_assemble",
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
            Ok(not_implemented_stub(
                OwnerSlice::C1ContributorStudio,
                "sensei_nudge",
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
            Ok(not_implemented_stub(
                OwnerSlice::C4ShareOrchestrator,
                "share_intent_create",
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
            Ok(not_implemented_stub(
                OwnerSlice::C1ContributorStudio,
                "studio_run_skill",
                inv,
            ))
        }),
    }
}
