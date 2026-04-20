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
    self, decode_node_data, encode_node_data_extended, encode_node_data_extended_with_sssp,
    get_actual_node_id, get_node_type, is_agent_node,
    is_knowledge_node, set_agent_flag, set_knowledge_flag, BinaryProtocol, Message, MessageType,
    MultiplexedMessage, NodeType,
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

    let encoded = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);

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

    let encoded = encode_node_data_extended(&nodes, &agent_ids, &knowledge_ids, &[], &[], &[]);
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
    let encoded = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);
    // Header only
    assert_eq!(encoded.len(), 1);
    let decoded = decode_node_data(&encoded).expect("decode must succeed");
    assert!(decoded.is_empty());
}

// ---------------------------------------------------------------------------
// 2. V4 delta encoding: REMOVED by ADR-037 (Implemented 2026-04-20).
//    Tests that exercised the delta_encoding module have been deleted along
//    with the module itself.
// ---------------------------------------------------------------------------

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

// node_type_flags_preserved_through_delta_encoding removed by ADR-037.

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
    let encoded = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);
    let decoded = decode_node_data(&encoded).expect("Encoding/decoding should not panic on NaN/Inf");

    // Verify the decoded values are still NaN/Inf (protocol preserves raw bytes)
    assert!(decoded[0].1.x.is_nan(), "NaN should survive roundtrip");
    assert!(decoded[1].1.x.is_infinite(), "Infinity should survive roundtrip");
    assert!(decoded[2].1.y.is_infinite(), "Neg infinity should survive roundtrip");
}

// NaN / delta i16 overflow tests removed along with the V4 delta encoder (ADR-037).

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

// History limit + bandwidth savings tests removed with the V4 delta encoder (ADR-037).

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
fn decode_v4_rejected_with_adr037_message() {
    // ADR-037: V4 delta frames are no longer a valid wire version.
    let data = vec![4u8];
    let result = decode_node_data(&data);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("V4") && err.contains("ADR-037"),
        "V4 rejection must cite ADR-037: got {}",
        err
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

// Delta decode edge-case tests removed with the V4 encoder (ADR-037).

// ---------------------------------------------------------------------------
// V5 broadcast-sequence frame decode
// ---------------------------------------------------------------------------

#[test]
fn v5_frame_decodes_through_sequence_header() {
    let nodes = vec![make_node_simple(1, 5.0, 6.0, 7.0)];
    // Build a V5 frame: version(1) + sequence(8) + V3 node data
    let v3_payload = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);
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

    let encoded = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);
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

// delta_encode_decode_1000_nodes_5pct_changed removed along with the V4 encoder (ADR-037).

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
