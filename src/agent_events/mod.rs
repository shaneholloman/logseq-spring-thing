//! Canonical agent-event envelope (Phase 1 of ADR-059).
//!
//! Carries the existing numeric IDs alongside optional `source_urn`,
//! `target_urn`, and `pubkey` per agentbox ADR-013 grammar. Backward-
//! compatible: legacy clients that don't populate the new fields keep
//! working unchanged.
//!
//! See ADR-059 §2 for the schema and ADR-014 for the agentbox side.

use serde::{Deserialize, Serialize};

pub mod transient;

/// Inbound `agent_action` event from agentbox via `/wss/agent-events`.
///
/// `source_urn`, `target_urn`, and `pubkey` are optional in Phases 1-3,
/// required in Phases 4-5. The legacy fields (`source_agent_id`,
/// `target_node_id`, `action_type`, `duration_ms`) match the existing
/// agentbox `AgentEvent` shape (`agent-event-publisher.js:44-54`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActionEnvelope {
    /// Wire envelope version. Currently `3`.
    pub version: u8,
    /// Discriminator. Currently `"agent_action"`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Sequential event ID assigned by agentbox.
    pub id: u64,
    /// Milliseconds since epoch.
    pub timestamp: i64,
    /// Legacy numeric source agent ID (hash of agent name).
    pub source_agent_id: u32,
    /// Optional canonical URN of the source agent (e.g. `did:nostr:<hex>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_urn: Option<String>,
    /// Legacy numeric target node ID.
    pub target_node_id: u32,
    /// Optional canonical URN of the target node (e.g. `urn:visionclaw:kg:...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_urn: Option<String>,
    /// Action type per `AgentActionType` (0..5).
    pub action_type: u8,
    /// Animation/spring duration in milliseconds.
    pub duration_ms: u16,
    /// Optional did:nostr hex pubkey for identity attribution.
    /// Phases 1-3: optional. Phase 4: required for owned-KGNode mutations.
    /// Phase 5: signed + NIP-26 delegation chain validated (deferred to ADR-061).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,
    /// Free-form metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl AgentActionEnvelope {
    /// Returns the action colour for the beam visual (ADR-059 §4).
    /// Unknown action types default to grey per recommendation #2 in ADR-059.
    pub fn beam_color(&self) -> &'static str {
        match self.action_type {
            0 => "#3b82f6", // QUERY blue
            1 => "#facc15", // UPDATE yellow
            2 => "#22c55e", // CREATE green
            3 => "#ef4444", // DELETE red
            4 => "#a855f7", // LINK purple
            5 => "#06b6d4", // TRANSFORM cyan
            _ => "#9ca3af", // UNKNOWN grey
        }
    }

    /// Whether the envelope provides identity attribution (Phase 1+).
    pub fn has_identity(&self) -> bool {
        self.pubkey.is_some() || self.source_urn.is_some()
    }
}

/// Outbound `user_interaction` event from VisionClaw → agentbox (ADR-059 §3).
///
/// Transient. Not persisted to Neo4j. Tied to the live UI session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInteractionEvent {
    /// Wire envelope version. Currently `1`.
    pub version: u8,
    /// Discriminator. Always `"user_interaction"`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Interaction kind.
    pub kind: UserInteractionKind,
    /// Per-session UUID for correlation.
    pub session_id: String,
    /// Optional did:nostr hex pubkey of the user (when authenticated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_pubkey: Option<String>,
    /// Numeric target node ID.
    pub target_node_id: u32,
    /// Optional canonical URN of the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_urn: Option<String>,
    /// Expected interaction lifetime in milliseconds (hover dwell, drag duration, etc.).
    pub duration_ms: u32,
    /// Milliseconds since epoch.
    pub timestamp: i64,
}

impl UserInteractionEvent {
    pub fn new(
        kind: UserInteractionKind,
        session_id: impl Into<String>,
        target_node_id: u32,
        duration_ms: u32,
    ) -> Self {
        Self {
            version: 1,
            event_type: "user_interaction".to_string(),
            kind,
            session_id: session_id.into(),
            session_pubkey: None,
            target_node_id,
            target_urn: None,
            duration_ms,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserInteractionKind {
    /// Node entered camera centre band with >1.5 s dwell.
    Focus,
    /// Click / tap on node.
    Select,
    /// Raycast hover ≥ 250 ms.
    Hover,
    /// Interactive grab in progress.
    Drag,
}

/// WebSocket subprotocol token negotiated on `/wss/agent-events`.
pub const WS_SUBPROTOCOL: &str = "vc-agent-events.v1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_minimal_envelope() {
        let json = r#"{
            "version": 3,
            "type": "agent_action",
            "id": 42,
            "timestamp": 1714312345678,
            "source_agent_id": 7,
            "target_node_id": 4242,
            "action_type": 1,
            "duration_ms": 250
        }"#;
        let env: AgentActionEnvelope = serde_json::from_str(json).expect("parse");
        assert_eq!(env.version, 3);
        assert_eq!(env.action_type, 1);
        assert!(env.source_urn.is_none());
        assert!(env.pubkey.is_none());
        assert!(!env.has_identity());
        assert_eq!(env.beam_color(), "#facc15");
    }

    #[test]
    fn round_trip_full_envelope_with_urns() {
        let json = r#"{
            "version": 3,
            "type": "agent_action",
            "id": 42,
            "timestamp": 1714312345678,
            "source_agent_id": 7,
            "source_urn": "did:nostr:abc123",
            "target_node_id": 4242,
            "target_urn": "urn:visionclaw:kg:79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798:sha256-12-deadbeef0001",
            "action_type": 4,
            "duration_ms": 250,
            "pubkey": "abc123",
            "metadata": {"intent": "test"}
        }"#;
        let env: AgentActionEnvelope = serde_json::from_str(json).expect("parse");
        assert_eq!(env.source_urn.as_deref(), Some("did:nostr:abc123"));
        assert_eq!(env.target_urn.as_deref(), Some("urn:visionclaw:kg:79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798:sha256-12-deadbeef0001"));
        assert_eq!(env.pubkey.as_deref(), Some("abc123"));
        assert!(env.has_identity());
        assert_eq!(env.beam_color(), "#a855f7");
    }

    #[test]
    fn unknown_action_type_defaults_grey() {
        let env = AgentActionEnvelope {
            version: 3,
            event_type: "agent_action".to_string(),
            id: 1,
            timestamp: 0,
            source_agent_id: 1,
            source_urn: None,
            target_node_id: 1,
            target_urn: None,
            action_type: 99,
            duration_ms: 100,
            pubkey: None,
            metadata: serde_json::Value::Null,
        };
        assert_eq!(env.beam_color(), "#9ca3af");
    }

    #[test]
    fn user_interaction_round_trip() {
        let evt = UserInteractionEvent::new(
            UserInteractionKind::Focus,
            "session-uuid-1",
            4242,
            1500,
        );
        let json = serde_json::to_string(&evt).unwrap();
        let back: UserInteractionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, UserInteractionKind::Focus);
        assert_eq!(back.target_node_id, 4242);
        assert_eq!(back.duration_ms, 1500);
    }
}
