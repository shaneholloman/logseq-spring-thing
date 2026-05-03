//! Integration tests for the XR presence service (PRD-008 §5.3).
//!
//! These exercise the per-room actor end-to-end (auth handshake plumbing is
//! tested at the unit level alongside the handler module). The test suite
//! covers join → ingest → broadcast-to-peer, validation rejection, and the
//! sender-not-echoed contract.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use actix::prelude::*;

use webxr::actors::presence_actor::{
    BroadcastFrame, IngestOutcome, IngestPose, JoinRoom, LeaveRoom, PresenceActor,
    RoomEventEnvelope,
};
use webxr::services::nostr_identity_verifier::WellFormedOnlyVerifier;
use visionclaw_xr_presence::{
    ports::{IdentityVerifier, SignedChallenge},
    types::{Aabb, AvatarMetadata, Did, PoseFrame, RoomId, Transform},
    wire::encode,
};

const ROOM_URN: &str = "urn:visionclaw:room:sha256-12-deadbeef1234";

struct CollectActor {
    frames: Arc<Mutex<Vec<BroadcastFrame>>>,
    events: Arc<Mutex<Vec<RoomEventEnvelope>>>,
}

impl Actor for CollectActor {
    type Context = Context<Self>;
}

impl Handler<BroadcastFrame> for CollectActor {
    type Result = ();
    fn handle(&mut self, msg: BroadcastFrame, _: &mut Context<Self>) {
        self.frames.lock().expect("frames mutex").push(msg);
    }
}

impl Handler<RoomEventEnvelope> for CollectActor {
    type Result = ();
    fn handle(&mut self, msg: RoomEventEnvelope, _: &mut Context<Self>) {
        self.events.lock().expect("events mutex").push(msg);
    }
}

fn fake_did(byte: u8) -> Did {
    Did::parse(format!("did:nostr:{}", format!("{:02x}", byte).repeat(32))).unwrap()
}

fn meta(d: &Did, name: &str) -> AvatarMetadata {
    AvatarMetadata {
        did: d.clone(),
        display_name: name.into(),
        model_uri: None,
    }
}

fn sample_frame(ts_us: u64, x: f32) -> PoseFrame {
    PoseFrame {
        timestamp_us: ts_us,
        head: Transform {
            position: [x, 1.6, -0.3],
            rotation: [0.0, 0.0, 0.0, 1.0],
        },
        left_hand: None,
        right_hand: None,
    }
}

#[actix::test]
async fn ingest_broadcasts_to_peer_only() {
    let room = RoomId::parse(ROOM_URN).unwrap();
    let actor = PresenceActor::new(room.clone()).start();

    let frames_a = Arc::new(Mutex::new(Vec::new()));
    let events_a = Arc::new(Mutex::new(Vec::new()));
    let frames_b = Arc::new(Mutex::new(Vec::new()));
    let events_b = Arc::new(Mutex::new(Vec::new()));

    let collector_a = CollectActor {
        frames: frames_a.clone(),
        events: events_a,
    }
    .start();
    let collector_b = CollectActor {
        frames: frames_b.clone(),
        events: events_b,
    }
    .start();

    let did_a = fake_did(0x10);
    let did_b = fake_did(0x20);

    let ack_a = actor
        .send(JoinRoom {
            did: did_a.clone(),
            metadata: meta(&did_a, "alice"),
            frame_recipient: collector_a.clone().recipient(),
            event_recipient: collector_a.clone().recipient(),
        })
        .await
        .expect("mailbox a")
        .expect("join a");

    let _ack_b = actor
        .send(JoinRoom {
            did: did_b.clone(),
            metadata: meta(&did_b, "bob"),
            frame_recipient: collector_b.clone().recipient(),
            event_recipient: collector_b.clone().recipient(),
        })
        .await
        .expect("mailbox b")
        .expect("join b");

    let bytes = encode(&sample_frame(1_000_000, 0.5), &room, &ack_a.avatar_id)
        .expect("encode")
        .to_vec();
    let outcome = actor
        .send(IngestPose {
            avatar_id: ack_a.avatar_id.clone(),
            frame_bytes: bytes,
        })
        .await
        .expect("ingest mailbox");
    assert_eq!(outcome, IngestOutcome::Accepted);

    actix_rt::time::sleep(Duration::from_millis(50)).await;
    assert!(
        frames_a.lock().unwrap().is_empty(),
        "sender should not receive its own broadcast"
    );
    let received = frames_b.lock().unwrap();
    assert_eq!(received.len(), 1, "peer should receive one broadcast");
    assert_eq!(received[0].bytes[0], 0x43, "preamble must be opcode 0x43");

    actor.send(LeaveRoom { avatar_id: ack_a.avatar_id }).await.unwrap();
    System::current().stop();
}

#[actix::test]
async fn ingest_rejects_out_of_bounds_pose() {
    let room = RoomId::parse(ROOM_URN).unwrap();
    let actor = PresenceActor::new(room.clone())
        .with_bounds(Aabb::symmetric(2.0), 20.0)
        .start();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let collector = CollectActor { frames, events }.start();

    let did = fake_did(0x42);
    let ack = actor
        .send(JoinRoom {
            did: did.clone(),
            metadata: meta(&did, "eve"),
            frame_recipient: collector.clone().recipient(),
            event_recipient: collector.clone().recipient(),
        })
        .await
        .unwrap()
        .unwrap();

    let mut bad = sample_frame(1_000_000, 0.5);
    bad.head.position = [100.0, 0.0, 0.0];
    let bytes = encode(&bad, &room, &ack.avatar_id).unwrap().to_vec();
    let outcome = actor
        .send(IngestPose {
            avatar_id: ack.avatar_id.clone(),
            frame_bytes: bytes,
        })
        .await
        .unwrap();
    assert!(
        matches!(outcome, IngestOutcome::ValidationFailed(_)),
        "expected validation failure, got {outcome:?}"
    );
    System::current().stop();
}

#[actix::test]
async fn duplicate_did_join_rejected() {
    let room = RoomId::parse(ROOM_URN).unwrap();
    let actor = PresenceActor::new(room.clone()).start();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let collector = CollectActor { frames, events }.start();

    let did = fake_did(0xa1);
    let _ok = actor
        .send(JoinRoom {
            did: did.clone(),
            metadata: meta(&did, "alice"),
            frame_recipient: collector.clone().recipient(),
            event_recipient: collector.clone().recipient(),
        })
        .await
        .unwrap()
        .unwrap();

    let dup = actor
        .send(JoinRoom {
            did: did.clone(),
            metadata: meta(&did, "alice2"),
            frame_recipient: collector.clone().recipient(),
            event_recipient: collector.clone().recipient(),
        })
        .await
        .unwrap();
    assert!(dup.is_err(), "duplicate DID join must be rejected");
    System::current().stop();
}

#[test]
fn well_formed_verifier_rejects_bad_signature() {
    let v = WellFormedOnlyVerifier::default();
    let did = fake_did(0xff);
    let res = v.verify_signed_challenge(&SignedChallenge {
        nonce: [0u8; 32],
        timestamp_us: 0,
        claimed_pubkey_hex: did.pubkey_hex().to_owned(),
        signature_hex: "not-hex".into(),
    });
    assert!(res.is_err());
}

#[test]
fn well_formed_verifier_accepts_well_formed() {
    let v = WellFormedOnlyVerifier::default();
    let did = fake_did(0x77);
    let res = v.verify_signed_challenge(&SignedChallenge {
        nonce: [0u8; 32],
        timestamp_us: 0,
        claimed_pubkey_hex: did.pubkey_hex().to_owned(),
        signature_hex: "ab".repeat(64),
    });
    assert!(res.is_ok(), "well-formed sig must pass: {res:?}");
}
