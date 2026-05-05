//! Edge-case integration tests for the binary protocol public API.

use bytes::Bytes;
use visionclaw_xr_gdext::binary_protocol::{
    decode_position_frame, ingest_frame, OPCODE_POSITION_FRAME, NODE_RECORD_BYTES,
};

/// Helper: build a valid 0x42 frame from a slice of (node_id, position, velocity).
fn build_frame(records: &[(u32, [f32; 3], [f32; 3])]) -> Vec<u8> {
    let mut out = vec![OPCODE_POSITION_FRAME];
    for (id, pos, vel) in records {
        out.extend_from_slice(&id.to_le_bytes());
        for v in pos {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in vel {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

#[test]
fn empty_candidates_returns_none() {
    // A frame with no records decodes to an empty vec, which means
    // any downstream "find first matching node" logic gets nothing.
    let frame = vec![OPCODE_POSITION_FRAME];
    let decoded = decode_position_frame(&frame).unwrap();
    assert!(decoded.is_empty());
    assert_eq!(decoded.first(), None);
}

#[test]
fn large_frame_100_nodes() {
    let records: Vec<(u32, [f32; 3], [f32; 3])> = (0u32..100)
        .map(|i| {
            let f = i as f32;
            (i, [f, f * 2.0, f * 3.0], [f * 0.01, f * 0.02, f * 0.03])
        })
        .collect();
    let frame = build_frame(&records);

    // Verify frame size: 1-byte header + 100 * 28 bytes
    assert_eq!(frame.len(), 1 + 100 * NODE_RECORD_BYTES);

    let decoded = decode_position_frame(&frame).unwrap();
    assert_eq!(decoded.len(), 100);

    for (i, update) in decoded.iter().enumerate() {
        let f = i as f32;
        assert_eq!(update.node_id, i as u32);
        assert_eq!(update.position, [f, f * 2.0, f * 3.0]);
        assert_eq!(update.velocity, [f * 0.01, f * 0.02, f * 0.03]);
    }
}

#[test]
fn header_only_zero_nodes() {
    let frame = vec![OPCODE_POSITION_FRAME];
    let decoded = decode_position_frame(&frame).unwrap();
    assert_eq!(decoded, vec![]);
}

#[test]
fn ingest_frame_100_nodes_fires_all_callbacks() {
    let records: Vec<(u32, [f32; 3], [f32; 3])> = (0u32..100)
        .map(|i| (i, [i as f32; 3], [0.0; 3]))
        .collect();
    let frame = build_frame(&records);
    let mut count = 0usize;
    let mut last_id: Option<u32> = None;
    ingest_frame(Bytes::from(frame), |u| {
        count += 1;
        last_id = Some(u.node_id);
    });
    assert_eq!(count, 100);
    assert_eq!(last_id, Some(99));
}
