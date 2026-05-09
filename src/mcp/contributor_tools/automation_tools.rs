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
        description: "Install a cron definition to /private/automations/ and register it with \
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
            // Wiring assessment: AutomationOrchestratorActor does NOT exist yet (C5 sprint).
            // BriefingService (src/services/briefing_service.rs) and briefing_handler
            // (src/handlers/briefing_handler.rs) implement submit_brief flow, but this is
            // a one-shot brief submission — not a cron/interval scheduler.
            //
            // The automation_schedule tool needs:
            //   1. AutomationOrchestratorActor (new actor) to own the cron/interval scheduler
            //   2. Pod write to /private/automations/{name}.jsonld with the routine_spec
            //   3. NIP-26 delegated key issuance for inbox delivery
            //   4. cron parser (e.g. cron crate) to validate schedule expressions
            //   5. Integration with studio_run_skill to execute the target_skill on trigger
            //   6. Idempotency key management per ADR-057 Open Question 2
            //
            // BriefingService is NOT suitable as the backing service — it submits one-shot
            // briefs to the Management API, not scheduled automations.
            let args = inv.arguments.as_object();
            let routine_name = args
                .and_then(|a| a.get("routine_spec"))
                .and_then(|r| r.as_object())
                .and_then(|r| r.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let target_skill = args
                .and_then(|a| a.get("routine_spec"))
                .and_then(|r| r.as_object())
                .and_then(|r| r.get("target_skill"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let has_cron = args
                .and_then(|a| a.get("routine_spec"))
                .and_then(|r| r.as_object())
                .and_then(|r| r.get("schedule"))
                .and_then(|s| s.as_object())
                .map(|s| s.contains_key("cron"))
                .unwrap_or(false);

            log::info!(
                "[automation_schedule] payload accepted: routine={}, target_skill={}, \
                 schedule_type={}; AutomationOrchestratorActor does not exist yet",
                routine_name,
                target_skill,
                if has_cron { "cron" } else { "interval" }
            );

            Ok(not_implemented_stub(
                OwnerSlice::C5Automation,
                "automation_schedule",
                &format!(
                    "AutomationOrchestratorActor does NOT exist yet (C5 sprint). Payload \
                     validated (routine={routine_name}, target_skill={target_skill}, \
                     schedule_type={sched_type}). BriefingService \
                     (src/services/briefing_service.rs) handles one-shot brief submissions, \
                     NOT scheduled automations. Requires: (1) AutomationOrchestratorActor \
                     (new) with cron/interval scheduler; (2) Pod write adapter for \
                     /private/automations/{{name}}.jsonld; (3) NIP-26 delegated key service \
                     for inbox delivery; (4) cron expression parser and validation; \
                     (5) studio_run_skill integration for trigger execution; \
                     (6) idempotency key strategy implementation per ADR-057 OQ-2; \
                     (7) async ToolDispatcher.",
                    routine_name = routine_name,
                    target_skill = target_skill,
                    sched_type = if has_cron { "cron" } else { "interval" },
                ),
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
            // Wiring assessment: No inbox service or actor exists yet (C5 sprint).
            // The inbox_ack flow requires:
            //   1. Inbox read service to locate the brief by brief_id in /inbox/
            //   2. Disposition routing: accept/defer/dismiss are local pod writes
            //      (move to /inbox/.archive/ or set defer_until); escalate_to_broker
            //      opens a contributor_mesh_share BrokerCase on BC11
            //   3. BrokerCaseFactory (src/domain/broker/) exists for case creation
            //      but the inbox -> broker bridge does not
            //   4. Pod write adapter for /inbox/.archive/ moves
            //   5. Notification emission for disposition acknowledgement
            let args = inv.arguments.as_object();
            let brief_id = args
                .and_then(|a| a.get("brief_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let disposition = args
                .and_then(|a| a.get("disposition"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let has_defer_until = args
                .and_then(|a| a.get("defer_until"))
                .is_some();

            // Validate disposition-specific constraints
            if disposition == "defer" && !has_defer_until {
                log::warn!(
                    "[inbox_ack] disposition=defer but defer_until not provided for brief_id={}",
                    brief_id
                );
            }

            log::info!(
                "[inbox_ack] payload accepted: brief_id={}, disposition={}; \
                 no inbox service exists yet",
                brief_id, disposition
            );

            Ok(not_implemented_stub(
                OwnerSlice::C5Automation,
                "inbox_ack",
                &format!(
                    "No inbox service or actor exists yet (C5 sprint). Payload validated \
                     (brief_id={brief_id}, disposition={disposition}). Requires: (1) Inbox \
                     read service to locate briefs in /inbox/ pod container; (2) Pod write \
                     adapter for disposition mutations (accept -> archive, defer -> set \
                     defer_until, dismiss -> archive with reason); (3) for \
                     escalate_to_broker: BrokerCase creation via BrokerCaseFactory \
                     (src/domain/broker/ — domain exists, bridge to inbox does not); \
                     (4) Solid Notification emission on disposition change; \
                     (5) async ToolDispatcher.",
                    brief_id = brief_id,
                    disposition = disposition,
                ),
                inv,
            ))
        }),
    }
}
