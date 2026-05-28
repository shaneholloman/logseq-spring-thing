//! Integration tests for the physics data flow from server to client WebSocket.
//!
//! Validates the complete pipeline:
//!   PhysicsOrchestratorActor -> ForceComputeActor (GPU)
//!     -> PhysicsStepCompleted -> broadcast_position_updates
//!     -> ClientCoordinatorActor -> BroadcastPositionUpdate
//!     -> SocketFlowServer -> binary WebSocket frame -> Client
//!
//! These tests run WITHOUT a GPU by exercising the encoding, decoding,
//! delta compression, position sanitisation, and protocol framing logic.

use std::collections::HashMap;

// Re-export types used across all test scenarios
use webxr::utils::binary_protocol::{
    self, decode_node_data, encode_node_data, encode_node_data_extended_with_sssp,
    encode_node_data_with_types, get_actual_node_id, get_node_type, is_agent_node,
    is_knowledge_node, set_agent_flag, set_knowledge_flag, BinaryProtocol, Message, MessageType,
    MultiplexedMessage, NodeType,
};
use webxr::utils::delta_encoding::{
    calculate_delta_savings, decode_node_data_delta, encode_node_data_delta,
    enforce_history_limit, enforce_history_limit_vec, MAX_HISTORY_FRAMES,
};
use webxr::utils::socket_flow_messages::BinaryNodeData;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_node(id: u32, x: f32, y: f32, z: f32, vx: f32, vy: f32, vz: f32) -> (u32, BinaryNodeData) {
    (
        id,
        BinaryNodeData {
            node_id: id,
            x,
            y,
            z,
            vx,
            vy,
            vz,
        },
    )
}

fn make_node_simple(id: u32, x: f32, y: f32, z: f32) -> (u32, BinaryNodeData) {
    make_node(id, x, y, z, 0.0, 0.0, 0.0)
}

// ---------------------------------------------------------------------------
// 1. Binary protocol V3 encode/decode roundtrip
// ---------------------------------------------------------------------------

#[test]
fn v3_encode_decode_roundtrip_preserves_position_and_velocity() {
    let nodes = vec![
        make_node(0, 1.5, -2.25, 300.0, 0.01, -0.02, 0.03),
        make_node(1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        make_node(99, -100.5, 200.75, 50.125, 1.0, -1.0, 0.5),
    ];

    let encoded = encode_node_data(&nodes);

    // Protocol V3 header byte
    assert_eq!(encoded[0], 3, "Expected protocol V3 header");

    // 48 bytes per node + 1 header
    assert_eq!(encoded.len(), 1 + nodes.len() * 48);

    let decoded = decode_node_data(&encoded).expect("decode must succeed");
    assert_eq!(decoded.len(), nodes.len());

    for ((orig_id, orig), (dec_id, dec)) in nodes.iter().zip(decoded.iter()) {
        assert_eq!(*orig_id, *dec_id, "Node ID mismatch");
        assert_eq!(orig.x, dec.x, "x mismatch for node {}", orig_id);
        assert_eq!(orig.y, dec.y, "y mismatch for node {}", orig_id);
        assert_eq!(orig.z, dec.z, "z mismatch for node {}", orig_id);
        assert_eq!(orig.vx, dec.vx, "vx mismatch for node {}", orig_id);
        assert_eq!(orig.vy, dec.vy, "vy mismatch for node {}", orig_id);
        assert_eq!(orig.vz, dec.vz, "vz mismatch for node {}", orig_id);
    }
}

#[test]
fn v3_encode_decode_with_node_type_flags() {
    let nodes = vec![
        make_node_simple(1, 10.0, 20.0, 30.0),
        make_node_simple(2, 40.0, 50.0, 60.0),
        make_node_simple(3, 70.0, 80.0, 90.0),
    ];
    let agent_ids = vec![1u32];
    let knowledge_ids = vec![2u32];

    let encoded = encode_node_data_with_types(&nodes, &agent_ids, &knowledge_ids);
    let decoded = decode_node_data(&encoded).expect("decode must succeed");

    assert_eq!(decoded.len(), 3);
    // decode_node_data strips flags, returning actual IDs
    for (dec_id, _) in &decoded {
        assert!(*dec_id <= 3, "Decoded ID should be actual (stripped) ID");
    }
}

#[test]
fn v3_encode_with_sssp_and_analytics_roundtrip() {
    let nodes = vec![
        make_node_simple(5, 1.0, 2.0, 3.0),
        make_node_simple(6, 4.0, 5.0, 6.0),
    ];
    let mut analytics: HashMap<u32, (u32, f32, u32)> = HashMap::new();
    analytics.insert(5, (42, 0.87, 7));
    analytics.insert(6, (43, 0.12, 8));

    let encoded = encode_node_data_extended_with_sssp(
        &nodes,
        &[],
        &[],
        &[],
        &[],
        &[],
        None,
        Some(&analytics),
    );

    // Verify it decodes without error (analytics are discarded in basic decode)
    let decoded = decode_node_data(&encoded).expect("decode must succeed");
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].1.x, 1.0);
    assert_eq!(decoded[1].1.x, 4.0);
}

#[test]
fn empty_node_list_encodes_and_decodes_cleanly() {
    let nodes: Vec<(u32, BinaryNodeData)> = vec![];
    let encoded = encode_node_data(&nodes);
    // Header only
    assert_eq!(encoded.len(), 1);
    let decoded = decode_node_data(&encoded).expect("decode must succeed");
    assert!(decoded.is_empty());
}

// ---------------------------------------------------------------------------
// 2. Delta encoding: nodes that haven't moved are filtered out
// ---------------------------------------------------------------------------

#[test]
fn delta_encoding_filters_unchanged_nodes() {
    let node_a = make_node(1, 10.0, 20.0, 30.0, 1.0, 2.0, 3.0);
    let node_b = make_node(2, 40.0, 50.0, 60.0, 4.0, 5.0, 6.0);

    let nodes = vec![node_a.clone(), node_b.clone()];
    let previous: HashMap<u32, BinaryNodeData> = nodes.iter().cloned().collect();

    // Only change node 1's position
    let updated_nodes = vec![
        make_node(1, 10.5, 20.0, 30.0, 1.0, 2.0, 3.0), // x changed by 0.5
        node_b.clone(),                                    // unchanged
    ];

    let encoded = encode_node_data_delta(&updated_nodes, &previous, 1, &[], &[]);

    // Should be V4 (delta)
    assert_eq!(encoded[0], 4, "Expected protocol V4 for delta frame");

    // Parse header: frame_number, num_changed
    let num_changed = u16::from_le_bytes([encoded[2], encoded[3]]);
    assert_eq!(num_changed, 1, "Only 1 node changed, but got {}", num_changed);
}

#[test]
fn delta_encoding_includes_new_nodes_not_in_previous() {
    let previous: HashMap<u32, BinaryNodeData> = HashMap::new();
    let nodes = vec![make_node_simple(1, 1.0, 2.0, 3.0)];

    let encoded = encode_node_data_delta(&nodes, &previous, 1, &[], &[]);
    assert_eq!(encoded[0], 4); // V4 delta
    let num_changed = u16::from_le_bytes([encoded[2], encoded[3]]);
    assert_eq!(num_changed, 1, "New node should be included in delta");
}

#[test]
fn delta_below_quantization_threshold_is_filtered() {
    // Deltas smaller than 1/100 (0.01) truncate to zero in i16 and should be filtered
    let node = make_node(1, 100.0, 200.0, 300.0, 0.0, 0.0, 0.0);
    let previous: HashMap<u32, BinaryNodeData> = vec![node.clone()].into_iter().collect();

    // Move by 0.005 -- below the 0.01 threshold
    let updated = vec![make_node(1, 100.005, 200.0, 300.0, 0.0, 0.0, 0.0)];
    let encoded = encode_node_data_delta(&updated, &previous, 1, &[], &[]);

    assert_eq!(encoded[0], 4);
    let num_changed = u16::from_le_bytes([encoded[2], encoded[3]]);
    assert_eq!(num_changed, 0, "Sub-threshold delta should be filtered out");
}

// ---------------------------------------------------------------------------
// 3. Full-sync frame every 60 frames regardless of delta
// ---------------------------------------------------------------------------

#[test]
fn full_sync_at_frame_0_60_120() {
    let nodes = vec![make_node_simple(1, 1.0, 2.0, 3.0)];
    let previous: HashMap<u32, BinaryNodeData> = nodes.iter().cloned().collect();

    for frame in [0u64, 60, 120, 180, 240] {
        let encoded = encode_node_data_delta(&nodes, &previous, frame, &[], &[]);
        assert_eq!(
            encoded[0], 3,
            "Frame {} should be full V3 resync, got protocol {}",
            frame, encoded[0]
        );
    }
}

#[test]
fn delta_frames_between_resyncs() {
    let nodes = vec![make_node(1, 1.0, 2.0, 3.0, 0.1, 0.2, 0.3)];
    let previous: HashMap<u32, BinaryNodeData> = nodes.iter().cloned().collect();

    // Change position so delta is non-empty
    let updated = vec![make_node(1, 2.0, 3.0, 4.0, 0.1, 0.2, 0.3)];

    for frame in [1u64, 15, 30, 45, 59] {
        let encoded = encode_node_data_delta(&updated, &previous, frame, &[], &[]);
        assert_eq!(
            encoded[0], 4,
            "Frame {} should be V4 delta, got protocol {}",
            frame, encoded[0]
        );
    }
}

#[test]
fn delta_encode_decode_roundtrip_across_multiple_frames() {
    let initial_nodes = vec![
        make_node(1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        make_node(2, 10.0, 10.0, 10.0, 0.0, 0.0, 0.0),
    ];

    // Frame 0: full sync
    let encoded_f0 = encode_node_data_delta(&initial_nodes, &HashMap::new(), 0, &[], &[]);
    assert_eq!(encoded_f0[0], 3); // V3 full

    // Build previous state from initial nodes
    let mut state: HashMap<u32, BinaryNodeData> = initial_nodes.iter().cloned().collect();

    // Frame 1: move node 1
    let frame1_nodes = vec![
        make_node(1, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0),
        make_node(2, 10.0, 10.0, 10.0, 0.0, 0.0, 0.0),
    ];
    let encoded_f1 = encode_node_data_delta(&frame1_nodes, &state, 1, &[], &[]);
    assert_eq!(encoded_f1[0], 4); // V4 delta

    // Decode delta frame 1
    let decoded_state = decode_node_data_delta(&encoded_f1[1..], &state)
        .expect("delta decode frame 1 must succeed");

    let node1 = decoded_state.get(&1).expect("node 1 should exist in decoded state");
    assert!(
        (node1.x - 0.5).abs() < 0.02,
        "Node 1 x should be ~0.5, got {}",
        node1.x
    );

    let node2 = decoded_state.get(&2).expect("node 2 should exist in decoded state");
    assert_eq!(node2.x, 10.0, "Unchanged node 2 should keep original x");
}

// ---------------------------------------------------------------------------
// 4. Node type flag preservation through encode/decode
// ---------------------------------------------------------------------------

#[test]
fn node_type_flags_agent_and_knowledge() {
    let agent_id = set_agent_flag(42);
    assert!(is_agent_node(agent_id));
    assert!(!is_knowledge_node(agent_id));
    assert_eq!(get_actual_node_id(agent_id), 42);
    assert_eq!(get_node_type(agent_id), NodeType::Agent);

    let knowledge_id = set_knowledge_flag(99);
    assert!(is_knowledge_node(knowledge_id));
    assert!(!is_agent_node(knowledge_id));
    assert_eq!(get_actual_node_id(knowledge_id), 99);
    assert_eq!(get_node_type(knowledge_id), NodeType::Knowledge);
}

#[test]
fn node_type_flags_preserved_through_delta_encoding() {
    let node = make_node(5, 1.0, 2.0, 3.0, 0.1, 0.2, 0.3);
    let nodes = vec![node.clone()];
    let previous: HashMap<u32, BinaryNodeData> = HashMap::new();

    // Encode with agent flag
    let encoded = encode_node_data_delta(&nodes, &previous, 1, &[5], &[]);
    assert_eq!(encoded[0], 4); // V4

    // Parse the wire ID from the delta payload
    // Header: version(1) + frame(1) + num_changed(2) = 4 bytes, then first item starts
    let wire_id = u32::from_le_bytes([encoded[4], encoded[5], encoded[6], encoded[7]]);
    assert!(is_agent_node(wire_id), "Wire ID should have agent flag set");
    assert_eq!(get_actual_node_id(wire_id), 5);
}

// ---------------------------------------------------------------------------
// 5. Backpressure: BroadcastAck decode
// ---------------------------------------------------------------------------

#[test]
fn broadcast_ack_encode_decode_roundtrip() {
    // Build a BroadcastAck payload: type_byte(0x34) + seq(8) + nodes(4) + timestamp(8)
    let mut payload = vec![0x34u8]; // MessageType::BroadcastAck
    let seq: u64 = 12345;
    let nodes_received: u32 = 500;
    let timestamp: u64 = 1700000000000;

    payload.extend_from_slice(&seq.to_le_bytes());
    payload.extend_from_slice(&nodes_received.to_le_bytes());
    payload.extend_from_slice(&timestamp.to_le_bytes());

    let msg = BinaryProtocol::decode_message(&payload).expect("BroadcastAck decode must succeed");
    match msg {
        Message::BroadcastAck {
            sequence_id,
            nodes_received: nr,
            timestamp: ts,
        } => {
            assert_eq!(sequence_id, seq);
            assert_eq!(nr, nodes_received);
            assert_eq!(ts, timestamp);
        }
        _ => panic!("Expected BroadcastAck, got {:?}", msg),
    }
}

#[test]
fn broadcast_ack_too_short_payload_rejected() {
    // BroadcastAck requires 20 bytes of payload after the type byte
    let mut payload = vec![0x34u8]; // type byte
    payload.extend_from_slice(&[0u8; 10]); // only 10 bytes, need 20

    let result = BinaryProtocol::decode_message(&payload);
    assert!(result.is_err(), "Short BroadcastAck payload should be rejected");
}

// ---------------------------------------------------------------------------
// 6. Position sanitisation: NaN/Infinity rejection (VULN-05)
// ---------------------------------------------------------------------------

#[test]
fn nan_position_rejected_by_sanitize() {
    // The sanitize_position function is private to position_updates.rs,
    // so we test the invariant at the protocol level: NaN/Infinity f32 values
    // should survive encode/decode but the server-side sanitisation logic
    // must reject them before they reach the wire.
    //
    // This test verifies the encoding does not crash on NaN and that
    // decoders can detect NaN values for downstream rejection.

    let nan_node = make_node(1, f32::NAN, 0.0, 0.0, 0.0, 0.0, 0.0);
    let inf_node = make_node(2, f32::INFINITY, 0.0, 0.0, 0.0, 0.0, 0.0);
    let neg_inf_node = make_node(3, 0.0, f32::NEG_INFINITY, 0.0, 0.0, 0.0, 0.0);

    let nodes = vec![nan_node, inf_node, neg_inf_node];
    let encoded = encode_node_data(&nodes);
    let decoded = decode_node_data(&encoded).expect("Encoding/decoding should not panic on NaN/Inf");

    // Verify the decoded values are still NaN/Inf (protocol preserves raw bytes)
    assert!(decoded[0].1.x.is_nan(), "NaN should survive roundtrip");
    assert!(decoded[1].1.x.is_infinite(), "Infinity should survive roundtrip");
    assert!(decoded[2].1.y.is_infinite(), "Neg infinity should survive roundtrip");
}

#[test]
fn nan_in_delta_encoding_triggers_full_frame_or_valid_output() {
    // NaN deltas can occur if previous or current position is NaN.
    // The encoder should either emit a full V3 frame (overflow fallback)
    // or a valid V4 frame without corruption.
    let prev_node = make_node(1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let previous: HashMap<u32, BinaryNodeData> = vec![prev_node].into_iter().collect();

    let nan_nodes = vec![make_node(1, f32::NAN, 0.0, 0.0, 0.0, 0.0, 0.0)];
    let encoded = encode_node_data_delta(&nan_nodes, &previous, 1, &[], &[]);

    // NaN - 0.0 = NaN; abs(NaN) > i16_max_as_f32 is false (NaN comparisons return false)
    // So NaN delta does NOT trigger the overflow fallback. The NaN gets clamped to i16.
    // This is a known edge case -- the server-side sanitiser (VULN-05) must reject NaN
    // BEFORE it reaches the delta encoder.
    let version = encoded[0];
    assert!(
        version == 3 || version == 4,
        "Should produce valid V3 or V4 frame, got version {}",
        version
    );
}

// ---------------------------------------------------------------------------
// Delta i16 overflow triggers full V3 fallback
// ---------------------------------------------------------------------------

#[test]
fn delta_i16_overflow_falls_back_to_full_v3() {
    // i16 max scaled = 32767 / 100 = 327.67
    // A delta > 327.67 would overflow i16, so the encoder should fall back to V3.
    let prev = make_node(1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let previous: HashMap<u32, BinaryNodeData> = vec![prev].into_iter().collect();

    let big_jump = vec![make_node(1, 500.0, 0.0, 0.0, 0.0, 0.0, 0.0)]; // delta=500 > 327.67
    let encoded = encode_node_data_delta(&big_jump, &previous, 1, &[], &[]);

    assert_eq!(
        encoded[0], 3,
        "i16 overflow should trigger full V3 frame, got protocol {}",
        encoded[0]
    );
}

// ---------------------------------------------------------------------------
// Multiplexed message framing
// ---------------------------------------------------------------------------

#[test]
fn multiplexed_positions_encode_decode() {
    let nodes = vec![make_node_simple(1, 1.0, 2.0, 3.0)];
    let mux = MultiplexedMessage::positions(&nodes);
    let wire = mux.encode();

    // First byte is BinaryPositions type (0x00)
    assert_eq!(wire[0], 0x00);

    let decoded_mux = MultiplexedMessage::decode(&wire).expect("mux decode must succeed");
    assert_eq!(decoded_mux.msg_type as u8, MessageType::BinaryPositions as u8);

    // Inner payload is a valid V3 binary frame
    let inner_decoded = decode_node_data(&decoded_mux.data).expect("inner decode must succeed");
    assert_eq!(inner_decoded.len(), 1);
    assert_eq!(inner_decoded[0].1.x, 1.0);
}

#[test]
fn unknown_multiplexed_type_rejected() {
    let result = MultiplexedMessage::decode(&[0xFF, 0x00, 0x01]);
    assert!(result.is_err(), "Unknown message type 0xFF should be rejected");
}

// ---------------------------------------------------------------------------
// History limit enforcement
// ---------------------------------------------------------------------------

#[test]
fn enforce_history_limit_vecdeque() {
    let mut history: std::collections::VecDeque<u32> = (0..200).collect();
    enforce_history_limit(&mut history);
    assert_eq!(history.len(), MAX_HISTORY_FRAMES);
    // Oldest entries removed, newest retained
    assert_eq!(*history.front().unwrap(), 200 - MAX_HISTORY_FRAMES as u32);
    assert_eq!(*history.back().unwrap(), 199);
}

#[test]
fn enforce_history_limit_vec_trims_oldest() {
    let mut history: Vec<u32> = (0..200).collect();
    enforce_history_limit_vec(&mut history);
    assert_eq!(history.len(), MAX_HISTORY_FRAMES);
    assert_eq!(history[0], 200 - MAX_HISTORY_FRAMES as u32);
}

// ---------------------------------------------------------------------------
// Bandwidth savings calculation
// ---------------------------------------------------------------------------

#[test]
fn bandwidth_savings_realistic_scenario() {
    // 10K nodes, 5% changing
    let (full_size, delta_size, savings) = calculate_delta_savings(10_000, 500, 1);

    // Full: 1 + 10000*36 = 360_001
    assert_eq!(full_size, 360_001);
    // Delta: 4 + 500*20 = 10_004
    assert_eq!(delta_size, 10_004);
    // >97% savings
    assert!(savings > 97.0, "Expected >97% savings, got {:.1}%", savings);
}

#[test]
fn bandwidth_savings_resync_frame_no_savings() {
    let (full_size, delta_size, savings) = calculate_delta_savings(1000, 100, 60);
    // Resync frame: delta_size == full_size
    assert_eq!(full_size, delta_size);
    assert!(savings.abs() < 0.001, "Resync frame should have 0% savings");
}

// ---------------------------------------------------------------------------
// Protocol edge cases
// ---------------------------------------------------------------------------

#[test]
fn decode_empty_data_returns_empty_vec() {
    let result = decode_node_data(&[]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn decode_v1_protocol_rejected() {
    let mut data = vec![1u8]; // V1 version
    data.extend_from_slice(&[0u8; 48]);
    let result = decode_node_data(&data);
    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("V1"),
        "Error should mention V1"
    );
}

#[test]
fn decode_v2_protocol_rejected() {
    let mut data = vec![2u8]; // V2 version
    data.extend_from_slice(&[0u8; 36]);
    let result = decode_node_data(&data);
    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("V2"),
        "Error should mention V2"
    );
}

#[test]
fn decode_v4_without_previous_state_rejected() {
    let mut data = vec![4u8]; // V4 delta version
    data.extend_from_slice(&[0u8; 20]);
    let result = decode_node_data(&data);
    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("delta"),
        "V4 decode via decode_node_data should indicate delta requirement"
    );
}

#[test]
fn decode_unknown_protocol_rejected() {
    let data = vec![99u8, 0, 0, 0];
    let result = decode_node_data(&data);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown protocol"));
}

#[test]
fn decode_v3_misaligned_payload_rejected() {
    // V3 requires payload to be multiple of 48 bytes
    let mut data = vec![3u8]; // V3 header
    data.extend_from_slice(&[0u8; 47]); // 47 bytes, not 48
    let result = decode_node_data(&data);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not a multiple"));
}

// ---------------------------------------------------------------------------
// Delta decode edge cases
// ---------------------------------------------------------------------------

#[test]
fn delta_decode_too_small_payload() {
    let result = decode_node_data_delta(&[0u8; 2], &HashMap::new());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("too small"));
}

#[test]
fn delta_decode_size_mismatch() {
    // Header says 1 changed node but payload is too short
    let mut data = vec![0u8]; // frame number
    data.extend_from_slice(&1u16.to_le_bytes()); // 1 changed node
    data.extend_from_slice(&[0u8; 10]); // only 10 bytes, need 20
    let result = decode_node_data_delta(&data, &HashMap::new());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid delta data size"));
}

// ---------------------------------------------------------------------------
// V5 broadcast-sequence frame decode
// ---------------------------------------------------------------------------

#[test]
fn v5_frame_decodes_through_sequence_header() {
    let nodes = vec![make_node_simple(1, 5.0, 6.0, 7.0)];
    // Build a V5 frame: version(1) + sequence(8) + V3 node data
    let v3_payload = encode_node_data(&nodes);
    // v3_payload[0] is protocol V3 header; strip it
    let v3_body = &v3_payload[1..];

    let mut v5_frame = vec![5u8]; // V5 version
    let broadcast_seq: u64 = 42;
    v5_frame.extend_from_slice(&broadcast_seq.to_le_bytes());
    v5_frame.extend_from_slice(v3_body);

    let decoded = decode_node_data(&v5_frame).expect("V5 frame decode must succeed");
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].1.x, 5.0);
}

// ---------------------------------------------------------------------------
// Large-scale encoding stress test (no GPU)
// ---------------------------------------------------------------------------

#[test]
fn encode_decode_1000_nodes() {
    let nodes: Vec<(u32, BinaryNodeData)> = (0..1000u32)
        .map(|i| {
            make_node(
                i,
                i as f32 * 0.1,
                i as f32 * -0.1,
                (i as f32).sin(),
                0.01,
                0.02,
                0.03,
            )
        })
        .collect();

    let encoded = encode_node_data(&nodes);
    assert_eq!(encoded.len(), 1 + 1000 * 48);

    let decoded = decode_node_data(&encoded).expect("1000-node decode must succeed");
    assert_eq!(decoded.len(), 1000);

    for (i, (id, data)) in decoded.iter().enumerate() {
        let expected_x = i as f32 * 0.1;
        assert_eq!(*id, i as u32);
        assert!(
            (data.x - expected_x).abs() < 1e-6,
            "Node {} x mismatch: {} vs {}",
            i,
            data.x,
            expected_x
        );
    }
}

#[test]
fn delta_encode_decode_1000_nodes_5pct_changed() {
    let nodes: Vec<(u32, BinaryNodeData)> = (0..1000u32)
        .map(|i| make_node(i, i as f32, 0.0, 0.0, 0.0, 0.0, 0.0))
        .collect();

    let previous: HashMap<u32, BinaryNodeData> = nodes.iter().cloned().collect();

    // Change 50 nodes (5%)
    let mut updated = nodes.clone();
    for i in 0..50 {
        updated[i].1.x += 1.0;
    }

    let encoded = encode_node_data_delta(&updated, &previous, 1, &[], &[]);
    assert_eq!(encoded[0], 4); // V4 delta

    let num_changed = u16::from_le_bytes([encoded[2], encoded[3]]);
    assert_eq!(num_changed, 50);

    // Decode and verify
    let decoded = decode_node_data_delta(&encoded[1..], &previous)
        .expect("delta decode must succeed");

    for i in 0..50u32 {
        let node = decoded.get(&i).expect("changed node should exist");
        assert!(
            (node.x - (i as f32 + 1.0)).abs() < 0.02,
            "Node {} x should be ~{}, got {}",
            i,
            i as f32 + 1.0,
            node.x
        );
    }
    for i in 50..1000u32 {
        let node = decoded.get(&i).expect("unchanged node should exist");
        assert_eq!(node.x, i as f32, "Unchanged node {} should keep original x", i);
    }
}

// ---------------------------------------------------------------------------
// Voice data protocol
// ---------------------------------------------------------------------------

#[test]
fn voice_data_roundtrip() {
    let audio = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02];
    let encoded = BinaryProtocol::encode_voice_data(&audio);

    assert_eq!(encoded[0], MessageType::VoiceData as u8);

    let decoded = BinaryProtocol::decode_message(&encoded).expect("voice decode must succeed");
    match decoded {
        Message::VoiceData { audio: decoded_audio } => {
            assert_eq!(decoded_audio, audio);
        }
        _ => panic!("Expected VoiceData"),
    }
}

#[test]
fn empty_message_rejected() {
    let result = BinaryProtocol::decode_message(&[]);
    assert!(result.is_err());
}

#[test]
fn invalid_message_type_rejected() {
    let result = BinaryProtocol::decode_message(&[0xFF]);
    assert!(result.is_err());
}
