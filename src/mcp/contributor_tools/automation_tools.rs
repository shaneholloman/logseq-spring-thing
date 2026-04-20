//! Automation + inbox MCP tool definitions (C5).
//!
//! Delegated-cap Pod writes to /private/automations/ and /inbox/ per
//! ADR-057 §"Pod Layout Extensions" and design 03 §2.

use serde_json::json;
use std::sync::Arc;

use super::{not_implemented_stub, OwnerSlice, ToolDefinition, ToolInvocation, ToolOutcome};

pub fn automation_schedule_definition() -> ToolDefinition {
    ToolDefinition {
        name: "automation_schedule",
        aliases: &[],
        description:
            "Install a cron definition to /private/automations/ and register it with \
             AutomationOrchestratorActor. Delivery targets /inbox/ via a NIP-26 delegated key.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["routine_spec"],
            "properties": {
                "routine_spec": {
                    "type": "object",
                    "required": ["name", "schedule", "target_skill"],
                    "properties": {
                        "name": {
                            "type": "string",
                            "pattern": "^[a-z0-9][a-z0-9-]{1,62}[a-z0-9]$"
                        },
                        "description": { "type": "string", "maxLength": 600 },
                        "schedule": {
                            "type": "object",
                            "oneOf": [
                                {
                                    "required": ["cron"],
                                    "properties": {
                                        "cron": { "type": "string" }
                                    }
                                },
                                {
                                    "required": ["interval_seconds"],
                                    "properties": {
                                        "interval_seconds": {
                                            "type": "integer",
                                            "minimum": 60
                                        }
                                    }
                                }
                            ]
                        },
                        "target_skill": {
                            "type": "string",
                            "description": "skill_id or skill_uri of the routine entry point."
                        },
                        "variables": { "type": "object" },
                        "delivery": {
                            "type": "string",
                            "enum": ["inbox", "notification_only", "email"],
                            "default": "inbox"
                        },
                        "idempotency_key_strategy": {
                            "type": "string",
                            "enum": ["at_most_once", "at_least_once", "exactly_once"],
                            "default": "at_least_once",
                            "description": "Per ADR-057 Open Question 2."
                        }
                    }
                }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["automation_id", "pod_uri"],
            "properties": {
                "automation_id": { "type": "string" },
                "pod_uri": {
                    "type": "string",
                    "format": "uri",
                    "description": "Canonical URI under /private/automations/."
                },
                "next_fire_at": { "type": "string", "format": "date-time" }
            }
        }),
        owner_slice: OwnerSlice::C5Automation,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            Ok(not_implemented_stub(
                OwnerSlice::C5Automation,
                "automation_schedule",
                inv,
            ))
        }),
    }
}

pub fn inbox_ack_definition() -> ToolDefinition {
    ToolDefinition {
        name: "inbox_ack",
        aliases: &[],
        description:
            "Mark a contributor inbox brief as reviewed. Disposition drives onward routing: \
             accept/defer/dismiss are local-only; escalate_to_broker opens a contributor_mesh_share \
             case on BC11.",
        input_schema: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["brief_id", "disposition"],
            "properties": {
                "brief_id": { "type": "string" },
                "disposition": {
                    "type": "string",
                    "enum": ["accept", "defer", "dismiss", "escalate_to_broker"]
                },
                "reason": { "type": "string", "maxLength": 2000 },
                "defer_until": {
                    "type": "string",
                    "format": "date-time",
                    "description": "Required iff disposition=defer."
                }
            }
        }),
        output_schema: json!({
            "type": "object",
            "required": ["brief_id", "disposition", "acknowledged_at"],
            "properties": {
                "brief_id": { "type": "string" },
                "disposition": { "type": "string" },
                "acknowledged_at": { "type": "string", "format": "date-time" },
                "broker_case_id": {
                    "type": "string",
                    "description": "Populated iff disposition=escalate_to_broker."
                },
                "archive_uri": {
                    "type": "string",
                    "format": "uri",
                    "description": "Populated when the brief is moved to /inbox/.archive/."
                }
            }
        }),
        owner_slice: OwnerSlice::C5Automation,
        dispatcher: Arc::new(|inv: &ToolInvocation| {
            Ok(not_implemented_stub(
                OwnerSlice::C5Automation,
                "inbox_ack",
                inv,
            ))
        }),
    }
}
