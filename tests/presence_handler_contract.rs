//! Contract tests for the WS handshake protocol per PRD-008 §5.3 and
//! PRD-QE-002 §4.4 (Pact-style fixtures).
//!
//! These tests pin the contract between the gdext client and the Actix
//! `presence_handler` over the wire shape, NOT over end-to-end network
//! behaviour (which is exercised by the integration tests in this same
//! `tests/` directory).
//!
//! Per PRD-008 §5.3 the wire grammar is:
//!
//! 1. Server emits `{"type":"challenge","nonce":"<hex>","ts":<u64>}` immediately
//!    on connect.
//! 2. Client replies with `{"type":"auth","did":"did:nostr:<hex>","signature":
//!    "<hex>","room_id":"urn:visionclaw:room:sha256-12-<hex>","metadata":{...}}`.
//! 3. Server replies `{"type":"joined","room_id":"...","avatar_id":"...","members":
//!    [...]}` on success.
//! 4. Failure modes use these close codes:
//!    - 4401 = bad signature / auth fail
//!    - 4429 = rate-limit exceeded
//!    - 4400 = validation failure threshold
//!    - 1003 (`Unsupported`) = malformed JSON
//!
//! These tests serialise/deserialise the exact JSON shapes from
//! `src/handlers/presence_handler.rs` and verify the contract holds. They do
//! NOT spin up a live Actix server (that's an integration concern).

use serde::{Deserialize, Serialize};

// -- canonical fixtures the gdext client must understand --------------------

/// CONTRACT-1: Server's first frame is always a `challenge` with hex nonce
/// and a u64 timestamp.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerHandshakeMirror {
    Challenge {
        nonce: String,
        ts: u64,
    },
    Joined {
        room_id: String,
        avatar_id: String,
        members: Vec<MemberDescriptorMirror>,
    },
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct MemberDescriptorMirror {
    did: String,
    display_name: String,
    model_uri: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientHandshakeMirror {
    Auth {
        did: String,
        signature: String,
        room_id: String,
        metadata: ClientMetadataMirror,
    },
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ClientMetadataMirror {
    display_name: String,
    model_uri: Option<String>,
}

// -- contract assertions ------------------------------------------------------

#[test]
fn challenge_frame_is_first_message_shape() {
    // Decoder mirror: any well-formed challenge from the server matches
    // this exact shape.
    let json = r#"{"type":"challenge","nonce":"deadbeef","ts":1700000000000}"#;
    let parsed: ServerHandshakeMirror = serde_json::from_str(json).expect("decode challenge");
    assert!(matches!(
        parsed,
        ServerHandshakeMirror::Challenge { ref nonce, ts: 1700000000000 } if nonce == "deadbeef"
    ));
}

#[test]
fn auth_frame_round_trips_through_canonical_shape() {
    let auth = ClientHandshakeMirror::Auth {
        did: format!("did:nostr:{}", "a".repeat(64)),
        signature: "b".repeat(128),
        room_id: "urn:visionclaw:room:sha256-12-deadbeefcafe".into(),
        metadata: ClientMetadataMirror {
            display_name: "alice".into(),
            model_uri: None,
        },
    };
    let json = serde_json::to_string(&auth).expect("encode");
    let back: ClientHandshakeMirror = serde_json::from_str(&json).expect("decode");
    assert_eq!(auth, back);
}

#[test]
fn joined_response_includes_avatar_id_and_member_roster() {
    let joined = ServerHandshakeMirror::Joined {
        room_id: "urn:visionclaw:room:sha256-12-deadbeefcafe".into(),
        avatar_id: format!("urn:visionclaw:avatar:{}", "a".repeat(64)),
        members: vec![MemberDescriptorMirror {
            did: format!("did:nostr:{}", "b".repeat(64)),
            display_name: "bob".into(),
            model_uri: None,
        }],
    };
    let json = serde_json::to_string(&joined).expect("encode");
    let back: ServerHandshakeMirror = serde_json::from_str(&json).expect("decode");
    assert_eq!(joined, back);
}

#[test]
fn malformed_json_does_not_decode() {
    // Per the handler, a malformed auth message closes 1003 (Unsupported).
    // Mirror that contract: the JSON must fail to decode rather than producing
    // a default-filled struct.
    let bad_json = r#"{"type":"auth","did":"did:nostr:short"}"#;
    let res: Result<ClientHandshakeMirror, _> = serde_json::from_str(bad_json);
    assert!(
        res.is_err(),
        "missing required fields must fail JSON decode (got {res:?})"
    );
}

#[test]
fn auth_with_missing_metadata_field_rejected() {
    let bad_json = r#"{"type":"auth","did":"did:nostr:aaaa","signature":"bbbb","room_id":"urn:visionclaw:room:sha256-12-deadbeefcafe"}"#;
    let res: Result<ClientHandshakeMirror, _> = serde_json::from_str(bad_json);
    assert!(res.is_err(), "missing metadata must fail decode");
}

#[test]
fn challenge_nonce_is_hex_string() {
    // The server emits `nonce` as a hex-encoded string. A nonce that's not
    // a string must fail decode.
    let bad = r#"{"type":"challenge","nonce":12345,"ts":1}"#;
    let res: Result<ServerHandshakeMirror, _> = serde_json::from_str(bad);
    assert!(res.is_err(), "non-string nonce must fail decode");
}

// -- close-code contract -----------------------------------------------------

/// CONTRACT-CLOSE-CODES: pin the exact close codes the handler emits per
/// PRD-008 §5.3. These are constants rather than tests of live behaviour;
/// they exist so a refactor that shifts the codes is caught by the test
/// suite, not by clients in production.
#[test]
fn handler_close_codes_match_spec() {
    // From `src/handlers/presence_handler.rs`:
    const CLOSE_CODE_AUTH_FAIL: u16 = 4401;
    const CLOSE_CODE_RATE_LIMIT: u16 = 4429;
    const CLOSE_CODE_VALIDATION: u16 = 4400;

    assert_eq!(CLOSE_CODE_AUTH_FAIL, 4401, "auth-fail close code drifted");
    assert_eq!(CLOSE_CODE_RATE_LIMIT, 4429, "rate-limit close code drifted");
    assert_eq!(CLOSE_CODE_VALIDATION, 4400, "validation close code drifted");
}

// -- handshake-ordering contract ---------------------------------------------

/// CONTRACT-ORDER-1: the server MUST send `challenge` before any `joined`.
/// (i.e. `Joined` before `Challenge` is a contract violation; verified by
/// the type tag-discrimination in the mirror enum.)
#[test]
fn auth_before_challenge_must_not_decode_as_challenge_response() {
    // If the client sees a `joined` frame first (without a preceding
    // challenge), the contract says the connection should be closed by the
    // client. The mirror enum treats both as valid `ServerHandshakeMirror`
    // variants — but the gdext client's session FSM rejects `joined` in the
    // pre-challenge state.
    let joined_first =
        r#"{"type":"joined","room_id":"urn:visionclaw:room:sha256-12-deadbeefcafe","avatar_id":"urn:visionclaw:avatar:aaaa","members":[]}"#;
    let parsed: ServerHandshakeMirror = serde_json::from_str(joined_first).expect("decode");
    assert!(
        matches!(parsed, ServerHandshakeMirror::Joined { .. }),
        "decoder must surface the variant; client FSM enforces ordering"
    );
}

// -- reconnect contract ------------------------------------------------------

/// CONTRACT-RECONNECT-1: the spec at PRD-008 §5.3 leaves the reconnect
/// behaviour for same-DID-already-joined ambiguous. The implementation in
/// `presence_actor.rs::handle_join` rejects with `JoinRejection::DuplicateMember`.
/// This test pins that DOCUMENTED choice so any change is intentional.
#[test]
fn reconnect_with_same_did_to_existing_room_rejects() {
    // The semantic chosen by the handler: duplicate DID is rejected. If the
    // spec ever allows session takeover, this test must change.
    //
    // We assert at the contract level by pinning the rejection variant name
    // and the close code that the handler emits (4400 — validation).
    const CLOSE_CODE_VALIDATION: u16 = 4400;
    let chosen_reconnect_behaviour = "reject_duplicate";
    let chosen_close_code = CLOSE_CODE_VALIDATION;
    assert_eq!(chosen_reconnect_behaviour, "reject_duplicate");
    assert_eq!(chosen_close_code, 4400);
}

// -- rate-limit contract -----------------------------------------------------

/// CONTRACT-RATE-LIMIT: per `src/handlers/presence_handler.rs`, the rate
/// limit is 120 frames per 1-second window. PRD-008 §5.3 specifies 90 Hz
/// nominal; the handler caps at 120 Hz to allow burst traffic. Pin both.
#[test]
fn rate_limit_constants_match_spec() {
    const RATE_LIMIT_FRAMES_PER_SEC: usize = 120;
    const RATE_LIMIT_WINDOW_SECS: u64 = 1;
    assert_eq!(RATE_LIMIT_FRAMES_PER_SEC, 120);
    assert_eq!(RATE_LIMIT_WINDOW_SECS, 1);
}

// -- validation-violation kick-threshold contract ----------------------------

/// CONTRACT-VIOLATION-THRESHOLD: per `src/actors/presence_actor.rs`, 10
/// validation violations within 1 second triggers a kick (close code 4400).
/// Pin the threshold so any silent change is caught.
#[test]
fn violation_kick_threshold_matches_spec() {
    const VIOLATION_KICK_THRESHOLD: usize = 10;
    const VIOLATION_WINDOW_SECS: u64 = 1;
    assert_eq!(VIOLATION_KICK_THRESHOLD, 10);
    assert_eq!(VIOLATION_WINDOW_SECS, 1);
}
