//! Property tests for the 0x42 binary position frame codec (PRD-QE-002 §4.5).
//!
//! Layout reminder (from `docs/binary-protocol.md`):
//! ```text
//! [u8 opcode = 0x42]
//! [{ u32_le node_id, f32_le[3] position, f32_le[3] velocity }; N]
//! ```
//! Each node record is 28 bytes. There is no explicit count field; the decoder
//! infers N from `(payload.len() / 28)`.

use proptest::prelude::*;
use visionclaw_xr_gdext::binary_protocol::{
    decode_position_frame, OPCODE_POSITION_FRAME,
};

/// Build a valid 0x42 frame from a slice of (node_id, position, velocity).
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

proptest! {
    /// PROP-BIN-1: any valid (u32, [f32;3], [f32;3]) tuple round-trips exactly
    /// through build_frame + decode_position_frame.
    #[test]
    fn single_record_round_trip(
        node_id in any::<u32>(),
        pos in proptest::array::uniform3(any::<f32>()),
        vel in proptest::array::uniform3(any::<f32>()),
    ) {
        let frame = build_frame(&[(node_id, pos, vel)]);
        let decoded = decode_position_frame(&frame).unwrap();
        prop_assert_eq!(decoded.len(), 1);
        prop_assert_eq!(decoded[0].node_id, node_id);
        // Use bitwise comparison to handle NaN/special floats: the raw bytes
        // must round-trip identically.
        for i in 0..3 {
            prop_assert_eq!(
                decoded[0].position[i].to_bits(),
                pos[i].to_bits(),
                "position[{i}] mismatch"
            );
            prop_assert_eq!(
                decoded[0].velocity[i].to_bits(),
                vel[i].to_bits(),
                "velocity[{i}] mismatch"
            );
        }
    }

    /// PROP-BIN-2: a frame with N records (0..50) always decodes to exactly N
    /// NodeUpdate entries.
    #[test]
    fn frame_count_preserved(n in 0usize..50) {
        let records: Vec<(u32, [f32; 3], [f32; 3])> = (0..n)
            .map(|i| (i as u32, [i as f32; 3], [0.0f32; 3]))
            .collect();
        let frame = build_frame(&records);
        let decoded = decode_position_frame(&frame).unwrap();
        prop_assert_eq!(decoded.len(), n);
    }

    /// PROP-BIN-3: multi-record round-trip preserves ordering and all fields.
    #[test]
    fn multi_record_round_trip(
        n in 1usize..20,
        seed in any::<u32>(),
    ) {
        let records: Vec<(u32, [f32; 3], [f32; 3])> = (0..n)
            .map(|i| {
                let id = seed.wrapping_add(i as u32);
                let f = id as f32;
                (id, [f, f + 1.0, f + 2.0], [f * 0.1, f * 0.2, f * 0.3])
            })
            .collect();
        let frame = build_frame(&records);
        let decoded = decode_position_frame(&frame).unwrap();
        prop_assert_eq!(decoded.len(), n);
        for (i, (id, pos, vel)) in records.iter().enumerate() {
            prop_assert_eq!(decoded[i].node_id, *id);
            prop_assert_eq!(decoded[i].position, *pos);
            prop_assert_eq!(decoded[i].velocity, *vel);
        }
    }
}
