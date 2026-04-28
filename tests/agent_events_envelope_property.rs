//! ADR-059 / ADR-014 — envelope edge-case tests.
//!
//! `proptest` is not in dev-dependencies (verified against Cargo.toml as of
//! 2026-04-28), so these are handcrafted edge-case sweeps that emulate
//! property-based coverage:
//!
//!   1. Round-trip JSON ↔ struct over a curated set of arbitrary URN strings
//!      (incl. unicode, control chars, long strings).
//!   2. All `UserInteractionKind` variants serialize lowercase.
//!   3. `beam_color()` is total over `u8` — never panics, always returns a
//!      non-empty `&'static str`.
//!   4. `has_identity()` is true iff at least one of `pubkey` / `source_urn`
//!      is `Some` (truth-table coverage).

use webxr::agent_events::{
    AgentActionEnvelope, UserInteractionEvent, UserInteractionKind,
};

fn make_envelope(
    source_urn: Option<&str>,
    target_urn: Option<&str>,
    pubkey: Option<&str>,
    action_type: u8,
) -> AgentActionEnvelope {
    AgentActionEnvelope {
        version: 3,
        event_type: "agent_action".to_string(),
        id: 1,
        timestamp: 1714312345678,
        source_agent_id: 7,
        source_urn: source_urn.map(String::from),
        target_node_id: 4242,
        target_urn: target_urn.map(String::from),
        action_type,
        duration_ms: 250,
        pubkey: pubkey.map(String::from),
        metadata: serde_json::Value::Null,
    }
}

// ---------------------------------------------------------------------------
// Round-trip JSON ↔ struct over an arbitrary corpus of URN-like strings.
// ---------------------------------------------------------------------------

#[test]
fn round_trip_arbitrary_urn_corpus() {
    let urns: &[&str] = &[
        "",
        "urn:visionclaw:concept:bc:smart-contract",
        "did:nostr:79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        "urn:visionclaw:kg:npub1xyz:sha256-12-deadbeefcafe",
        "urn:agentbox:agent:devuser:claude",
        // Edge: pure ASCII with %-encoded bits
        "urn:visionclaw:concept:bc:hello%20world",
        // Edge: unicode (BMP + supplementary)
        "urn:visionclaw:concept:bc:ünïcødé-π",
        "urn:visionclaw:concept:bc:🦀",
        // Edge: long string (1 KiB)
        &"a".repeat(1024),
        // Edge: control-char-ish (tabs / spaces are valid in JSON strings)
        "urn:visionclaw:concept:bc:\tfoo bar",
        // Edge: backslash and quote — ensure JSON encoder escapes correctly
        "urn:visionclaw:concept:bc:\"quoted\"\\backslash",
    ];

    for urn in urns {
        let env = make_envelope(Some(urn), Some(urn), Some(urn), 0);
        let json = serde_json::to_string(&env).expect("serialize must succeed");
        let back: AgentActionEnvelope =
            serde_json::from_str(&json).expect("deserialize must succeed");
        assert_eq!(back.source_urn.as_deref(), Some(*urn));
        assert_eq!(back.target_urn.as_deref(), Some(*urn));
        assert_eq!(back.pubkey.as_deref(), Some(*urn));
        assert!(back.has_identity());
    }
}

#[test]
fn round_trip_handles_optional_field_omission() {
    // None values must be elided (skip_serializing_if = "Option::is_none").
    let env = make_envelope(None, None, None, 1);
    let json = serde_json::to_string(&env).unwrap();
    assert!(!json.contains("source_urn"), "source_urn should be elided when None");
    assert!(!json.contains("target_urn"), "target_urn should be elided when None");
    assert!(!json.contains("pubkey"), "pubkey should be elided when None");

    // And inverse: re-parsing the elided form yields all-None.
    let back: AgentActionEnvelope = serde_json::from_str(&json).unwrap();
    assert!(back.source_urn.is_none());
    assert!(back.target_urn.is_none());
    assert!(back.pubkey.is_none());
}

// ---------------------------------------------------------------------------
// UserInteractionKind: all variants serialize lowercase.
// ---------------------------------------------------------------------------

#[test]
fn user_interaction_kind_lowercase_exhaustive() {
    let pairs = [
        (UserInteractionKind::Focus, "\"focus\""),
        (UserInteractionKind::Select, "\"select\""),
        (UserInteractionKind::Hover, "\"hover\""),
        (UserInteractionKind::Drag, "\"drag\""),
    ];
    for (kind, expected) in pairs {
        let s = serde_json::to_string(&kind).unwrap();
        assert_eq!(s, expected, "{:?} must serialize as {}", kind, expected);
        // And parses back.
        let back: UserInteractionKind = serde_json::from_str(&s).unwrap();
        assert_eq!(back, kind);
    }
}

#[test]
fn user_interaction_kind_rejects_uppercase_input() {
    // serde rename_all = "lowercase" → uppercase variant names should fail.
    let r: Result<UserInteractionKind, _> = serde_json::from_str("\"Focus\"");
    assert!(r.is_err(), "uppercase input must be rejected");
    let r: Result<UserInteractionKind, _> = serde_json::from_str("\"FOCUS\"");
    assert!(r.is_err(), "all-caps input must be rejected");
}

#[test]
fn user_interaction_event_round_trip_with_optional_pubkey() {
    let mut evt = UserInteractionEvent::new(
        UserInteractionKind::Drag,
        "session-uuid",
        99,
        500,
    );
    evt.session_pubkey = Some("deadbeef".into());
    evt.target_urn = Some("urn:visionclaw:kg:npub1xyz:sha256-12-abc123".into());

    let json = serde_json::to_string(&evt).unwrap();
    let back: UserInteractionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back.session_pubkey.as_deref(), Some("deadbeef"));
    assert_eq!(back.target_urn.as_deref(), Some("urn:visionclaw:kg:npub1xyz:sha256-12-abc123"));
    assert_eq!(back.duration_ms, 500);
}

// ---------------------------------------------------------------------------
// beam_color: total over u8, never panics.
// ---------------------------------------------------------------------------

#[test]
fn beam_color_is_total_over_u8() {
    for action_type in 0u8..=255u8 {
        let env = make_envelope(None, None, None, action_type);
        let color = env.beam_color();
        assert!(!color.is_empty(), "color must be non-empty for action_type={}", action_type);
        // Exact spec: 0..=5 are mapped colours, 6..=255 are unknown grey.
        match action_type {
            0 => assert_eq!(color, "#3b82f6"),
            1 => assert_eq!(color, "#facc15"),
            2 => assert_eq!(color, "#22c55e"),
            3 => assert_eq!(color, "#ef4444"),
            4 => assert_eq!(color, "#a855f7"),
            5 => assert_eq!(color, "#06b6d4"),
            _ => assert_eq!(color, "#9ca3af"),
        }
        // Hex shape invariant for every output.
        assert!(color.starts_with('#'));
        assert_eq!(color.len(), 7);
        assert!(color.chars().skip(1).all(|c| c.is_ascii_hexdigit()));
    }
}

// ---------------------------------------------------------------------------
// has_identity: truth table over (pubkey, source_urn).
// ---------------------------------------------------------------------------

#[test]
fn has_identity_truth_table() {
    let cases = [
        // (pubkey,        source_urn,    expected)
        (None,             None,          false),
        (Some("p"),        None,          true),
        (None,             Some("urn:x"), true),
        (Some("p"),        Some("urn:x"), true),
        // Empty string is still Some(_), so identity counts as present.
        // (Validating non-emptiness is a Phase 5 concern per ADR-059.)
        (Some(""),         None,          true),
        (None,             Some(""),      true),
    ];
    for (pubkey, source_urn, expected) in cases {
        let env = make_envelope(source_urn, None, pubkey, 0);
        assert_eq!(
            env.has_identity(),
            expected,
            "has_identity({:?}, {:?}) should be {}",
            pubkey, source_urn, expected
        );
    }
}

// ---------------------------------------------------------------------------
// Numeric edge cases: u32::MAX, u64::MAX, i64 negative.
// ---------------------------------------------------------------------------

#[test]
fn numeric_extremes_round_trip() {
    let env = AgentActionEnvelope {
        version: u8::MAX,
        event_type: "agent_action".to_string(),
        id: u64::MAX,
        timestamp: i64::MIN,
        source_agent_id: u32::MAX,
        source_urn: None,
        target_node_id: u32::MAX,
        target_urn: None,
        action_type: u8::MAX,
        duration_ms: u16::MAX,
        pubkey: None,
        metadata: serde_json::Value::Null,
    };
    let json = serde_json::to_string(&env).unwrap();
    let back: AgentActionEnvelope = serde_json::from_str(&json).unwrap();
    assert_eq!(back.version, u8::MAX);
    assert_eq!(back.id, u64::MAX);
    assert_eq!(back.timestamp, i64::MIN);
    assert_eq!(back.source_agent_id, u32::MAX);
    assert_eq!(back.target_node_id, u32::MAX);
    assert_eq!(back.duration_ms, u16::MAX);
    assert_eq!(back.beam_color(), "#9ca3af"); // 255 → unknown grey
}
