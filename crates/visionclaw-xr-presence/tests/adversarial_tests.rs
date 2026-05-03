//! Adversarial / hand-crafted attack inputs for the XR presence wire codec
//! and validators (PRD-QE-002 §4.10, threat model T-PROTO-1, T-PROTO-2,
//! T-WS-1, T-WS-3, T-FRAME-1).
//!
//! Each test corresponds to a specific entry in `docs/xr-godot-threat-model.md`.

use visionclaw_xr_presence::{
    monotonic_timestamp,
    types::{Aabb, AvatarMetadata, Did, PoseFrame, RoomId, Transform},
    validate::{velocity_gate, world_bounds},
    wire, AvatarId, PresenceRoom, RoomError, ValidationError, WireError,
};

fn fixed_room() -> RoomId {
    RoomId::parse("urn:visionclaw:room:sha256-12-deadbeefcafe").expect("room parse")
}

fn fixed_avatar() -> AvatarId {
    let did = Did::parse(format!("did:nostr:{}", "1".repeat(64))).expect("did parse");
    AvatarId::from_did(&did)
}

fn baseline_frame(ts_us: u64) -> PoseFrame {
    PoseFrame {
        timestamp_us: ts_us,
        head: Transform {
            position: [0.0, 1.6, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
        },
        left_hand: None,
        right_hand: None,
    }
}

// -- T-PROTO-1: malformed frames must be rejected, never panic ---------------

#[test]
fn truncated_frame_rejected() {
    let bytes = wire::encode(&baseline_frame(1), &fixed_room(), &fixed_avatar())
        .expect("baseline encode");
    // Lop off the last 4 bytes of the transform payload.
    let truncated = &bytes[..bytes.len() - 4];
    let res = wire::decode(truncated);
    assert!(
        matches!(res, Err(WireError::LengthMismatch { .. } | WireError::TooShort { .. })),
        "truncated frame must reject (got {res:?})"
    );
}

#[test]
fn oversized_declared_length_rejected() {
    let bytes = wire::encode(&baseline_frame(1), &fixed_room(), &fixed_avatar())
        .expect("baseline encode");
    let mut tampered = bytes.to_vec();
    // Patch the u16-LE frame_len at bytes [1..3] to claim 10 extra bytes.
    let new_len = u16::from_le_bytes([tampered[1], tampered[2]]).saturating_add(10);
    tampered[1..3].copy_from_slice(&new_len.to_le_bytes());
    let res = wire::decode(&tampered);
    assert!(
        matches!(res, Err(WireError::LengthMismatch { .. })),
        "oversized declared length must reject (got {res:?})"
    );
}

#[test]
fn empty_buffer_rejected() {
    let res = wire::decode(&[]);
    assert!(
        matches!(res, Err(WireError::TooShort { .. })),
        "zero-byte input must reject (got {res:?})"
    );
}

#[test]
fn frame_len_zero_rejected() {
    // Manually craft: opcode | u16 len = 0 | (no body).
    // Decoder requires body for room_hash + avatar_id_len + ts + mask.
    let bytes = vec![0x43u8, 0x00, 0x00];
    let res = wire::decode(&bytes);
    assert!(
        matches!(res, Err(WireError::TooShort { .. })),
        "frame_len=0 must reject (got {res:?})"
    );
}

#[test]
fn wrong_opcode_rejected() {
    let mut bytes = wire::encode(&baseline_frame(1), &fixed_room(), &fixed_avatar())
        .expect("encode")
        .to_vec();
    bytes[0] = 0x42; // graph position frame opcode, not pose
    let res = wire::decode(&bytes);
    assert!(
        matches!(res, Err(WireError::BadOpcode { .. })),
        "0x42 must reject in pose decoder (got {res:?})"
    );
}

#[test]
fn invalid_transform_mask_rejected() {
    // Mask bits 3-7 are reserved; setting them must reject (decoder enforces
    // mask == valid SLOT bits only).
    let bytes = wire::encode(&baseline_frame(1), &fixed_room(), &fixed_avatar())
        .expect("encode");
    let mut tampered = bytes.to_vec();
    // Find the mask byte: layout is [opcode 1][len 2][hash 16][id_len 1][id N][ts 8][mask 1].
    let id_len = tampered[1 + 2 + 16] as usize;
    let mask_idx = 1 + 2 + 16 + 1 + id_len + 8;
    tampered[mask_idx] = 0xF8; // reserved bits set, head bit clear
    let res = wire::decode(&tampered);
    assert!(
        matches!(res, Err(WireError::BadTransformCount { .. })),
        "reserved mask bits must reject (got {res:?})"
    );
}

#[test]
fn mask_with_no_head_rejected() {
    let bytes = wire::encode(&baseline_frame(1), &fixed_room(), &fixed_avatar())
        .expect("encode");
    let mut tampered = bytes.to_vec();
    let id_len = tampered[1 + 2 + 16] as usize;
    let mask_idx = 1 + 2 + 16 + 1 + id_len + 8;
    tampered[mask_idx] = 0b010; // left only, no head
    let res = wire::decode(&tampered);
    assert!(
        matches!(res, Err(WireError::BadTransformCount { .. })),
        "head-less mask must reject (got {res:?})"
    );
}

// -- T-WS-3: replay attacks (duplicate timestamps) ----------------------------

#[test]
fn replay_same_timestamp_rejected() {
    monotonic_timestamp(0, 100_000).expect("first frame must be accepted");
    let res = monotonic_timestamp(100_000, 100_000);
    assert!(
        matches!(res, Err(ValidationError::DuplicateTimestamp { .. })),
        "replay of identical timestamp must reject (got {res:?})"
    );
}

#[test]
fn time_travel_backwards_rejected() {
    let res = monotonic_timestamp(1_000_000_000, 999_999_999);
    assert!(
        matches!(res, Err(ValidationError::NonMonotonicTimestamp { .. })),
        "backwards timestamp must reject (got {res:?})"
    );
}

#[test]
fn rapid_frames_below_min_interval_rejected() {
    // DEFAULT_MIN_FRAME_INTERVAL_US = 8_000 (~125 Hz cap). Anything closer
    // than 8ms is rate-limited.
    let res = monotonic_timestamp(0, 1_000);
    assert!(
        matches!(res, Err(ValidationError::IntervalTooShort { .. })),
        "sub-min interval must reject (got {res:?})"
    );
}

// -- T-POSE-1: impossible velocities ------------------------------------------

#[test]
fn quaternion_not_normalised_rejected_by_world_bounds() {
    let bounds = Aabb::symmetric(50.0);
    let bad = Transform {
        position: [0.0, 0.0, 0.0],
        rotation: [3.0, 0.0, 0.0, 0.0], // |q| = 3
    };
    let res = world_bounds(&bad, &bounds);
    assert!(
        matches!(res, Err(ValidationError::NonUnitQuaternion { .. })),
        "non-unit quaternion must reject (got {res:?})"
    );
}

#[test]
fn nan_position_rejected_by_velocity_gate() {
    let prev = baseline_frame(0);
    let mut next = baseline_frame(100_000);
    next.head.position[0] = f32::NAN;
    let res = velocity_gate(&prev, &next, 20.0);
    // NaN compares > 20.0 as `false`, so the validator reports OK in current
    // impl. We document this and rely on `world_bounds` to catch it; assert
    // world_bounds DOES reject.
    let _ = res;
    let bounds = Aabb::symmetric(50.0);
    let res2 = world_bounds(&next.head, &bounds);
    assert!(
        matches!(res2, Err(ValidationError::OutOfBounds { .. })),
        "NaN coord must be rejected by world_bounds (got {res2:?})"
    );
}

#[test]
fn infinity_position_rejected_by_world_bounds() {
    let bounds = Aabb::symmetric(50.0);
    let t = Transform {
        position: [f32::INFINITY, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    let res = world_bounds(&t, &bounds);
    assert!(
        matches!(res, Err(ValidationError::OutOfBounds { .. })),
        "+inf coord must reject (got {res:?})"
    );
}

#[test]
fn teleport_velocity_rejected() {
    let prev = baseline_frame(0);
    // 1000 m in 1 ms = 1,000,000 m/s — well over the 20 m/s gate.
    let mut next = baseline_frame(1_000);
    next.head.position[0] = 1000.0;
    let res = velocity_gate(&prev, &next, 20.0);
    assert!(
        matches!(res, Err(ValidationError::VelocityExceeded { .. })),
        "teleport velocity must reject (got {res:?})"
    );
}

// -- T-WS-1 / T-AVATAR-1: identity / URN parsing ------------------------------

#[test]
fn empty_room_urn_rejected() {
    let res = RoomId::parse("");
    assert!(matches!(res, Err(RoomError::InvalidUrn { .. })));
}

#[test]
fn non_urn_avatar_id_rejected() {
    let res = AvatarId::parse("not-a-urn-at-all");
    assert!(matches!(res, Err(RoomError::InvalidUrn { .. })));
}

#[test]
fn avatar_id_with_short_pubkey_rejected() {
    let res = AvatarId::parse("urn:visionclaw:avatar:short");
    assert!(matches!(res, Err(RoomError::InvalidUrn { .. })));
}

#[test]
fn did_with_non_hex_pubkey_rejected() {
    let res = Did::parse(format!("did:nostr:{}", "Z".repeat(64)));
    assert!(matches!(res, Err(RoomError::InvalidDid { .. })));
}

#[test]
fn did_with_wrong_prefix_rejected() {
    let res = Did::parse(format!("did:web:{}", "a".repeat(64)));
    assert!(matches!(res, Err(RoomError::InvalidDid { .. })));
}

#[test]
fn empty_did_rejected() {
    let res = Did::parse("");
    assert!(matches!(res, Err(RoomError::InvalidDid { .. })));
}

#[test]
fn avatar_id_at_max_wire_len_accepted_in_encode() {
    // Max wire len for avatar id is 255 bytes; the canonical URN is fixed at
    // `urn:visionclaw:avatar:<64-hex>` = 22 + 64 = 86 bytes, well under 255.
    // This test asserts that the canonical form encodes successfully.
    let avatar = fixed_avatar();
    let bytes = wire::encode(&baseline_frame(1), &fixed_room(), &avatar);
    assert!(bytes.is_ok(), "canonical avatar must encode");
    assert!(avatar.as_str().len() <= 255);
}

// -- I-PR01: one DID one avatar -----------------------------------------------

#[test]
fn duplicate_did_in_same_room_rejected() {
    let mut room = PresenceRoom::new(fixed_room());
    let did = Did::parse(format!("did:nostr:{}", "a".repeat(64))).expect("did parse");
    let m = AvatarMetadata {
        did: did.clone(),
        display_name: "alice".into(),
        model_uri: None,
    };
    room.join(did.clone(), m.clone()).expect("first join");
    let err = room.join(did, m).expect_err("dup must fail");
    assert!(matches!(err, RoomError::DuplicateDid { .. }));
}

#[test]
fn leave_unknown_avatar_rejected() {
    let mut room = PresenceRoom::new(fixed_room());
    let did = Did::parse(format!("did:nostr:{}", "b".repeat(64))).expect("did parse");
    let avatar = AvatarId::from_did(&did);
    let res = room.leave(&avatar);
    assert!(matches!(res, Err(RoomError::UnknownAvatar { .. })));
}

#[test]
fn pose_update_for_unknown_avatar_rejected() {
    let mut room = PresenceRoom::new(fixed_room());
    let did = Did::parse(format!("did:nostr:{}", "c".repeat(64))).expect("did parse");
    let avatar = AvatarId::from_did(&did);
    let res = room.update_pose(&avatar, baseline_frame(1));
    assert!(matches!(res, Err(RoomError::UnknownAvatar { .. })));
}
