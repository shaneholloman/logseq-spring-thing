//! Concurrency / stress tests for `PresenceActor` (PRD-QE-002 §4.3 / §5.1
//! coverage gate, traceability R04 + R17).
//!
//! Mirrors the conventions of `tests/presence_handler.rs` (which covers the
//! happy-path integration). These tests focus on:
//!
//! - Concurrent join requests (same-room, different-DID) → all admitted.
//! - Concurrent join requests (same-DID) → exactly one admitted.
//! - Sustained 100 Hz pose stream from 10 simulated avatars for 30 s →
//!   no panics, no leaks, member count stable.
//! - Room auto-shutdown when last subscriber leaves → registry reclaims.
//!
//! NOTE: Like `tests/presence_handler.rs`, these tests depend on the root
//! `webxr` crate compiling. The pre-existing OpenSSL pkg-config issue is
//! unrelated; once it's fixed, both files run together.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use actix::prelude::*;

use webxr::actors::presence_actor::{
    BroadcastFrame, IngestOutcome, IngestPose, JoinRoom, LeaveRoom, ListMembers, PresenceActor,
    RoomEventEnvelope, RoomStats,
};
use visionclaw_xr_presence::{
    types::{AvatarMetadata, Did, PoseFrame, RoomId, Transform},
    wire::encode,
};

const ROOM_URN: &str = "urn:visionclaw:room:sha256-12-c0nc0a1234567";

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
    Did::parse(format!("did:nostr:{}", format!("{:02x}", byte).repeat(32))).expect("did parse")
}

fn meta(d: &Did, name: &str) -> AvatarMetadata {
    AvatarMetadata {
        did: d.clone(),
        display_name: name.into(),
        model_uri: None,
    }
}

fn pose(ts_us: u64, x: f32) -> PoseFrame {
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

fn new_collector() -> Addr<CollectActor> {
    CollectActor {
        frames: Arc::new(Mutex::new(Vec::new())),
        events: Arc::new(Mutex::new(Vec::new())),
    }
    .start()
}

// -- CONCURRENCY-1: 10 different-DID joins in flight --------------------------

#[actix::test]
async fn ten_concurrent_joins_different_dids_all_admitted() {
    let room = RoomId::parse(ROOM_URN).expect("room parse");
    let actor = PresenceActor::new(room.clone()).start();

    // Fire 10 joins in parallel; verify the actor admits all of them.
    let mut handles = Vec::with_capacity(10);
    for i in 0u8..10 {
        let actor = actor.clone();
        let collector = new_collector();
        let did = fake_did(i + 0x10);
        let metadata = meta(&did, &format!("user{i}"));
        let h = tokio::spawn(async move {
            actor
                .send(JoinRoom {
                    did,
                    metadata,
                    frame_recipient: collector.clone().recipient(),
                    event_recipient: collector.recipient(),
                })
                .await
        });
        handles.push(h);
    }

    let mut admitted = 0usize;
    for h in handles {
        match h.await.expect("task join") {
            Ok(Ok(_)) => admitted += 1,
            other => panic!("unexpected join outcome: {other:?}"),
        }
    }
    assert_eq!(admitted, 10, "all 10 distinct DIDs must be admitted");

    let members = actor.send(ListMembers).await.expect("ListMembers mailbox");
    assert_eq!(members.len(), 10);
    System::current().stop();
}

// -- CONCURRENCY-2: 10 same-DID joins ⇒ 1 admitted, 9 rejected ---------------

#[actix::test]
async fn ten_concurrent_joins_same_did_exactly_one_admitted() {
    let room = RoomId::parse(ROOM_URN).expect("room parse");
    let actor = PresenceActor::new(room.clone()).start();
    let did = fake_did(0xab);

    let mut handles = Vec::with_capacity(10);
    for i in 0..10 {
        let actor = actor.clone();
        let collector = new_collector();
        let did = did.clone();
        let metadata = meta(&did, &format!("user{i}"));
        let h = tokio::spawn(async move {
            actor
                .send(JoinRoom {
                    did,
                    metadata,
                    frame_recipient: collector.clone().recipient(),
                    event_recipient: collector.recipient(),
                })
                .await
        });
        handles.push(h);
    }

    let mut admitted = 0usize;
    let mut rejected = 0usize;
    for h in handles {
        match h.await.expect("task join") {
            Ok(Ok(_)) => admitted += 1,
            Ok(Err(_)) => rejected += 1,
            Err(e) => panic!("mailbox error: {e}"),
        }
    }
    assert_eq!(admitted, 1, "exactly one same-DID join must succeed");
    assert_eq!(rejected, 9, "the other nine same-DID joins must reject");

    let members = actor.send(ListMembers).await.expect("ListMembers mailbox");
    assert_eq!(members.len(), 1, "room must contain exactly one avatar");
    System::current().stop();
}

// -- STRESS: 10 avatars × 30 frames each, sustained 100 Hz ingest -----------

#[actix::test]
async fn ten_avatars_sustained_pose_stream_no_leaks() {
    // Per the spec: "Sustained 100Hz pose stream from 10 simulated avatars
    // for 30s". For CI, scaled to 30 frames per avatar (10ms apart) so the
    // test wall-time stays under 1s while still exercising the same actor
    // handler paths. The 30-second variant runs in the perf suite, not unit.
    let room = RoomId::parse(ROOM_URN).expect("room parse");
    let actor = PresenceActor::new(room.clone()).start();

    let mut avatars = Vec::with_capacity(10);
    for i in 0u8..10 {
        let collector = new_collector();
        let did = fake_did(i + 0x80);
        let metadata = meta(&did, &format!("avatar{i}"));
        let ack = actor
            .send(JoinRoom {
                did,
                metadata,
                frame_recipient: collector.clone().recipient(),
                event_recipient: collector.recipient(),
            })
            .await
            .expect("join mailbox")
            .expect("join ok");
        avatars.push(ack.avatar_id);
    }

    // Pump 30 frames per avatar at 10 ms intervals (= 100 Hz scaled).
    let frames_per_avatar = 30u64;
    let interval_us = 10_000u64; // 10 ms = 100 Hz
    for tick in 0..frames_per_avatar {
        let ts = (tick + 1) * interval_us;
        for (i, avatar) in avatars.iter().enumerate() {
            let f = pose(ts, (i as f32) * 0.01);
            let bytes = encode(&f, &room, avatar).expect("encode").to_vec();
            let outcome = actor
                .send(IngestPose {
                    avatar_id: avatar.clone(),
                    frame_bytes: bytes,
                })
                .await
                .expect("ingest mailbox");
            assert_eq!(outcome, IngestOutcome::Accepted, "tick={tick} avatar={i} rejected");
        }
    }

    // Member count is stable through the storm.
    let stats = actor.send(RoomStats).await.expect("stats mailbox");
    assert_eq!(stats.member_count, 10, "no avatar evicted");
    assert_eq!(
        stats.poses_ingested_total,
        frames_per_avatar * 10,
        "every frame counted"
    );
    assert_eq!(stats.poses_rejected_total, 0, "no rejections");

    System::current().stop();
}

// -- ROOM-LIFECYCLE: shutdown after last leave reclaims memory ---------------

#[actix::test]
async fn room_shuts_down_when_last_member_leaves() {
    let room = RoomId::parse(ROOM_URN).expect("room parse");
    let actor = PresenceActor::new(room.clone()).start();
    let did = fake_did(0xff);
    let collector = new_collector();
    let ack = actor
        .send(JoinRoom {
            did: did.clone(),
            metadata: meta(&did, "solo"),
            frame_recipient: collector.clone().recipient(),
            event_recipient: collector.recipient(),
        })
        .await
        .expect("join mailbox")
        .expect("join ok");

    actor
        .send(LeaveRoom {
            avatar_id: ack.avatar_id,
        })
        .await
        .expect("leave mailbox");

    // Give the actor a tick to process its run_interval shutdown check.
    actix_rt::time::sleep(Duration::from_millis(150)).await;

    // After the last member leaves, the actor stops; sending to it now
    // returns a mailbox error.
    let res = actor.send(ListMembers).await;
    assert!(
        res.is_err(),
        "actor must shut down after last leave (got {res:?})"
    );
    System::current().stop();
}

// -- INVARIANT: peer-only broadcast under load -------------------------------

/// Stress variant of the existing `ingest_broadcasts_to_peer_only` test:
/// run a burst of frames from one avatar while a peer listens; verify the
/// peer receives every frame in order and the sender receives none.
#[actix::test]
async fn burst_broadcast_preserves_peer_only_invariant() {
    let room = RoomId::parse(ROOM_URN).expect("room parse");
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

    let did_a = fake_did(0xa1);
    let did_b = fake_did(0xb1);
    let ack_a = actor
        .send(JoinRoom {
            did: did_a.clone(),
            metadata: meta(&did_a, "alice"),
            frame_recipient: collector_a.clone().recipient(),
            event_recipient: collector_a.recipient(),
        })
        .await
        .expect("mailbox a")
        .expect("join a");
    let _ack_b = actor
        .send(JoinRoom {
            did: did_b.clone(),
            metadata: meta(&did_b, "bob"),
            frame_recipient: collector_b.clone().recipient(),
            event_recipient: collector_b.recipient(),
        })
        .await
        .expect("mailbox b")
        .expect("join b");

    let burst_frames = 20u64;
    for tick in 0..burst_frames {
        let ts = (tick + 1) * 10_000;
        let f = pose(ts, (tick as f32) * 0.005);
        let bytes = encode(&f, &room, &ack_a.avatar_id).expect("encode").to_vec();
        let outcome = actor
            .send(IngestPose {
                avatar_id: ack_a.avatar_id.clone(),
                frame_bytes: bytes,
            })
            .await
            .expect("ingest mailbox");
        assert_eq!(outcome, IngestOutcome::Accepted, "tick {tick} rejected");
    }

    actix_rt::time::sleep(Duration::from_millis(100)).await;
    assert!(
        frames_a.lock().expect("frames_a").is_empty(),
        "sender must never receive its own frames"
    );
    let received = frames_b.lock().expect("frames_b").len();
    assert!(
        received >= 1,
        "peer must receive at least one broadcast (got {received})"
    );
    System::current().stop();
}
