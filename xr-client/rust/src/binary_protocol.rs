//! gdext class wrapping the 0x42 graph position frame decoder.
//!
//! The full unified decoder lives in `crates/binary-protocol` (see PRD-007 +
//! ADR-061). Until that crate ships, `decode_position_frame` is a local
//! placeholder that parses the 24-byte/node layout described in
//! `docs/binary-protocol.md`. It is intentionally fallible (no `.unwrap()`)
//! so a malformed frame never panics the Quest render loop.

use bytes::Bytes;
use thiserror::Error;
use tracing::{debug, error, warn};

#[cfg(not(test))]
use godot::prelude::*;

pub const OPCODE_POSITION_FRAME: u8 = 0x42;
pub const NODE_RECORD_BYTES: usize = 28;
const HEADER_BYTES: usize = 1;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("frame too short: need {need} bytes, got {got}")]
    TooShort { need: usize, got: usize },
    #[error("unexpected opcode 0x{opcode:02x}, expected 0x{expected:02x}")]
    BadOpcode { opcode: u8, expected: u8 },
    #[error("payload length {len} not aligned to {NODE_RECORD_BYTES}-byte node record")]
    Misaligned { len: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeUpdate {
    pub node_id: u32,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
}

pub fn decode_position_frame(bytes: &[u8]) -> Result<Vec<NodeUpdate>, DecodeError> {
    if bytes.len() < HEADER_BYTES {
        return Err(DecodeError::TooShort {
            need: HEADER_BYTES,
            got: bytes.len(),
        });
    }
    if bytes[0] != OPCODE_POSITION_FRAME {
        return Err(DecodeError::BadOpcode {
            opcode: bytes[0],
            expected: OPCODE_POSITION_FRAME,
        });
    }
    let payload = &bytes[HEADER_BYTES..];
    if payload.len() % NODE_RECORD_BYTES != 0 {
        return Err(DecodeError::Misaligned { len: payload.len() });
    }
    let count = payload.len() / NODE_RECORD_BYTES;
    let mut out = Vec::with_capacity(count);
    for chunk in payload.chunks_exact(NODE_RECORD_BYTES) {
        out.push(parse_node_record(chunk));
    }
    Ok(out)
}

fn parse_node_record(bytes: &[u8]) -> NodeUpdate {
    let node_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let mut position = [0f32; 3];
    let mut velocity = [0f32; 3];
    for (i, off) in (4usize..16).step_by(4).enumerate() {
        position[i] = f32::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
        ]);
    }
    for (i, off) in (16usize..28).step_by(4).enumerate() {
        if off + 4 > bytes.len() {
            break;
        }
        velocity[i] = f32::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
        ]);
    }
    NodeUpdate {
        node_id,
        position,
        velocity,
    }
}

pub fn dispatch_opcode(bytes: &[u8]) -> Option<u8> {
    bytes.first().copied()
}

pub fn ingest_frame<F: FnMut(NodeUpdate)>(frame: Bytes, mut sink: F) {
    match decode_position_frame(&frame) {
        Ok(updates) => {
            debug!(count = updates.len(), "decoded position frame");
            for u in updates {
                sink(u);
            }
        }
        Err(DecodeError::BadOpcode { opcode, .. }) => {
            warn!(opcode, "ignoring non-position opcode on graph stream");
        }
        Err(e) => {
            error!(err = %e, "graph position frame decode failed");
        }
    }
}

#[cfg(not(test))]
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub struct BinaryProtocolClient {
    base: Base<RefCounted>,
}

#[cfg(not(test))]
#[godot_api]
impl BinaryProtocolClient {
    #[signal]
    fn position_updated(node_id: u32, position: Vector3, velocity: Vector3);

    #[func]
    fn create() -> Gd<Self> {
        Gd::from_init_fn(|base| Self { base })
    }

    #[func]
    fn ingest(&mut self, payload: PackedByteArray) {
        let bytes = Bytes::copy_from_slice(payload.as_slice());
        let mut emit_buf: Vec<NodeUpdate> = Vec::new();
        ingest_frame(bytes, |u| emit_buf.push(u));
        for u in emit_buf {
            self.base_mut().emit_signal(
                "position_updated",
                &[
                    Variant::from(u.node_id),
                    Variant::from(Vector3::new(u.position[0], u.position[1], u.position[2])),
                    Variant::from(Vector3::new(u.velocity[0], u.velocity[1], u.velocity[2])),
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn decodes_single_node_record() {
        let frame = build_frame(&[(7, [1.0, 2.0, 3.0], [0.1, 0.2, 0.3])]);
        let decoded = decode_position_frame(&frame).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].node_id, 7);
        assert_eq!(decoded[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(decoded[0].velocity, [0.1, 0.2, 0.3]);
    }

    #[test]
    fn rejects_bad_opcode() {
        let frame = vec![0x99, 0, 0, 0];
        assert!(matches!(
            decode_position_frame(&frame),
            Err(DecodeError::BadOpcode { .. })
        ));
    }

    #[test]
    fn rejects_misaligned_payload() {
        let mut frame = vec![OPCODE_POSITION_FRAME];
        frame.extend_from_slice(&[0u8; 27]);
        assert!(matches!(
            decode_position_frame(&frame),
            Err(DecodeError::Misaligned { .. })
        ));
    }

    #[test]
    fn rejects_too_short_for_header() {
        assert!(matches!(
            decode_position_frame(&[]),
            Err(DecodeError::TooShort { .. })
        ));
    }

    #[test]
    fn ingest_fires_callback_per_record() {
        let frame = build_frame(&[
            (1, [0.0; 3], [0.0; 3]),
            (2, [1.0, 0.0, 0.0], [0.0; 3]),
        ]);
        let mut count = 0usize;
        ingest_frame(Bytes::from(frame), |_| count += 1);
        assert_eq!(count, 2);
    }

    #[test]
    fn decodes_multi_node_frame() {
        let records = [
            (1u32, [1.0f32, 2.0, 3.0], [0.1f32, 0.2, 0.3]),
            (2, [4.0, 5.0, 6.0], [0.4, 0.5, 0.6]),
            (3, [7.0, 8.0, 9.0], [0.7, 0.8, 0.9]),
        ];
        let frame = build_frame(&records);
        let decoded = decode_position_frame(&frame).unwrap();
        assert_eq!(decoded.len(), 3);
        for (i, (id, pos, vel)) in records.iter().enumerate() {
            assert_eq!(decoded[i].node_id, *id);
            assert_eq!(decoded[i].position, *pos);
            assert_eq!(decoded[i].velocity, *vel);
        }
    }

    #[test]
    fn zero_node_frame_is_valid() {
        let frame = vec![OPCODE_POSITION_FRAME];
        let decoded = decode_position_frame(&frame).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn dispatch_opcode_returns_first_byte() {
        assert_eq!(dispatch_opcode(&[0x42, 0x00, 0x01]), Some(0x42));
        assert_eq!(dispatch_opcode(&[0x43]), Some(0x43));
        assert_eq!(dispatch_opcode(&[0x00]), Some(0x00));
        assert_eq!(dispatch_opcode(&[0xFF]), Some(0xFF));
    }

    #[test]
    fn dispatch_opcode_empty_returns_none() {
        assert_eq!(dispatch_opcode(&[]), None);
    }

    #[test]
    fn ingest_frame_bad_opcode_is_silent() {
        let frame = vec![0x99, 0x00, 0x00, 0x00];
        let mut count = 0usize;
        ingest_frame(Bytes::from(frame), |_| count += 1);
        assert_eq!(count, 0);
    }

    #[test]
    fn parse_preserves_negative_values() {
        let frame = build_frame(&[(
            42,
            [-1.5, -2.5, -3.5],
            [-0.1, -0.2, -0.3],
        )]);
        let decoded = decode_position_frame(&frame).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].node_id, 42);
        assert_eq!(decoded[0].position, [-1.5, -2.5, -3.5]);
        assert_eq!(decoded[0].velocity, [-0.1, -0.2, -0.3]);
    }

    #[test]
    fn max_u32_node_id() {
        let frame = build_frame(&[(u32::MAX, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0])]);
        let decoded = decode_position_frame(&frame).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].node_id, u32::MAX);
    }
}
