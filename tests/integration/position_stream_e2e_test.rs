//! Server integration test: ForceComputeActor → ClientCoordinator path
//! produces a 24 B/node frame with monotonic sequence numbering.
//!
//! Pins (PRD-007 §4.1 / ADR-061 §D1 / DDD invariant I04):
//!   - The bytes produced by `ClientCoordinatorActor::serialize_positions`
//!     for N nodes are exactly `9 + 24 * N`.
//!   - Frame preamble is 0x42.
//!   - `broadcast_sequence` strictly increases across consecutive
//!     `BroadcastPositions` messages.
//!   - The actor's serialised body byte-for-byte equals the canonical
//!     `encode_position_frame` output (single canonical encoder).
//!
//! Implementation under test (Workstream A): `ClientCoordinatorActor::
//! serialize_positions` becomes a thin wrapper around
//! `binary_protocol::encode_position_frame`, dropping the
//! `private_opaque_ids`, `analytics_data`, and `node_type_arrays`
//! parameters from the per-frame path. Workstream A also exposes a
//! `current_broadcast_sequence()` getter (a public read of the existing
//! `broadcast_sequence: u64` field) so this test can observe the
//! monotonic advance invariant. This file will not compile until those
//! changes land — RED phase.

use actix::prelude::*;

use webxr::actors::client_coordinator_actor::ClientCoordinatorActor;
use webxr::actors::messages::{BroadcastPositions, GetBroadcastSequence};
use webxr::utils::binary_protocol::{decode_position_frame, encode_position_frame};
use webxr::utils::socket_flow_messages::{BinaryNodeData, BinaryNodeDataClient};

const PREAMBLE: u8 = 0x42;
const NODE_STRIDE: usize = 24;
const HEADER_LEN: usize = 9;

fn client_pos(node_id: u32, x: f32, y: f32, z: f32) -> BinaryNodeDataClient {
    BinaryNodeDataClient {
        node_id,
        x,
        y,
        z,
        vx: 0.0,
        vy: 0.0,
        vz: 0.0,
    }
}

// ---------------------------------------------------------------------------
// Wire shape via the actor's serializer
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn serialize_positions_produces_24b_per_node_with_0x42_preamble() {
    // GIVEN: A ClientCoordinatorActor with two positions queued for
    // broadcast.
    let actor = ClientCoordinatorActor::new();
    let positions = vec![client_pos(1, 1.0, 2.0, 3.0), client_pos(42, 4.0, 5.0, 6.0)];

    // WHEN: We ask the actor to serialise the positions for the wire. After
    // Workstream A this is a thin wrapper over `encode_position_frame`.
    let serialised = actor.serialize_positions(&positions);

    // THEN: Exactly `9 + 24 * 2` bytes — header + two-node body.
    assert_eq!(
        serialised.len(),
        HEADER_LEN + NODE_STRIDE * 2,
        "two-node frame must be exactly 9 + 24*2 = 57 bytes (got {})",
        serialised.len()
    );

    // THEN: Preamble byte is 0x42.
    assert_eq!(
        serialised[0], PREAMBLE,
        "frame preamble must be 0x42, got 0x{:02X}",
        serialised[0]
    );

    // THEN: The bytes round-trip through the standalone decoder, yielding
    // the two ids in order.
    let (_seq, decoded) =
        decode_position_frame(&serialised).expect("frame must decode without error");
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].0, 1);
    assert_eq!(decoded[1].0, 42);
}

#[actix_rt::test]
async fn empty_position_list_serialises_to_just_the_header() {
    // GIVEN: Empty position list (e.g. all clients are filtered out).
    let actor = ClientCoordinatorActor::new();
    let positions: Vec<BinaryNodeDataClient> = Vec::new();

    // WHEN: Serialised.
    let serialised = actor.serialize_positions(&positions);

    // THEN: Exactly the 9-byte header.
    assert_eq!(serialised.len(), HEADER_LEN);
    assert_eq!(serialised[0], PREAMBLE);
}

// ---------------------------------------------------------------------------
// Sequence monotonicity across BroadcastPositions calls
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn broadcast_positions_increments_sequence_monotonically() {
    // GIVEN: A live ClientCoordinatorActor running in an actix arbiter.
    // The actor's BroadcastPositions handler increments
    // `broadcast_sequence` BEFORE the manager fan-out (see ADR-031). The
    // current value is exposed via `current_broadcast_sequence()` once
    // Workstream A lands a public getter for the existing private field.
    let actor_addr = ClientCoordinatorActor::new().start();

    let before = actor_addr
        .send(GetBroadcastSequence)
        .await
        .expect("GetBroadcastSequence must succeed before broadcasts");

    // WHEN: We send two BroadcastPositions messages back-to-back.
    actor_addr.do_send(BroadcastPositions {
        positions: vec![client_pos(1, 0.0, 0.0, 0.0)],
    });
    actor_addr.do_send(BroadcastPositions {
        positions: vec![client_pos(2, 1.0, 1.0, 1.0)],
    });

    // Allow actix to process the queued do_send calls.
    actix_rt::time::sleep(std::time::Duration::from_millis(50)).await;

    let after = actor_addr
        .send(GetBroadcastSequence)
        .await
        .expect("GetBroadcastSequence must succeed after broadcasts");

    // THEN: The broadcast_sequence advanced by at least 2 across the two
    // BroadcastPositions sends. Strictly increasing per DDD invariant I04.
    assert!(
        after >= before + 2,
        "broadcast_sequence must advance by >= 2 across two BroadcastPositions \
         (before={before}, after={after})"
    );
}

// ---------------------------------------------------------------------------
// Producer-side: encode_position_frame is the canonical path
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn actor_serialised_frame_body_matches_canonical_encoder_output() {
    // GIVEN: The same node payload represented two ways.
    let actor = ClientCoordinatorActor::new();
    let client_form = vec![client_pos(7, 1.5, 2.5, 3.5)];
    let canonical_form: Vec<(u32, BinaryNodeData)> = client_form
        .iter()
        .map(|p| {
            (
                p.node_id,
                BinaryNodeData {
                    node_id: p.node_id,
                    x: p.x,
                    y: p.y,
                    z: p.z,
                    vx: p.vx,
                    vy: p.vy,
                    vz: p.vz,
                },
            )
        })
        .collect();

    // WHEN: Both paths produce a frame. The actor mints its own sequence;
    // we ask the canonical encoder to produce the same shape with
    // sequence=0 to compare body bytes.
    let actor_bytes = actor.serialize_positions(&client_form);
    let canon_bytes = encode_position_frame(&canonical_form, 0);

    // THEN: Both bodies are 9 + 24*1 long; the 8-byte sequence portion
    // (offsets 1..9) may legitimately differ. Preamble byte and per-node
    // body bytes (offsets 9..) must match exactly — proving the actor
    // routes through `encode_position_frame` rather than re-implementing.
    assert_eq!(actor_bytes.len(), canon_bytes.len());
    assert_eq!(actor_bytes[0], canon_bytes[0]);
    assert_eq!(actor_bytes[0], PREAMBLE);
    assert_eq!(
        &actor_bytes[HEADER_LEN..],
        &canon_bytes[HEADER_LEN..],
        "actor's per-frame body must be byte-identical to the canonical \
         encoder output (single source of truth for wire layout)"
    );
}

// `GetBroadcastSequence` is the read-only stats message Workstream A adds
// to `webxr::actors::messages`. Its handler returns the actor's current
// `broadcast_sequence: u64`. We import it at the top of the file rather
// than redeclaring (orphan rule); the message itself lives in the actor
// crate so its `Handler<…>` impl is in-crate and lawful.
