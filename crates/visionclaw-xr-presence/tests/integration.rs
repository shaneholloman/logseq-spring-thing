use visionclaw_xr_presence::{
    decode, encode, monotonic_timestamp, velocity_gate, AvatarId, AvatarMetadata, Did,
    PresenceRoom, RoomError, RoomId, Transform, ValidationError,
};
use visionclaw_xr_presence::types::PoseFrame;

fn room() -> RoomId {
    RoomId::parse("urn:visionclaw:room:sha256-12-0123456789ab").unwrap()
}

fn did(seed: u8) -> Did {
    Did::parse(format!("did:nostr:{}", format!("{:02x}", seed).repeat(32))).unwrap()
}

fn meta(d: &Did) -> AvatarMetadata {
    AvatarMetadata {
        did: d.clone(),
        display_name: "tester".into(),
        model_uri: None,
    }
}

fn frame_at(ts_us: u64, x: f32) -> PoseFrame {
    PoseFrame {
        timestamp_us: ts_us,
        head: Transform {
            position: [x, 1.7, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
        },
        left_hand: None,
        right_hand: None,
    }
}

#[test]
fn wire_round_trip_through_real_room() {
    let mut r = PresenceRoom::new(room());
    let d = did(0xAA);
    let avatar = r.join(d.clone(), meta(&d)).unwrap();

    let frame = frame_at(1_000_000, 0.0);
    let bytes = encode(&frame, &room(), &avatar).unwrap();
    let decoded = decode(&bytes).unwrap();

    assert_eq!(decoded.frame, frame);
    assert_eq!(decoded.avatar_id, avatar.as_str());

    let delta = r.update_pose(&avatar, frame.clone()).unwrap();
    assert!(delta.head.is_some(), "first frame should populate head");
}

#[test]
fn duplicate_did_join_rejected() {
    let mut r = PresenceRoom::new(room());
    let d = did(0xBB);
    r.join(d.clone(), meta(&d)).unwrap();
    let err = r.join(d.clone(), meta(&d)).unwrap_err();
    assert!(matches!(err, RoomError::DuplicateDid { .. }));
}

#[test]
fn velocity_gate_edge_cases() {
    let prev = frame_at(0, 0.0);
    let exactly_at_limit = frame_at(1_000_000, 20.0);
    velocity_gate(&prev, &exactly_at_limit, 20.0).unwrap();

    let just_over = frame_at(1_000_000, 20.001);
    let err = velocity_gate(&prev, &just_over, 20.0).unwrap_err();
    assert!(matches!(err, ValidationError::VelocityExceeded { .. }));

    let zero_dt = PoseFrame { ..prev.clone() };
    let err2 = velocity_gate(&prev, &zero_dt, 20.0).unwrap_err();
    assert!(matches!(err2, ValidationError::NonMonotonicTimestamp { .. }));
}

#[test]
fn monotonic_timestamp_replay_attack_blocked() {
    monotonic_timestamp(0, 10_000).unwrap();
    let err = monotonic_timestamp(10_000, 10_000).unwrap_err();
    assert!(matches!(err, ValidationError::DuplicateTimestamp { .. }));
}

#[test]
fn monotonic_timestamp_backwards_blocked() {
    let err = monotonic_timestamp(50_000, 10_000).unwrap_err();
    assert!(matches!(err, ValidationError::NonMonotonicTimestamp { .. }));
}

#[test]
fn pose_update_after_leave_rejected() {
    let mut r = PresenceRoom::new(room());
    let d = did(0xCC);
    let avatar = r.join(d.clone(), meta(&d)).unwrap();
    r.leave(&avatar).unwrap();
    let err = r.update_pose(&avatar, frame_at(0, 0.0)).unwrap_err();
    assert!(matches!(err, RoomError::UnknownAvatar { .. }));
}

#[test]
fn avatar_id_derived_from_did_pubkey() {
    let d = did(0xDD);
    let id = AvatarId::from_did(&d);
    assert_eq!(id.pubkey_hex(), d.pubkey_hex());
    let parsed = AvatarId::parse(id.as_str()).unwrap();
    assert_eq!(parsed, id);
}

#[test]
fn invalid_did_rejected() {
    assert!(matches!(
        Did::parse("did:nostr:tooshort"),
        Err(RoomError::InvalidDid { .. })
    ));
    assert!(matches!(
        Did::parse("did:other:".to_owned() + &"a".repeat(64)),
        Err(RoomError::InvalidDid { .. })
    ));
}

#[test]
fn invalid_room_urn_rejected() {
    assert!(matches!(
        RoomId::parse("urn:visionclaw:room:invalid"),
        Err(RoomError::InvalidUrn { .. })
    ));
    assert!(matches!(
        RoomId::parse("urn:visionclaw:room:sha256-12-XYZ"),
        Err(RoomError::InvalidUrn { .. })
    ));
}
