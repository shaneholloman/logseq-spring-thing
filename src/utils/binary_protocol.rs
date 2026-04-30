#![allow(dead_code)]
//! # The binary protocol
//!
//! Per-physics-tick stream of `(node_id, position, velocity)` tuples from the
//! GPU force-compute path to subscribed WebSocket clients. There is exactly
//! ONE wire format. There is no version negotiation, no flag-bit
//! discriminator, no analytics column. Sticky GPU outputs (cluster_id,
//! community_id, anomaly_score, sssp_distance, sssp_parent) ride a separate
//! `analytics_update` message at recompute cadence — see
//! `src/actors/messages/analytics_messages.rs`.
//!
//! ## Frame layout
//!
//! ```text
//! [u8  preamble        = 0x42]   ← fixed sanity byte; NOT a version dispatch
//! [u64 broadcast_seq_LE]         ← backpressure-ack key
//! [N × Node]
//!
//! Node (28 bytes — id + position + velocity):
//!   [u32 id_LE]
//!   [f32 x_LE] [f32 y_LE] [f32 z_LE]
//!   [f32 vx_LE] [f32 vy_LE] [f32 vz_LE]
//! ```
//!
//! See PRD-007, ADR-061, and `docs/ddd-binary-protocol-context.md`.
//!
//! ## Note on byte counts
//!
//! PRD-007 §4.1 and ADR-061 §D1 describe the per-node entry as "24 bytes"
//! while listing seven 4-byte fields (id + 6 floats). The arithmetic
//! resolves to 28 bytes; the prose figure is a documented spec error
//! corrected here. The wire ALWAYS carries the raw u32 id followed by
//! position + velocity = 28 bytes per node.

use crate::models::constraints::{AdvancedParams, Constraint};
use crate::utils::socket_flow_messages::BinaryNodeData;
use log::{debug, trace};
use serde::{Deserialize, Serialize};
use serde_json;

// =============================================================================
// ARCHITECTURE LOCK — ONE WIRE, NO VERSIONING
// =============================================================================
// The wire is LITERAL-ONLY: every broadcast is a full snapshot of every
// node's position + velocity, addressed by raw u32 id.
//
// Why no delta? This graph is a force-directed spring network. Every node
// moves on every physics tick. Delta encoding saves nothing while adding
// stale-position drift on reconnect, silent drops on threshold filtering,
// and parallel decoders. The bandwidth lever is BROADCAST CADENCE, not
// payload encoding.
//
// Why no version byte? See ADR-061 §D4. The preamble byte 0x42 is a
// permanent sanity check; it does not select a code path. If the protocol
// ever needs to evolve, it does so via a new endpoint, not a version byte.
//
// Why no analytics columns? See ADR-061 §D2. cluster_id, community_id,
// anomaly_score, sssp_distance, sssp_parent ride a separate
// `analytics_update` message at recompute cadence (~0.1–1 Hz), not the
// 60 Hz position stream.
// =============================================================================

/// Fixed sanity byte that prefixes every position frame. NOT a version field.
/// If the protocol ever changes, it changes via a new endpoint, not by
/// switching this byte.
pub const BINARY_PROTOCOL_PREAMBLE: u8 = 0x42;

/// Bytes per node entry on the wire: `id(4) + pos(12) + vel(12) = 28`.
/// (PRD-007 §4.1 / ADR-061 §D1 list this as "24" — that is a documented
/// arithmetic error in the spec text; the field list is authoritative.)
pub const NODE_ENTRY_SIZE: usize = 28;

/// Bytes in the frame header: `preamble(1) + broadcast_sequence(8) = 9`.
pub const FRAME_HEADER_SIZE: usize = 1 + 8;

// Safety limits for decode functions
const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const MAX_NODE_COUNT: usize = 100_000;

/// ADR-050: `true` iff the sovereign-schema feature flag is enabled.
///
/// This gates Neo4j sovereign-index creation in `neo4j_adapter`. It is
/// kept here as the single home for the flag check; it is unrelated to
/// the wire protocol (per ADR-061 §D3, visibility is enforced by
/// `ClientCoordinator::broadcast_with_filter` dropping invisible nodes
/// from the per-frame stream, not by setting bits on the wire id).
pub fn sovereign_schema_enabled() -> bool {
    std::env::var("SOVEREIGN_SCHEMA")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(false)
}

// =============================================================================
// THE encoder
// =============================================================================

/// Encode a position frame: preamble + broadcast_sequence + N×28-byte nodes.
///
/// `positions` is the (id, BinaryNodeData) list for nodes the receiving
/// client may see. Per-client visibility is enforced upstream — by the time
/// we reach this function, every node in `positions` is intended for the
/// recipient.
///
/// Output size: `9 + 28 × positions.len()`.
pub fn encode_position_frame(
    positions: &[(u32, BinaryNodeData)],
    broadcast_sequence: u64,
) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(FRAME_HEADER_SIZE + positions.len() * NODE_ENTRY_SIZE);

    buffer.push(BINARY_PROTOCOL_PREAMBLE);
    buffer.extend_from_slice(&broadcast_sequence.to_le_bytes());

    for (id, node) in positions {
        buffer.extend_from_slice(&id.to_le_bytes());
        buffer.extend_from_slice(&node.x.to_le_bytes());
        buffer.extend_from_slice(&node.y.to_le_bytes());
        buffer.extend_from_slice(&node.z.to_le_bytes());
        buffer.extend_from_slice(&node.vx.to_le_bytes());
        buffer.extend_from_slice(&node.vy.to_le_bytes());
        buffer.extend_from_slice(&node.vz.to_le_bytes());
    }

    if !positions.is_empty() {
        trace!(
            "encode_position_frame: {} nodes, seq={}, {} bytes",
            positions.len(),
            broadcast_sequence,
            buffer.len()
        );
    }

    buffer
}

/// Decode a position frame produced by `encode_position_frame`.
///
/// Returns `(broadcast_sequence, [(id, BinaryNodeData), ...])`.
pub fn decode_position_frame(
    bytes: &[u8],
) -> Result<(u64, Vec<(u32, BinaryNodeData)>), String> {
    if bytes.len() > MAX_PAYLOAD_SIZE {
        return Err(format!(
            "Payload size {} exceeds maximum {}",
            bytes.len(),
            MAX_PAYLOAD_SIZE
        ));
    }
    if bytes.len() < FRAME_HEADER_SIZE {
        return Err(format!(
            "Frame too small: {} bytes, need at least {}",
            bytes.len(),
            FRAME_HEADER_SIZE
        ));
    }
    if bytes[0] != BINARY_PROTOCOL_PREAMBLE {
        return Err(format!(
            "Bad preamble byte: 0x{:02X} (expected 0x{:02X})",
            bytes[0], BINARY_PROTOCOL_PREAMBLE
        ));
    }

    let seq = u64::from_le_bytes([
        bytes[1], bytes[2], bytes[3], bytes[4],
        bytes[5], bytes[6], bytes[7], bytes[8],
    ]);

    let body = &bytes[FRAME_HEADER_SIZE..];
    if body.len() % NODE_ENTRY_SIZE != 0 {
        return Err(format!(
            "Body size {} is not a multiple of node entry size {}",
            body.len(),
            NODE_ENTRY_SIZE
        ));
    }

    let count = body.len() / NODE_ENTRY_SIZE;
    if count > MAX_NODE_COUNT {
        return Err(format!(
            "Node count {} exceeds maximum {}",
            count, MAX_NODE_COUNT
        ));
    }

    let mut out = Vec::with_capacity(count);
    for chunk in body.chunks_exact(NODE_ENTRY_SIZE) {
        let id = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let x = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        let y = f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
        let z = f32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]);
        let vx = f32::from_le_bytes([chunk[16], chunk[17], chunk[18], chunk[19]]);
        let vy = f32::from_le_bytes([chunk[20], chunk[21], chunk[22], chunk[23]]);
        let vz = f32::from_le_bytes([chunk[24], chunk[25], chunk[26], chunk[27]]);
        let node = BinaryNodeData { node_id: id, x, y, z, vx, vy, vz };
        out.push((id, node));
    }

    debug!(
        "decode_position_frame: seq={}, {} nodes, {} bytes",
        seq, out.len(), bytes.len()
    );
    Ok((seq, out))
}

/// Compute the on-wire size of a position frame for the given updates.
pub fn calculate_message_size(updates: &[(u32, BinaryNodeData)]) -> usize {
    FRAME_HEADER_SIZE + updates.len() * NODE_ENTRY_SIZE
}

// =============================================================================
// AGENT ACTION EVENTS (Protocol 0x23) - Ephemeral Connection Visualization
// =============================================================================

/// Action types for agent-to-data interactions
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentActionType {
    Query = 0,      // Agent querying data node (blue)
    Update = 1,     // Agent updating data node (yellow)
    Create = 2,     // Agent creating data node (green)
    Delete = 3,     // Agent deleting data node (red)
    Link = 4,       // Agent linking nodes (purple)
    Transform = 5,  // Agent transforming data (cyan)
}

impl From<u8> for AgentActionType {
    fn from(value: u8) -> Self {
        match value {
            0 => AgentActionType::Query,
            1 => AgentActionType::Update,
            2 => AgentActionType::Create,
            3 => AgentActionType::Delete,
            4 => AgentActionType::Link,
            5 => AgentActionType::Transform,
            _ => AgentActionType::Query, // Default
        }
    }
}

/// Agent action event for visualization (15 bytes header + variable payload)
/// Used to render ephemeral connections between agent nodes and data nodes
#[repr(C)]
#[derive(Debug, Clone)]
pub struct AgentActionEvent {
    pub source_agent_id: u32,   // 4 bytes - ID of the acting agent
    pub target_node_id: u32,    // 4 bytes - ID of the target data node
    pub action_type: u8,        // 1 byte - AgentActionType
    pub timestamp: u32,         // 4 bytes - Event timestamp (ms)
    pub duration_ms: u16,       // 2 bytes - Animation duration hint
    pub payload: Vec<u8>,       // Variable - Optional metadata
}

// Wire format size (fixed header only, payload is variable)
const AGENT_ACTION_HEADER_SIZE: usize = 15;

impl AgentActionEvent {
    /// Create a new agent action event
    pub fn new(
        source_agent_id: u32,
        target_node_id: u32,
        action_type: AgentActionType,
        duration_ms: u16,
    ) -> Self {
        Self {
            source_agent_id,
            target_node_id,
            action_type: action_type as u8,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| (d.as_millis() % u32::MAX as u128) as u32)
                .unwrap_or(0),
            duration_ms,
            payload: Vec::new(),
        }
    }

    /// Encode to wire format
    pub fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(1 + AGENT_ACTION_HEADER_SIZE + self.payload.len());

        // Message type header
        buffer.push(MessageType::AgentAction as u8);

        // Fixed header (15 bytes)
        buffer.extend_from_slice(&self.source_agent_id.to_le_bytes());
        buffer.extend_from_slice(&self.target_node_id.to_le_bytes());
        buffer.push(self.action_type);
        buffer.extend_from_slice(&self.timestamp.to_le_bytes());
        buffer.extend_from_slice(&self.duration_ms.to_le_bytes());

        // Variable payload
        buffer.extend_from_slice(&self.payload);

        buffer
    }

    /// Decode from wire format (excludes message type byte)
    pub fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() < AGENT_ACTION_HEADER_SIZE {
            return Err(format!(
                "AgentActionEvent data too small: {} < {}",
                data.len(),
                AGENT_ACTION_HEADER_SIZE
            ));
        }

        let source_agent_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let target_node_id = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let action_type = data[8];
        let timestamp = u32::from_le_bytes([data[9], data[10], data[11], data[12]]);
        let duration_ms = u16::from_le_bytes([data[13], data[14]]);
        let payload = if data.len() > AGENT_ACTION_HEADER_SIZE {
            data[AGENT_ACTION_HEADER_SIZE..].to_vec()
        } else {
            Vec::new()
        };

        Ok(Self {
            source_agent_id,
            target_node_id,
            action_type,
            timestamp,
            duration_ms,
            payload,
        })
    }

    /// Get the action type as enum
    pub fn get_action_type(&self) -> AgentActionType {
        AgentActionType::from(self.action_type)
    }
}

/// Batch encode multiple agent action events
pub fn encode_agent_actions(events: &[AgentActionEvent]) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(
        1 + events.len() * (AGENT_ACTION_HEADER_SIZE + 16) // Estimate with avg payload
    );

    // Message type
    buffer.push(MessageType::AgentAction as u8);

    // Event count (u16)
    buffer.extend_from_slice(&(events.len() as u16).to_le_bytes());

    // Each event (length-prefixed)
    for event in events {
        let event_data = event.encode();
        let event_len = (event_data.len() - 1) as u16; // Exclude msg type byte
        buffer.extend_from_slice(&event_len.to_le_bytes());
        buffer.extend_from_slice(&event_data[1..]); // Skip msg type byte
    }

    buffer
}

/// Decode batch of agent action events
pub fn decode_agent_actions(data: &[u8]) -> Result<Vec<AgentActionEvent>, String> {
    if data.len() < 2 {
        return Err("AgentAction batch data too small".to_string());
    }

    if data.len() > MAX_PAYLOAD_SIZE {
        return Err(format!(
            "AgentAction payload size {} exceeds maximum {}",
            data.len(),
            MAX_PAYLOAD_SIZE
        ));
    }

    let event_count = u16::from_le_bytes([data[0], data[1]]) as usize;
    let mut events = Vec::with_capacity(event_count);
    let mut offset = 2;

    for _ in 0..event_count {
        if offset + 2 > data.len() {
            return Err("Truncated event length".to_string());
        }

        let event_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        if offset + event_len > data.len() {
            return Err("Truncated event data".to_string());
        }

        let event = AgentActionEvent::decode(&data[offset..offset + event_len])?;
        events.push(event);
        offset += event_len;
    }

    Ok(events)
}

// Control frame structures for constraint and parameter updates

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ControlFrame {

    #[serde(rename = "constraints_update")]
    ConstraintsUpdate {
        version: u32,
        constraints: Vec<Constraint>,
        #[serde(skip_serializing_if = "Option::is_none")]
        advanced_params: Option<AdvancedParams>,
    },


    #[serde(rename = "lens_request")]
    LensRequest {
        lens_type: String,
        parameters: serde_json::Value,
    },


    #[serde(rename = "control_ack")]
    ControlAck {
        frame_type: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },


    #[serde(rename = "physics_params")]
    PhysicsParams { advanced_params: AdvancedParams },


    #[serde(rename = "preset_request")]
    PresetRequest { preset_name: String },
}

impl ControlFrame {

    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }


    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }


    pub fn constraints_update(
        constraints: Vec<Constraint>,
        params: Option<AdvancedParams>,
    ) -> Self {
        ControlFrame::ConstraintsUpdate {
            version: 1,
            constraints,
            advanced_params: params,
        }
    }


    pub fn ack(frame_type: &str, success: bool, message: Option<String>) -> Self {
        ControlFrame::ControlAck {
            frame_type: frame_type.to_string(),
            success,
            message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageType {

    /// Binary position frame: `preamble(0x42) + broadcast_seq(8) + N×28B nodes`.
    /// See `encode_position_frame`.
    BinaryPositions = 0,

    VoiceData = 0x02,

    ControlFrame = 0x03,

    /// Client acknowledgement of position broadcast (backpressure).
    /// Enables true end-to-end flow control vs queue-only confirmation.
    BroadcastAck = 0x34,

    /// Agent action event for visualization of agent-to-data interactions
    /// Used for ephemeral connection visualization in 3D space
    AgentAction = 0x23,
}

/// WebSocket message types for voice and acknowledgements
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    VoiceData { audio: Vec<u8> },

    /// Client acknowledgement of position broadcast for backpressure flow control
    BroadcastAck {
        sequence_id: u64,    // Correlates with server broadcast sequence
        nodes_received: u32, // Number of nodes client processed
        timestamp: u64,      // Client receive timestamp (ms since epoch)
    },
}

#[derive(Debug)]
pub enum ProtocolError {
    InvalidMessageType(u8),
    InvalidPayloadSize(String),
    EncodingError(String),
    DecodingError(String),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::InvalidMessageType(t) => write!(f, "Invalid message type: {}", t),
            ProtocolError::InvalidPayloadSize(s) => write!(f, "Invalid payload size: {}", s),
            ProtocolError::EncodingError(s) => write!(f, "Encoding error: {}", s),
            ProtocolError::DecodingError(s) => write!(f, "Decoding error: {}", s),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Binary protocol utilities for voice and acknowledgement messages
pub struct BinaryProtocol;

impl BinaryProtocol {
    /// Decode incoming WebSocket messages (voice data and acknowledgements)
    pub fn decode_message(data: &[u8]) -> Result<Message, ProtocolError> {
        if data.is_empty() {
            return Err(ProtocolError::DecodingError("Empty message".to_string()));
        }

        if data.len() > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::InvalidPayloadSize(format!(
                "Message size {} exceeds maximum {}",
                data.len(),
                MAX_PAYLOAD_SIZE
            )));
        }

        let message_type = data[0];

        match message_type {
            0x02 => Self::decode_voice_data(&data[1..]),
            0x34 => Self::decode_broadcast_ack(&data[1..]),
            _ => Err(ProtocolError::InvalidMessageType(message_type)),
        }
    }

    fn decode_voice_data(data: &[u8]) -> Result<Message, ProtocolError> {
        Ok(Message::VoiceData {
            audio: data.to_vec(),
        })
    }

    /// Decode client broadcast acknowledgement for backpressure flow control
    /// Payload: 8 bytes sequence_id + 4 bytes nodes_received + 8 bytes timestamp = 20 bytes
    fn decode_broadcast_ack(data: &[u8]) -> Result<Message, ProtocolError> {
        if data.len() < 20 {
            return Err(ProtocolError::InvalidPayloadSize(format!(
                "BroadcastAck payload size {} is less than required 20 bytes",
                data.len()
            )));
        }

        // Decode sequence_id (u64, little-endian)
        let sequence_id = u64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);

        // Decode nodes_received (u32, little-endian)
        let nodes_received = u32::from_le_bytes([
            data[8], data[9], data[10], data[11],
        ]);

        // Decode timestamp (u64, little-endian)
        let timestamp = u64::from_le_bytes([
            data[12], data[13], data[14], data[15],
            data[16], data[17], data[18], data[19],
        ]);

        Ok(Message::BroadcastAck {
            sequence_id,
            nodes_received,
            timestamp,
        })
    }



    pub fn encode_voice_data(audio: &[u8]) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(1 + audio.len());
        buffer.push(MessageType::VoiceData as u8);
        buffer.extend_from_slice(audio);
        buffer
    }
}

pub struct MultiplexedMessage {
    pub msg_type: MessageType,
    pub data: Vec<u8>,
}

impl MultiplexedMessage {

    /// Build a binary-positions multiplexed message from a list of nodes.
    /// Uses sequence number 0 (intended for tests / non-broadcast paths).
    pub fn positions(node_data: &[(u32, BinaryNodeData)]) -> Self {
        Self {
            msg_type: MessageType::BinaryPositions,
            data: encode_position_frame(node_data, 0),
        }
    }


    pub fn control(frame: &ControlFrame) -> Result<Self, serde_json::Error> {
        Ok(Self {
            msg_type: MessageType::ControlFrame,
            data: frame.to_bytes()?,
        })
    }


    pub fn encode(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(1 + self.data.len());
        result.push(self.msg_type as u8);
        result.extend_from_slice(&self.data);
        result
    }

    /// Decode multiplexed message from wire format
    pub fn decode(data: &[u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("Empty message".to_string());
        }

        let msg_type = match data[0] {
            0 => MessageType::BinaryPositions,
            0x02 => MessageType::VoiceData,
            0x03 => MessageType::ControlFrame,
            0x23 => MessageType::AgentAction,
            0x34 => MessageType::BroadcastAck,
            t => return Err(format!("Unknown message type: {}", t)),
        };

        Ok(Self {
            msg_type,
            data: data[1..].to_vec(),
        })
    }
}
