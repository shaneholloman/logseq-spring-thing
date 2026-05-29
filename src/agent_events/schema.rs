//! Canonical agent-action wire-envelope schema (ADR-059 §2 mirror).
//!
//! agentbox `management-api/utils/agent-event-publisher.js` is the **canonical
//! schema source** (ADR-059 Consequences). This module mirrors that one shape
//! byte-for-field so the `/wss/agent-events` ingest (ADR-059 Phase 2)
//! deserialises exactly what agentbox emits — including the ADR-013 identity
//! attribution (`source_urn` / `target_urn` / `pubkey`) that the deprecated
//! MCP-TCP bridge used to drop at the federation boundary.
//!
//! Phasing (ADR-059 §5):
//!   * Phase 1 (this module): schema + cross-repo fixture tests. No transport.
//!   * Phase 2: the `/wss/agent-events` handler consumes `AgentActionNotification`
//!     and projects it onto the identity-blind binary `0x23` frame
//!     (`crate::utils::binary_protocol`) after resolving `source_urn` /
//!     `target_urn` → numeric ids. Identity is carried in this JSON ingest
//!     envelope; the GPU binary frame stays numeric-only by design.
//!   * Phase 5: `source_urn` / `pubkey` become mandatory (fail-closed NIP-26).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::utils::binary_protocol::{AgentActionEvent, AgentActionType};

/// Top-level JSON-RPC 2.0 notification: `notifications/agent_action`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentActionNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: AgentActionParams,
}

impl AgentActionNotification {
    pub const METHOD: &'static str = "notifications/agent_action";

    /// True when this is a well-formed canonical agent-action notification.
    pub fn is_canonical(&self) -> bool {
        self.jsonrpc == "2.0"
            && self.method == Self::METHOD
            && self.params.kind == "agent_action"
            && self.params.event.version >= 3
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentActionParams {
    #[serde(rename = "type")]
    pub kind: String,
    pub event: AgentActionEnvelope,
    /// Binary-frame parity (ADR-059 §1): AGENT_ACTION = `0x23`.
    pub message_type: u8,
    /// Binary protocol version (V2).
    pub protocol_version: u8,
    /// ISO-8601 wall-clock emit time (distinct from the event's epoch-ms field).
    pub timestamp: String,
}

/// The additive ADR-059 §2 event. Legacy numeric ids are retained for binary
/// projection; the URN/pubkey fields are optional in Phase 1 and become
/// mandatory under fail-closed attribution in Phase 5.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentActionEnvelope {
    pub version: u8,
    pub id: u64,
    pub source_agent_id: u32,
    pub target_node_id: u32,
    /// Mirror of `binary_protocol::AgentActionType` (0..=5).
    pub action_type: u8,
    pub action_type_name: String,
    /// Epoch milliseconds (full width; the binary frame truncates to u32).
    pub timestamp: u64,
    pub duration_ms: u32,

    /// ADR-013 identity attribution. `None` (serialised `null`) until Phase 5.
    #[serde(default)]
    pub source_urn: Option<String>,
    #[serde(default)]
    pub target_urn: Option<String>,
    #[serde(default)]
    pub pubkey: Option<String>,

    #[serde(default)]
    pub metadata: Value,
}

impl AgentActionEnvelope {
    pub fn action_type(&self) -> AgentActionType {
        AgentActionType::from(self.action_type)
    }

    /// Project the identity-bearing JSON envelope onto the identity-blind binary
    /// `0x23` frame the GPU consumes. Numeric ids pass through; identity is
    /// dropped here *on purpose* — it has already been resolved/persisted
    /// server-side (ADR-059 §2). The JSON `metadata` rides as the binary payload.
    pub fn to_binary_event(&self) -> AgentActionEvent {
        AgentActionEvent {
            source_agent_id: self.source_agent_id,
            target_node_id: self.target_node_id,
            action_type: self.action_type,
            timestamp: (self.timestamp % u32::MAX as u64) as u32,
            duration_ms: self.duration_ms.min(u16::MAX as u32) as u16,
            payload: serde_json::to_vec(&self.metadata).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exact agentbox `createMcpNotification` output (the cross-repo contract
    // fixture). Mirrors tests/sovereign/agent-event-notification.test.js.
    fn canonical_json_with_identity() -> &'static str {
        r#"{
          "jsonrpc": "2.0",
          "method": "notifications/agent_action",
          "params": {
            "type": "agent_action",
            "event": {
              "version": 3,
              "id": 7,
              "source_agent_id": 7,
              "target_node_id": 4242,
              "action_type": 1,
              "action_type_name": "update",
              "timestamp": 1748500000000,
              "duration_ms": 250,
              "source_urn": "did:nostr:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              "target_urn": "urn:visionclaw:kg:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb:sha256-12-deadbeef0011",
              "pubkey": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              "metadata": { "note": "x" }
            },
            "message_type": 35,
            "protocol_version": 2,
            "timestamp": "2026-05-29T00:00:00.000Z"
          }
        }"#
    }

    #[test]
    fn deserialises_identity_attribution_end_to_end() {
        let n: AgentActionNotification =
            serde_json::from_str(canonical_json_with_identity()).expect("parse");
        assert!(n.is_canonical());
        assert_eq!(n.method, AgentActionNotification::METHOD);
        assert_eq!(n.params.message_type, 0x23);
        assert_eq!(n.params.protocol_version, 2);

        let e = &n.params.event;
        assert_eq!(e.version, 3);
        assert_eq!(e.id, 7);
        assert_eq!(e.action_type, 1);
        assert_eq!(e.action_type(), AgentActionType::Update);
        assert_eq!(e.action_type_name, "update");
        assert_eq!(
            e.source_urn.as_deref(),
            Some("did:nostr:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(e.target_urn.as_deref().unwrap().starts_with("urn:visionclaw:kg:"));
        assert_eq!(e.pubkey.as_deref().unwrap().len(), 64);
    }

    #[test]
    fn identity_absent_deserialises_as_none_not_error() {
        // Phase 1 backward-compatibility: a producer that omits the identity
        // fields entirely must still parse (null vs missing both → None).
        let json = r#"{
          "jsonrpc": "2.0",
          "method": "notifications/agent_action",
          "params": {
            "type": "agent_action",
            "event": {
              "version": 3, "id": 1, "source_agent_id": 1, "target_node_id": 2,
              "action_type": 0, "action_type_name": "query",
              "timestamp": 1748500000000, "duration_ms": 100
            },
            "message_type": 35, "protocol_version": 2,
            "timestamp": "2026-05-29T00:00:00.000Z"
          }
        }"#;
        let n: AgentActionNotification = serde_json::from_str(json).expect("parse");
        assert!(n.params.event.source_urn.is_none());
        assert!(n.params.event.target_urn.is_none());
        assert!(n.params.event.pubkey.is_none());
        assert!(n.params.event.metadata.is_null());
    }

    #[test]
    fn round_trips_through_serde() {
        let n: AgentActionNotification =
            serde_json::from_str(canonical_json_with_identity()).expect("parse");
        let s = serde_json::to_string(&n).expect("serialise");
        let n2: AgentActionNotification = serde_json::from_str(&s).expect("reparse");
        assert_eq!(n, n2);
    }

    #[test]
    fn projects_onto_identity_blind_binary_frame() {
        let n: AgentActionNotification =
            serde_json::from_str(canonical_json_with_identity()).expect("parse");
        let bin = n.params.event.to_binary_event();
        assert_eq!(bin.source_agent_id, 7);
        assert_eq!(bin.target_node_id, 4242);
        assert_eq!(bin.get_action_type(), AgentActionType::Update);
        assert_eq!(bin.duration_ms, 250);
        // Full-width epoch ms is truncated into the u32 binary timestamp field.
        assert_eq!(bin.timestamp, (1748500000000_u64 % u32::MAX as u64) as u32);
        // Identity does not appear on the binary wire — only metadata rides.
        assert!(!bin.payload.is_empty());
    }
}
