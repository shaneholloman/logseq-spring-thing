//! Server unit tests for the unified binary protocol (PRD-007 / ADR-061).
//!
//! Pins:
//!   - 9-byte header: `[u8 preamble = 0x42][u64 broadcast_sequence_LE]`
//!   - 24-byte node stride: `[u32 id_LE][f32 x][f32 y][f32 z][f32 vx][f32 vy][f32 vz]`
//!   - Total frame length: `9 + 24 * N` for any N (including N == 0).
//!   - Round-trip preserves position + velocity per node.
//!   - NO trailing flag-bit residue: a non-empty frame's length modulo 24 is
//!     exactly 9 (header). Never 9 + 48 * N.
//!
//! These are the DDD aggregate `PositionFrame` invariants I03 ("per-node
//! payload = exactly 24 B") and the structural shape from ADR-061 §D1.
//!
//! Implementation under test: `webxr::utils::binary_protocol::{
//!     encode_position_frame, decode_position_frame
//! }` — both NEW symbols introduced by Workstream A. This file will not
//! compile until those land; that is intentional — RED phase.

use webxr::utils::binary_protocol::{decode_position_frame, encode_position_frame};
use webxr::utils::socket_flow_messages::BinaryNodeData;

/// Header byte that identifies a position frame on the wire. Fixed forever
/// per ADR-061 §D1; this is NOT a version dispatch byte and the protocol
/// does not evolve via this byte.
const PREAMBLE: u8 = 0x42;

/// Per-node stride in bytes — the contract.
const NODE_STRIDE: usize = 24;

/// Header length: 1 byte preamble + 8 bytes broadcast_sequence (LE).
const HEADER_LEN: usize = 9;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn node(id: u32, x: f32, y: f32, z: f32, vx: f32, vy: f32, vz: f32) -> (u32, BinaryNodeData) {
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

/// Read a little-endian u32 at the given byte offset.
fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Read a little-endian f32 at the given byte offset.
fn read_f32_le(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Read a little-endian u64 at the given byte offset.
fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(buf)
}

// ---------------------------------------------------------------------------
// Round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn encode_then_decode_preserves_positions_and_velocities() {
    // GIVEN: Two nodes spanning negative, zero, and large-magnitude floats.
    let positions: Vec<(u32, BinaryNodeData)> = vec![
        node(1, 1.0, 2.0, 3.0, 0.1, 0.2, 0.3),
        node(42, -100.5, 0.0, 1e6, 0.0, 0.0, 0.0),
    ];

    // WHEN: Encoded then decoded.
    let bytes = encode_position_frame(&positions, 12345);

    // THEN: Frame size is exactly 9 + 24 * N.
    assert_eq!(
        bytes.len(),
        HEADER_LEN + NODE_STRIDE * positions.len(),
        "expected header (9) + 24 per node, got {} for {} nodes",
        bytes.len(),
        positions.len()
    );

    // THEN: Preamble byte is 0x42, not a version number.
    assert_eq!(
        bytes[0], PREAMBLE,
        "preamble byte must be 0x42, got 0x{:02X}",
        bytes[0]
    );

    // THEN: Broadcast sequence is preserved at offset 1, little-endian.
    assert_eq!(
        read_u64_le(&bytes, 1),
        12345,
        "broadcast_sequence must be at bytes 1..9, little-endian"
    );

    // THEN: Decoder yields back the same nodes (sequence + entries).
    let (seq, decoded) = decode_position_frame(&bytes).expect("decode must succeed for round-trip");
    assert_eq!(seq, 12345, "decoded sequence must equal encoded sequence");
    assert_eq!(decoded.len(), 2, "all nodes round-trip");

    // First node: id=1, position (1,2,3), velocity (0.1, 0.2, 0.3).
    assert_eq!(decoded[0].0, 1);
    assert!((decoded[0].1.x - 1.0).abs() < 1e-6);
    assert!((decoded[0].1.y - 2.0).abs() < 1e-6);
    assert!((decoded[0].1.z - 3.0).abs() < 1e-6);
    assert!((decoded[0].1.vx - 0.1).abs() < 1e-6);
    assert!((decoded[0].1.vy - 0.2).abs() < 1e-6);
    assert!((decoded[0].1.vz - 0.3).abs() < 1e-6);
    assert_eq!(decoded[0].1.node_id, 1, "node_id field is the raw id");

    // Second node: id=42, large-magnitude position survives float round-trip.
    assert_eq!(decoded[1].0, 42);
    assert!((decoded[1].1.x - (-100.5)).abs() < 1e-3);
    assert!((decoded[1].1.y - 0.0).abs() < 1e-6);
    assert!((decoded[1].1.z - 1e6).abs() < 1.0);
    assert_eq!(decoded[1].1.vx, 0.0);
    assert_eq!(decoded[1].1.vy, 0.0);
    assert_eq!(decoded[1].1.vz, 0.0);
}

#[test]
fn byte_offsets_within_a_node_are_exactly_id_pos_vel() {
    // GIVEN: A single, well-known node so we can read raw bytes at known
    // offsets and pin the node layout.
    let positions = vec![node(7, 1.5, 2.5, 3.5, 0.4, 0.5, 0.6)];

    // WHEN: Encoded.
    let bytes = encode_position_frame(&positions, 0xCAFEBABE_DEADBEEF);

    // THEN: Header preamble at 0, sequence at 1..9, exactly one 24-byte node
    // body starting at offset 9.
    assert_eq!(bytes.len(), HEADER_LEN + NODE_STRIDE);
    assert_eq!(bytes[0], PREAMBLE);
    assert_eq!(read_u64_le(&bytes, 1), 0xCAFEBABE_DEADBEEF);

    // THEN: Within the single node body, id at offset 9, then six f32s at
    // offsets 13, 17, 21, 25, 29, 33. This pins the 24 B/node stride byte
    // for byte and rejects any 48-byte legacy layout.
    assert_eq!(read_u32_le(&bytes, 9), 7, "id at offset 9");
    assert!((read_f32_le(&bytes, 13) - 1.5).abs() < 1e-6, "x at offset 13");
    assert!((read_f32_le(&bytes, 17) - 2.5).abs() < 1e-6, "y at offset 17");
    assert!((read_f32_le(&bytes, 21) - 3.5).abs() < 1e-6, "z at offset 21");
    assert!((read_f32_le(&bytes, 25) - 0.4).abs() < 1e-6, "vx at offset 25");
    assert!((read_f32_le(&bytes, 29) - 0.5).abs() < 1e-6, "vy at offset 29");
    assert!((read_f32_le(&bytes, 33) - 0.6).abs() < 1e-6, "vz at offset 33");

    // THEN: Total length is exactly header + 24 — never 48 (no analytics
    // residue from the legacy V3 node body).
    assert_eq!(
        bytes.len(),
        HEADER_LEN + 24,
        "single-node frame is header + 24 bytes, not header + 48"
    );
}

#[test]
fn empty_frame_is_just_the_header() {
    // GIVEN: No nodes — common during initial connect / drained physics.
    let positions: Vec<(u32, BinaryNodeData)> = vec![];

    // WHEN: Encoded.
    let bytes = encode_position_frame(&positions, 0);

    // THEN: Output is exactly the 9-byte header. No trailing padding.
    assert_eq!(bytes.len(), HEADER_LEN, "empty frame is exactly 9 bytes");
    assert_eq!(bytes[0], PREAMBLE);
    assert_eq!(read_u64_le(&bytes, 1), 0);

    // THEN: Decode round-trips an empty entry list with sequence 0.
    let (seq, decoded) =
        decode_position_frame(&bytes).expect("empty frame must decode without error");
    assert_eq!(seq, 0);
    assert!(decoded.is_empty(), "empty input -> empty decoded list");
}

// ---------------------------------------------------------------------------
// Stride regression: zero residue from the legacy 48-byte format
// ---------------------------------------------------------------------------

/// Regression test for the per-node payload contract.
///
/// The whole point of ADR-061 is that the per-node payload is **24 bytes,
/// fixed**. If anyone re-introduces analytics columns (cluster_id,
/// community_id, anomaly_score, sssp_distance, sssp_parent) into the
/// per-frame stream, the per-node stride doubles to 48 and this test
/// fails immediately.
///
/// The check `bytes.len() % NODE_STRIDE == HEADER_LEN % NODE_STRIDE`
/// (i.e. `% 24 == 9`) holds for any N when the stride is 24, but does NOT
/// hold for any N >= 1 when the stride is 48 (because `9 + 48 * N` mod 24
/// is `9` only when `N == 0`). To be unambiguous we just verify the strict
/// equality `len == HEADER_LEN + NODE_STRIDE * N`.
#[test]
fn frame_size_is_exactly_header_plus_24_per_node_no_residue() {
    for n in [0usize, 1, 2, 5, 100] {
        let positions: Vec<(u32, BinaryNodeData)> = (0..n as u32)
            .map(|i| node(i, i as f32, 0.0, 0.0, 0.0, 0.0, 0.0))
            .collect();

        let bytes = encode_position_frame(&positions, n as u64);

        let expected = HEADER_LEN + NODE_STRIDE * n;
        assert_eq!(
            bytes.len(),
            expected,
            "frame for {} nodes must be exactly {} bytes (header {} + 24*N), got {}",
            n,
            expected,
            HEADER_LEN,
            bytes.len()
        );

        // The 48-byte regression check: `9 + 48*N` mod 24 == 9 only when
        // N == 0; for N >= 1, `(9 + 48*N) - (9 + 24*N) == 24*N`, which is
        // a strict size mismatch caught by the equality above. We also
        // assert the modular form for documentation.
        if !positions.is_empty() {
            assert_eq!(
                bytes.len() % NODE_STRIDE,
                HEADER_LEN % NODE_STRIDE,
                "non-empty frame length must be header_len mod 24 — i.e. 9 — \
                 confirming no 24-byte analytics residue per node"
            );
        }

        // Preamble pinned every frame.
        assert_eq!(bytes[0], PREAMBLE);
    }
}

#[test]
fn no_flag_bits_in_id_field() {
    // GIVEN: A node whose id falls within the 26-bit range that the legacy
    // wire used to OR with type/visibility flag bits (bits 26-31).
    let positions = vec![node(0x03FF_FFFF, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)];

    // WHEN: Encoded.
    let bytes = encode_position_frame(&positions, 0);

    // THEN: The wire id is the full raw id, and bits 26-31 are all zero —
    // no agent-flag, no knowledge-flag, no PRIVATE_OPAQUE_FLAG, no
    // ontology-type bits. Per ADR-061 §D3, those discriminators have been
    // demoted to JSON init.
    let wire_id = read_u32_le(&bytes, 9);
    assert_eq!(
        wire_id, 0x03FF_FFFF,
        "id must be the raw value with no flag bits"
    );
    assert_eq!(
        wire_id & 0xFC00_0000,
        0,
        "bits 26-31 (legacy flags) must be zero on the wire"
    );
}

#[test]
fn decoder_rejects_bad_preamble() {
    // GIVEN: A buffer with the LEGACY V5 preamble (0x05) instead of 0x42.
    let mut bad = vec![0u8; HEADER_LEN + NODE_STRIDE];
    bad[0] = 0x05;

    // WHEN: We try to decode.
    let result = decode_position_frame(&bad);

    // THEN: Decoder rejects with an error mentioning the preamble.
    assert!(
        result.is_err(),
        "decoder must reject non-0x42 preamble; old V5 byte 0x05 is not a version dispatch"
    );
}

#[test]
fn decoder_rejects_legacy_v3_preamble() {
    // GIVEN: A buffer with the LEGACY V3 preamble (0x03).
    let mut bad = vec![0u8; HEADER_LEN + NODE_STRIDE];
    bad[0] = 0x03;

    // WHEN: We try to decode.
    let result = decode_position_frame(&bad);

    // THEN: Rejected. There is no V3 anymore.
    assert!(
        result.is_err(),
        "decoder must reject 0x03 (old V3 preamble) — the protocol is no longer versioned"
    );
}

#[test]
fn decoder_rejects_truncated_frame() {
    // GIVEN: A buffer shorter than the 9-byte header.
    let truncated = vec![0x42u8, 0, 0, 0]; // 4 bytes total
    let result = decode_position_frame(&truncated);
    assert!(result.is_err(), "decoder must reject sub-header buffer");
}

#[test]
fn decoder_rejects_partial_node_body() {
    // GIVEN: A header + 23 bytes (one byte short of a single node entry).
    let mut bad = vec![0u8; HEADER_LEN + NODE_STRIDE - 1];
    bad[0] = PREAMBLE;
    let result = decode_position_frame(&bad);
    assert!(
        result.is_err(),
        "decoder must reject body whose length is not a multiple of 24"
    );
}
