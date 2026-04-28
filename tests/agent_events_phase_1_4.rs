//! Integration tests for ADR-059 Phases 1-4 (agent events + transient edges).
//! Lives in `tests/` so it compiles as a standalone test crate, isolated from
//! the lib test crate (which currently has pre-existing E0063/E0369 errors
//! in unrelated services).

use webxr::agent_events::{
    AgentActionEnvelope, UserInteractionEvent, UserInteractionKind, WS_SUBPROTOCOL,
};
use webxr::agent_events::transient::{BeamEdge, ChargeModulation};

#[test]
fn phase_1_envelope_round_trip_minimal() {
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
fn phase_1_envelope_round_trip_full() {
    let json = r#"{
        "version": 3,
        "type": "agent_action",
        "id": 42,
        "timestamp": 1714312345678,
        "source_agent_id": 7,
        "source_urn": "did:nostr:abc123",
        "target_node_id": 4242,
        "target_urn": "urn:visionclaw:kg:npub1xyz:sha256-12-deadbeef",
        "action_type": 4,
        "duration_ms": 250,
        "pubkey": "abc123",
        "metadata": {"intent": "test"}
    }"#;
    let env: AgentActionEnvelope = serde_json::from_str(json).expect("parse");
    assert_eq!(env.source_urn.as_deref(), Some("did:nostr:abc123"));
    assert_eq!(env.target_urn.as_deref(), Some("urn:visionclaw:kg:npub1xyz:sha256-12-deadbeef"));
    assert_eq!(env.pubkey.as_deref(), Some("abc123"));
    assert!(env.has_identity());
    assert_eq!(env.beam_color(), "#a855f7");
}

#[test]
fn phase_1_unknown_action_type_defaults_grey() {
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
fn phase_2_ws_subprotocol_token_is_canonical() {
    assert_eq!(WS_SUBPROTOCOL, "vc-agent-events.v1");
}

#[test]
fn phase_2_beam_edge_construction_and_expiry() {
    let mut beam = BeamEdge::new(1, 2, 0, "#3b82f6".into(), 100);
    assert_eq!(beam.source_agent_id, 1);
    assert_eq!(beam.target_node_id, 2);
    assert_eq!(beam.action_type, 0);
    assert!(!beam.is_expired(beam.spawned_at_ms));
    // Pretend we spawned in the past.
    beam.spawned_at_ms -= 200;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!(beam.is_expired(now));
}

#[test]
fn phase_2_charge_modulation_default_multiplier() {
    let m = ChargeModulation::new(1, 2, 1000);
    assert_eq!(m.agent_node_id, 1);
    assert_eq!(m.target_node_id, 2);
    assert_eq!(m.multiplier, 1.5);
    assert!(!m.is_expired());
}

#[test]
fn phase_3_user_interaction_round_trip() {
    let evt = UserInteractionEvent::new(
        UserInteractionKind::Focus,
        "session-uuid-1",
        4242,
        1500,
    );
    assert_eq!(evt.version, 1);
    assert_eq!(evt.event_type, "user_interaction");
    assert_eq!(evt.kind, UserInteractionKind::Focus);
    let json = serde_json::to_string(&evt).unwrap();
    let back: UserInteractionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back.kind, UserInteractionKind::Focus);
    assert_eq!(back.target_node_id, 4242);
    assert_eq!(back.duration_ms, 1500);
    assert_eq!(back.session_id, "session-uuid-1");
    assert!(back.session_pubkey.is_none());
}

#[test]
fn phase_3_user_interaction_kind_lowercase_serialization() {
    for (kind, expected) in [
        (UserInteractionKind::Focus, "\"focus\""),
        (UserInteractionKind::Select, "\"select\""),
        (UserInteractionKind::Hover, "\"hover\""),
        (UserInteractionKind::Drag, "\"drag\""),
    ] {
        let s = serde_json::to_string(&kind).unwrap();
        assert_eq!(s, expected, "kind {:?} should serialize as {}", kind, expected);
    }
}

#[test]
fn phase_4_envelope_with_pubkey_round_trips_for_visibility_filter() {
    // Phase 4 expects a session-bound pubkey. The envelope round-trips it.
    let json = r#"{
        "version": 3,
        "type": "agent_action",
        "id": 1,
        "timestamp": 0,
        "source_agent_id": 1,
        "target_node_id": 1,
        "action_type": 0,
        "duration_ms": 100,
        "pubkey": "deadbeefcafe"
    }"#;
    let env: AgentActionEnvelope = serde_json::from_str(json).unwrap();
    assert_eq!(env.pubkey.as_deref(), Some("deadbeefcafe"));
}

#[test]
fn phase_4_visibility_filter_flag_default_off() {
    // Document the default: PUBKEY_VISIBILITY_FILTER not set ⇒ opacify-only,
    // matching ADR-050 baseline.
    let enabled = std::env::var("PUBKEY_VISIBILITY_FILTER")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    assert!(!enabled, "feature flag must default to off for backward compat");
}
