#![allow(dead_code)]
use crate::models::constraints::{AdvancedParams, Constraint};
use crate::types::vec3::Vec3Data;
use crate::utils::socket_flow_messages::BinaryNodeData;
use log::{debug, trace};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;

// Protocol versions for wire format (V1 REMOVED - no backward compatibility)
// PROTOCOL_V2 (value: 2) removed — server no longer sends or decodes V2 frames
const PROTOCOL_V3: u8 = 3; // Analytics extension protocol (P0-4) - CURRENT
const PROTOCOL_V4: u8 = 4; // Delta encoding protocol

// Node type flag constants for u32 (server-side)
const AGENT_NODE_FLAG: u32 = 0x80000000;
const KNOWLEDGE_NODE_FLAG: u32 = 0x40000000;

/// ADR-050 private-opacity flag — bit 29 of the wire id.
///
/// When set, the client MUST render the node without label or metadata; the
/// node is private-sovereign and the consuming client is not its owner. The
/// server strips label/metadata at serialisation time, so the flag is a
/// redundant belt-and-braces signal the client can also use to style the
/// placeholder locally (e.g. render as "(private)").
pub const PRIVATE_OPAQUE_FLAG: u32 = 0x20000000;

// Ontology node type flags (bits 26-28, only valid when GraphType::Ontology)
const ONTOLOGY_TYPE_MASK: u32 = 0x1C000000;
const ONTOLOGY_CLASS_FLAG: u32 = 0x04000000;
const ONTOLOGY_INDIVIDUAL_FLAG: u32 = 0x08000000;
const ONTOLOGY_PROPERTY_FLAG: u32 = 0x10000000;

// Node ID mask: bits 0-25 only (excludes bits 26-31 for all flags).
// Bit layout of the 32-bit wire id:
//   31 AGENT | 30 KNOWLEDGE | 29 PRIVATE_OPAQUE | 28-26 ONTOLOGY_TYPE | 25-0 ID
// Supports node IDs: 0 to 67,108,863 (2^26 - 1)
const NODE_ID_MASK: u32 = 0x03FFFFFF;

/// Strip bit 29 from a wire-flagged id and return the base (type-flagged) id.
#[inline]
pub fn node_id_base(raw: u32) -> u32 { raw & !PRIVATE_OPAQUE_FLAG }

/// Did the server mark this wire id as private-opaque to the consuming client?
#[inline]
pub fn is_private_opaque(raw: u32) -> bool {
    raw & PRIVATE_OPAQUE_FLAG != 0
}

/// OR the private-opacity flag onto `base` when `is_private == true`.
/// Does not clear other flags (agent / knowledge / ontology type bits stay).
#[inline]
pub fn encode_node_id(base: u32, is_private: bool) -> u32 {
    if is_private { base | PRIVATE_OPAQUE_FLAG } else { base }
}

// V1 wire format constants REMOVED - caused node ID truncation bugs
// V2+ uses full u32 IDs with no truncation

// V2 wire flag constants removed — identical to AGENT_NODE_FLAG / KNOWLEDGE_NODE_FLAG / NODE_ID_MASK

// WireNodeDataItemV1 REMOVED - V1 protocol no longer supported
// WireNodeDataItemV2 REMOVED - V2 protocol no longer supported (was 36 bytes per node)

/// Wire format V3 - 48 bytes per node (P0-4 Analytics Extension)
/// Adds clustering, anomaly detection, and community detection
pub struct WireNodeDataItemV3 {
    pub id: u32,
    pub position: Vec3Data,
    pub velocity: Vec3Data,
    pub sssp_distance: f32,
    pub sssp_parent: i32,
    pub cluster_id: u32,
    pub anomaly_score: f32,
    pub community_id: u32,
}

// Backwards compatibility alias - now defaults to V3
pub type WireNodeDataItem = WireNodeDataItemV3;

// ============================================================================
// DELTA ENCODING (Protocol V4) - P1-3 Feature
// ============================================================================

/// Delta-encoded position update (20 bytes per changed node)
/// Used in frames 1-59 to send only changes from previous frame
/// Achieves 60-80% bandwidth reduction compared to full state updates
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DeltaNodeData {
    pub id: u32,            // 4 bytes - node ID with flags
    pub change_flags: u8,   // 1 byte - bits indicate which fields changed
    pub _padding: [u8; 3],  // 3 bytes - alignment padding
    pub dx: i16,            // 2 bytes - delta position x (scaled)
    pub dy: i16,            // 2 bytes - delta position y (scaled)
    pub dz: i16,            // 2 bytes - delta position z (scaled)
    pub dvx: i16,           // 2 bytes - delta velocity x (scaled)
    pub dvy: i16,           // 2 bytes - delta velocity y (scaled)
    pub dvz: i16,           // 2 bytes - delta velocity z (scaled)
}

// Change flags for delta encoding
const DELTA_POSITION_CHANGED: u8 = 0x01;
const DELTA_VELOCITY_CHANGED: u8 = 0x02;
const DELTA_ALL_CHANGED: u8 = DELTA_POSITION_CHANGED | DELTA_VELOCITY_CHANGED;

// Delta encoding constants
const DELTA_SCALE_FACTOR: f32 = 100.0; // Scale factor for i16 precision
const DELTA_ITEM_SIZE: usize = 20;     // Size of DeltaNodeData in bytes: 4(id) + 1(flags) + 3(padding) + 6*2(deltas) = 20
const DELTA_RESYNC_INTERVAL: u64 = 60; // Full state every 60 frames

// Safety limits for decode functions
const MAX_PAYLOAD_SIZE: usize = 10 * 1024 * 1024; // 10 MB
const MAX_NODE_COUNT: usize = 100_000;

// Constants for wire format sizes
const WIRE_VEC3_SIZE: usize = 12;
const WIRE_F32_SIZE: usize = 4;
const WIRE_I32_SIZE: usize = 4;
const WIRE_U32_SIZE: usize = 4;
const WIRE_ID_SIZE: usize = 4;
// V3: id(4) + pos(12) + vel(12) + sssp_dist(4) + sssp_parent(4) + cluster_id(4) + anomaly_score(4) + community_id(4) = 48
const WIRE_V3_ITEM_SIZE: usize =
    WIRE_ID_SIZE + WIRE_VEC3_SIZE + WIRE_VEC3_SIZE + WIRE_F32_SIZE + WIRE_I32_SIZE +
    WIRE_U32_SIZE + WIRE_F32_SIZE + WIRE_U32_SIZE;
const WIRE_ITEM_SIZE: usize = WIRE_V3_ITEM_SIZE;

// Binary format (explicit):
//
// PROTOCOL V3 (CURRENT - P0-4 Analytics Extension):
// - Wire format sent to client (48 bytes total):
//   - Node Index: 4 bytes (u32) - Bits 30-31 for agent/knowledge, bits 26-28 for ontology, bits 0-25 for ID
//   - Position: 3 × 4 bytes = 12 bytes
//   - Velocity: 3 × 4 bytes = 12 bytes
//   - SSSP Distance: 4 bytes (f32)
//   - SSSP Parent: 4 bytes (i32)
//   - Cluster ID: 4 bytes (u32) - K-means cluster assignment
//   - Anomaly Score: 4 bytes (f32) - LOF anomaly score (0.0-1.0)
//   - Community ID: 4 bytes (u32) - Louvain community assignment
// Total: 48 bytes per node
// Supports node IDs: 0 to 67,108,863 (2^26 - 1)
//
// PROTOCOL V2 REMOVED — was 36 bytes/node (no analytics), server no longer sends or decodes V2
//
// PROTOCOL V1 REMOVED - Had node ID truncation bug (IDs > 16383 were corrupted)
//
// - Server format (BinaryNodeData - 28 bytes total):
//   - Node ID: 4 bytes (u32)
//   - Position: 3 × 4 bytes = 12 bytes
//   - Velocity: 3 × 4 bytes = 12 bytes
// Total: 28 bytes per node
//
// Node Type Flags:
// - V2/V3: Bits 30-31 of u32 ID (Bit 31 = Agent, Bit 30 = Knowledge)
// - V2/V3: Bits 26-28 of u32 ID for Ontology types (Bit 26 = Class, Bit 27 = Individual, Bit 28 = Property)
// This allows the client to distinguish between different node types for visualization.

pub fn set_agent_flag(node_id: u32) -> u32 {
    debug_assert!(
        node_id <= NODE_ID_MASK,
        "Node ID {} (0x{:08X}) exceeds 26-bit limit (max {}). Use compact wire IDs.",
        node_id, node_id, NODE_ID_MASK
    );
    (node_id & NODE_ID_MASK) | AGENT_NODE_FLAG
}

pub fn set_knowledge_flag(node_id: u32) -> u32 {
    debug_assert!(
        node_id <= NODE_ID_MASK,
        "Node ID {} (0x{:08X}) exceeds 26-bit limit (max {}). Use compact wire IDs.",
        node_id, node_id, NODE_ID_MASK
    );
    (node_id & NODE_ID_MASK) | KNOWLEDGE_NODE_FLAG
}

pub fn clear_agent_flag(node_id: u32) -> u32 {
    node_id & !AGENT_NODE_FLAG
}

pub fn clear_all_flags(node_id: u32) -> u32 {
    node_id & NODE_ID_MASK
}

pub fn is_agent_node(node_id: u32) -> bool {
    (node_id & AGENT_NODE_FLAG) != 0
}

pub fn is_knowledge_node(node_id: u32) -> bool {
    (node_id & KNOWLEDGE_NODE_FLAG) != 0
}

pub fn get_actual_node_id(node_id: u32) -> u32 {
    node_id & NODE_ID_MASK
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeType {
    Knowledge,
    Agent,
    OntologyClass,
    OntologyIndividual,
    OntologyProperty,
    Unknown,
}

pub fn get_node_type(node_id: u32) -> NodeType {
    if is_agent_node(node_id) {
        NodeType::Agent
    } else if is_knowledge_node(node_id) {
        NodeType::Knowledge
    } else if is_ontology_class(node_id) {
        NodeType::OntologyClass
    } else if is_ontology_individual(node_id) {
        NodeType::OntologyIndividual
    } else if is_ontology_property(node_id) {
        NodeType::OntologyProperty
    } else {
        NodeType::Unknown
    }
}

pub fn set_ontology_class_flag(node_id: u32) -> u32 {
    debug_assert!(
        node_id <= NODE_ID_MASK,
        "Node ID {} (0x{:08X}) exceeds 26-bit limit (max {}). Use compact wire IDs.",
        node_id, node_id, NODE_ID_MASK
    );
    (node_id & NODE_ID_MASK) | ONTOLOGY_CLASS_FLAG
}

pub fn set_ontology_individual_flag(node_id: u32) -> u32 {
    debug_assert!(
        node_id <= NODE_ID_MASK,
        "Node ID {} (0x{:08X}) exceeds 26-bit limit (max {}). Use compact wire IDs.",
        node_id, node_id, NODE_ID_MASK
    );
    (node_id & NODE_ID_MASK) | ONTOLOGY_INDIVIDUAL_FLAG
}

pub fn set_ontology_property_flag(node_id: u32) -> u32 {
    debug_assert!(
        node_id <= NODE_ID_MASK,
        "Node ID {} (0x{:08X}) exceeds 26-bit limit (max {}). Use compact wire IDs.",
        node_id, node_id, NODE_ID_MASK
    );
    (node_id & NODE_ID_MASK) | ONTOLOGY_PROPERTY_FLAG
}

pub fn is_ontology_class(node_id: u32) -> bool {
    (node_id & ONTOLOGY_TYPE_MASK) == ONTOLOGY_CLASS_FLAG
}

pub fn is_ontology_individual(node_id: u32) -> bool {
    (node_id & ONTOLOGY_TYPE_MASK) == ONTOLOGY_INDIVIDUAL_FLAG
}

pub fn is_ontology_property(node_id: u32) -> bool {
    (node_id & ONTOLOGY_TYPE_MASK) == ONTOLOGY_PROPERTY_FLAG
}

pub fn is_ontology_node(node_id: u32) -> bool {
    (node_id & ONTOLOGY_TYPE_MASK) != 0
}

// to_wire_id_v1 and from_wire_id_v1 REMOVED - V1 protocol no longer supported
// Use to_wire_id_v2/from_wire_id_v2 for full 32-bit node ID support

pub fn to_wire_id_v2(node_id: u32) -> u32 {
    
    
    node_id
}

pub fn from_wire_id_v2(wire_id: u32) -> u32 {
    
    wire_id
}

// Backwards compatibility aliases - use V2 by default
pub fn to_wire_id(node_id: u32) -> u32 {
    to_wire_id_v2(node_id)
}

pub fn from_wire_id(wire_id: u32) -> u32 {
    from_wire_id_v2(wire_id)
}

/// Convert BinaryNodeData to wire format V3
impl BinaryNodeData {
    pub fn to_wire_format(&self, node_id: u32) -> WireNodeDataItem {
        self.to_wire_format_with_data(node_id, None, None)
    }

    /// Convert to wire format V3 with optional SSSP and analytics data.
    /// `sssp`: (distance, parent_id). Defaults to (INFINITY, -1).
    /// `analytics`: (cluster_id, anomaly_score, community_id). Defaults to (0, 0.0, 0).
    pub fn to_wire_format_with_data(
        &self,
        node_id: u32,
        sssp: Option<(f32, i32)>,
        analytics: Option<(u32, f32, u32)>,
    ) -> WireNodeDataItem {
        let (sssp_distance, sssp_parent) = sssp.unwrap_or((f32::INFINITY, -1));
        let (cluster_id, anomaly_score, community_id) = analytics.unwrap_or((0, 0.0, 0));
        WireNodeDataItem {
            id: to_wire_id(node_id),
            position: self.position(),
            velocity: self.velocity(),
            sssp_distance,
            sssp_parent,
            cluster_id,
            anomaly_score,
            community_id,
        }
    }
}

/// ADR-050: `true` iff the sovereign-schema feature flag is enabled for this
/// process. Gates:
///   - Neo4j sovereign indexes being created at startup.
///   - Bit 29 (`PRIVATE_OPAQUE_FLAG`) being ORed into wire ids.
///
/// When unset/false, the new `visibility`/`owner_pubkey`/`opaque_id`/`pod_url`
/// fields are still written through to Neo4j rows (so data gathered pre-flip
/// is not lost), but no privacy enforcement happens on the wire.
pub fn sovereign_schema_enabled() -> bool {
    std::env::var("SOVEREIGN_SCHEMA")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on"))
        .unwrap_or(false)
}

/// ADR-037: Convenience wrapper — calls encode_positions_v3 without SSSP or analytics.
pub fn encode_node_data_extended(
    nodes: &[(u32, BinaryNodeData)],
    agent_node_ids: &[u32],
    knowledge_node_ids: &[u32],
    ontology_class_ids: &[u32],
    ontology_individual_ids: &[u32],
    ontology_property_ids: &[u32],
) -> Vec<u8> {
    encode_positions_v3(
        nodes,
        agent_node_ids,
        knowledge_node_ids,
        ontology_class_ids,
        ontology_individual_ids,
        ontology_property_ids,
        None,
        None,
    )
}

/// ADR-037: Single canonical V3 encoder.
///
/// Encodes node positions into V3 binary frames (48 bytes/node) with type flags,
/// optional SSSP distances, and optional analytics data.
///
/// Type classification: if node IDs are pre-flagged (from fetch_nodes()), pass empty
/// type arrays to avoid double-flagging. If unflagged, pass type arrays.
///
/// `sssp_data`: maps node_id -> (distance, parent_id). Default: (INFINITY, -1).
/// `analytics_data`: maps node_id -> (cluster_id, anomaly_score, community_id). Default: (0, 0.0, 0).
pub fn encode_positions_v3(
    nodes: &[(u32, BinaryNodeData)],
    agent_node_ids: &[u32],
    knowledge_node_ids: &[u32],
    ontology_class_ids: &[u32],
    ontology_individual_ids: &[u32],
    ontology_property_ids: &[u32],
    sssp_data: Option<&HashMap<u32, (f32, i32)>>,
    analytics_data: Option<&HashMap<u32, (u32, f32, u32)>>,
) -> Vec<u8> {
    encode_positions_v3_with_privacy(
        nodes, agent_node_ids, knowledge_node_ids,
        ontology_class_ids, ontology_individual_ids, ontology_property_ids,
        sssp_data, analytics_data, None,
    )
}

/// ADR-050: V3 encoder with privacy enforcement.
///
/// `private_opaque_ids` (when `Some`) is the set of node ids that the caller
/// does NOT own but is allowed to see as opaque placeholders. The encoder ORs
/// `PRIVATE_OPAQUE_FLAG` (bit 29) into the wire id for each such node so the
/// client can render them without label/metadata.
///
/// The flag is only applied when `sovereign_schema_enabled()` returns true —
/// otherwise we retain pre-ADR-050 wire behaviour byte-for-byte.
pub fn encode_positions_v3_with_privacy(
    nodes: &[(u32, BinaryNodeData)],
    agent_node_ids: &[u32],
    knowledge_node_ids: &[u32],
    ontology_class_ids: &[u32],
    ontology_individual_ids: &[u32],
    ontology_property_ids: &[u32],
    sssp_data: Option<&HashMap<u32, (f32, i32)>>,
    analytics_data: Option<&HashMap<u32, (u32, f32, u32)>>,
    private_opaque_ids: Option<&std::collections::HashSet<u32>>,
) -> Vec<u8> {
    // Always use V3 as the default protocol (P0-4 Analytics Extension)
    let protocol_version = PROTOCOL_V3;
    let item_size = WIRE_V3_ITEM_SIZE;

    if !nodes.is_empty() {
        trace!(
            "Encoding {} nodes with agent flags using protocol v{} (item_size={})",
            nodes.len(),
            protocol_version,
            item_size
        );
    }

    let mut buffer = Vec::with_capacity(1 + nodes.len() * item_size);

    buffer.push(protocol_version);

    let sample_size = std::cmp::min(3, nodes.len());
    if sample_size > 0 {
        trace!(
            "Sample of nodes being encoded with agent flags (protocol v{}):",
            protocol_version
        );
    }

    let sovereign_on = sovereign_schema_enabled();

    for (node_id, node) in nodes {

        let flagged_id = if agent_node_ids.contains(node_id) {
            set_agent_flag(*node_id)
        } else if knowledge_node_ids.contains(node_id) {
            set_knowledge_flag(*node_id)
        } else if ontology_class_ids.contains(node_id) {
            set_ontology_class_flag(*node_id)
        } else if ontology_individual_ids.contains(node_id) {
            set_ontology_individual_flag(*node_id)
        } else if ontology_property_ids.contains(node_id) {
            set_ontology_property_flag(*node_id)
        } else {
            debug_assert!(
                *node_id <= NODE_ID_MASK,
                "Unflagged node ID {} (0x{:08X}) exceeds 26-bit limit (max {}). Raw Neo4j ID leaked to wire.",
                node_id, node_id, NODE_ID_MASK
            );
            *node_id
        };

        // ADR-050: OR in bit 29 if the caller is not the owner of this
        // private node. Gated by the SOVEREIGN_SCHEMA feature flag so the
        // wire format is unchanged when the flag is off.
        let flagged_id = if sovereign_on {
            let is_private = private_opaque_ids
                .map(|s| s.contains(node_id) || s.contains(&get_actual_node_id(flagged_id)))
                .unwrap_or(false);
            encode_node_id(flagged_id, is_private)
        } else {
            flagged_id
        };

        if sample_size > 0 && *node_id < sample_size as u32 {
            trace!(
                "Encoding node {}: pos=[{:.3},{:.3},{:.3}], vel=[{:.3},{:.3},{:.3}], is_agent={}",
                node_id,
                node.x,
                node.y,
                node.z,
                node.vx,
                node.vy,
                node.vz,
                agent_node_ids.contains(node_id)
            );
        }

        // V3 always uses u32 IDs
        let wire_id = to_wire_id_v2(flagged_id);
        buffer.extend_from_slice(&wire_id.to_le_bytes());

        // Position (12 bytes)
        buffer.extend_from_slice(&node.x.to_le_bytes());
        buffer.extend_from_slice(&node.y.to_le_bytes());
        buffer.extend_from_slice(&node.z.to_le_bytes());

        // Velocity (12 bytes)
        buffer.extend_from_slice(&node.vx.to_le_bytes());
        buffer.extend_from_slice(&node.vy.to_le_bytes());
        buffer.extend_from_slice(&node.vz.to_le_bytes());

        // SSSP data (8 bytes) - read from sssp_data if available
        let (sssp_distance, sssp_parent) = sssp_data
            .and_then(|m| m.get(node_id))
            .copied()
            .unwrap_or((f32::INFINITY, -1));
        buffer.extend_from_slice(&sssp_distance.to_le_bytes());
        buffer.extend_from_slice(&sssp_parent.to_le_bytes());

        // Analytics data (12 bytes) - V3 extension populated from shared analytics store
        let (cluster_id, anomaly_score, community_id) = analytics_data
            .and_then(|m| m.get(node_id))
            .copied()
            .unwrap_or((0, 0.0, 0));
        buffer.extend_from_slice(&cluster_id.to_le_bytes());
        buffer.extend_from_slice(&anomaly_score.to_le_bytes());
        buffer.extend_from_slice(&community_id.to_le_bytes());
    }

    
    if nodes.len() > 0 {
        trace!(
            "Encoded binary data with agent flags (v{}): {} bytes for {} nodes",
            protocol_version,
            buffer.len(),
            nodes.len()
        );
    }
    buffer
}

/// ADR-037: Encode pre-flagged node data with analytics.
///
/// For paths where fetch_nodes() already applied type flags to node IDs.
/// Passes empty type arrays to avoid double-flagging.
pub fn encode_node_data_with_live_analytics(
    nodes: &[(u32, BinaryNodeData)],
    analytics_data: Option<&HashMap<u32, (u32, f32, u32)>>,
) -> Vec<u8> {
    encode_positions_v3(nodes, &[], &[], &[], &[], &[], None, analytics_data)
}

/// ADR-037: Backward-compat alias for encode_positions_v3.
#[inline]
pub fn encode_node_data_extended_with_sssp(
    nodes: &[(u32, BinaryNodeData)],
    agent_node_ids: &[u32],
    knowledge_node_ids: &[u32],
    ontology_class_ids: &[u32],
    ontology_individual_ids: &[u32],
    ontology_property_ids: &[u32],
    sssp_data: Option<&HashMap<u32, (f32, i32)>>,
    analytics_data: Option<&HashMap<u32, (u32, f32, u32)>>,
) -> Vec<u8> {
    encode_positions_v3(
        nodes, agent_node_ids, knowledge_node_ids,
        ontology_class_ids, ontology_individual_ids, ontology_property_ids,
        sssp_data, analytics_data,
    )
}

pub fn decode_node_data(data: &[u8]) -> Result<Vec<(u32, BinaryNodeData)>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    if data.len() > MAX_PAYLOAD_SIZE {
        return Err(format!(
            "Payload size {} exceeds maximum {}",
            data.len(),
            MAX_PAYLOAD_SIZE
        ));
    }

    if data.len() < 1 {
        return Err("Data too small for protocol version".to_string());
    }

    let protocol_version = data[0];
    let payload = &data[1..];

    match protocol_version {
        1 => Err("Protocol V1 is no longer supported. Please upgrade client.".to_string()),
        2 => Err("V2 protocol no longer supported. Please upgrade client to V3+.".to_string()),
        PROTOCOL_V3 => decode_node_data_v3(payload),
        PROTOCOL_V4 => Err("V4 delta frames require decode_node_data_delta() with previous state".to_string()),
        5 => {
            // V5: [version_byte][8-byte broadcast_seq][V3 node data]
            if payload.len() < 8 {
                return Err("V5 frame too small for broadcast sequence".to_string());
            }
            // Skip 8-byte broadcast sequence number
            decode_node_data_v3(&payload[8..])
        }
        v => Err(format!("Unknown protocol version: {}", v)),
    }
}

// decode_node_data_v1 REMOVED - V1 protocol no longer supported

// decode_node_data_v2 REMOVED — V2 protocol no longer supported (was 36 bytes/node, no analytics)

/// Decode Protocol V3 with analytics data (P0-4)
/// Returns standard BinaryNodeData (analytics data is discarded in basic decode)
fn decode_node_data_v3(data: &[u8]) -> Result<Vec<(u32, BinaryNodeData)>, String> {
    if data.len() % WIRE_V3_ITEM_SIZE != 0 {
        return Err(format!(
            "Data size {} is not a multiple of V3 wire item size {}",
            data.len(),
            WIRE_V3_ITEM_SIZE
        ));
    }

    let expected_nodes = data.len() / WIRE_V3_ITEM_SIZE;
    if expected_nodes > MAX_NODE_COUNT {
        return Err(format!(
            "Node count {} exceeds maximum {}",
            expected_nodes, MAX_NODE_COUNT
        ));
    }

    debug!(
        "Decoding V3 binary data with analytics: size={} bytes, expected nodes={}",
        data.len(),
        expected_nodes
    );

    let mut updates = Vec::with_capacity(expected_nodes);
    let max_samples = 3;
    let mut samples_logged = 0;

    for chunk in data.chunks_exact(WIRE_V3_ITEM_SIZE) {
        let mut cursor = 0;

        // Node ID (4 bytes)
        let wire_id = u32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;

        // Position (12 bytes)
        let pos_x = f32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;
        let pos_y = f32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;
        let pos_z = f32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;

        // Velocity (12 bytes)
        let vel_x = f32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;
        let vel_y = f32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;
        let vel_z = f32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;

        // SSSP data (8 bytes) - read but not used
        let _sssp_distance = f32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;
        let _sssp_parent = i32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;

        // Analytics data (12 bytes) - NEW in V3
        let _cluster_id = u32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;
        let _anomaly_score = f32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);
        cursor += 4;
        let _community_id = u32::from_le_bytes([
            chunk[cursor],
            chunk[cursor + 1],
            chunk[cursor + 2],
            chunk[cursor + 3],
        ]);

        let full_node_id = from_wire_id_v2(wire_id);

        if samples_logged < max_samples {
            let is_agent = is_agent_node(full_node_id);
            let actual_id = get_actual_node_id(full_node_id);
            debug!(
                "Decoded V3 node wire_id={} -> full_id={} (actual_id={}, is_agent={}): pos=[{:.3},{:.3},{:.3}], vel=[{:.3},{:.3},{:.3}], cluster={}, anomaly={:.3}, community={}",
                wire_id, full_node_id, actual_id, is_agent,
                pos_x, pos_y, pos_z,
                vel_x, vel_y, vel_z,
                _cluster_id, _anomaly_score, _community_id
            );
            samples_logged += 1;
        }

        let actual_id = get_actual_node_id(full_node_id);
        let server_node_data = BinaryNodeData {
            node_id: actual_id,
            x: pos_x,
            y: pos_y,
            z: pos_z,
            vx: vel_x,
            vy: vel_y,
            vz: vel_z,
        };

        updates.push((actual_id, server_node_data));
    }

    debug!(
        "Successfully decoded {} V3 nodes with analytics from binary data",
        updates.len()
    );
    Ok(updates)
}

pub fn calculate_message_size(updates: &[(u32, BinaryNodeData)]) -> usize {
    // V3 is now the default protocol (48 bytes per node)
    1 + updates.len() * WIRE_V3_ITEM_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_format_size() {
        // V3: 4 + 12 + 12 + 4 + 4 + 4 + 4 + 4 = 48 bytes (CURRENT)
        assert_eq!(WIRE_V3_ITEM_SIZE, 48);
        assert_eq!(WIRE_ITEM_SIZE, WIRE_V3_ITEM_SIZE); // Default is now V3
        assert_eq!(
            WIRE_ID_SIZE + WIRE_VEC3_SIZE + WIRE_VEC3_SIZE + WIRE_F32_SIZE + WIRE_I32_SIZE +
            WIRE_U32_SIZE + WIRE_F32_SIZE + WIRE_U32_SIZE,
            48
        );
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let nodes = vec![
            (
                1u32,
                BinaryNodeData {
                    node_id: 1,
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    vx: 0.1,
                    vy: 0.2,
                    vz: 0.3,
                },
            ),
            (
                2u32,
                BinaryNodeData {
                    node_id: 2,
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                    vx: 0.4,
                    vy: 0.5,
                    vz: 0.6,
                },
            ),
        ];

        let encoded = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);

        // V3 is now the default: 1 header byte + nodes * 48 bytes
        assert_eq!(encoded.len(), 1 + nodes.len() * WIRE_V3_ITEM_SIZE);

        let decoded = decode_node_data(&encoded).unwrap();
        assert_eq!(nodes.len(), decoded.len());

        for ((orig_id, orig_data), (dec_id, dec_data)) in nodes.iter().zip(decoded.iter()) {
            assert_eq!(orig_id, dec_id);
            assert_eq!(orig_data.position(), dec_data.position());
            assert_eq!(orig_data.velocity(), dec_data.velocity());
        }
    }

    #[test]
    fn test_decode_invalid_data() {

        // V2 protocol should be rejected
        let mut data = vec![2u8]; // V2 version byte
        data.extend_from_slice(&[0u8; 37]);
        let result = decode_node_data(&data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("V2 protocol no longer supported"));

        // V2 with empty payload should also be rejected
        let result = decode_node_data(&[2u8]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("V2 protocol no longer supported"));
    }

    #[test]
    fn test_message_size_calculation() {
        let nodes = vec![(
            1u32,
            BinaryNodeData {
                node_id: 1,
                x: 1.0,
                y: 2.0,
                z: 3.0,
                vx: 0.1,
                vy: 0.2,
                vz: 0.3,
            },
        )];

        let size = calculate_message_size(&nodes);
        // V3: 1 header + 48 bytes per node
        assert_eq!(size, 1 + 48);

        let encoded = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);
        assert_eq!(encoded.len(), size);
    }

    #[test]
    fn test_agent_flag_functions() {
        let node_id = 42u32;

        
        let flagged_id = set_agent_flag(node_id);
        assert_eq!(flagged_id, node_id | AGENT_NODE_FLAG);
        assert!(is_agent_node(flagged_id));

        
        let actual_id = get_actual_node_id(flagged_id);
        assert_eq!(actual_id, node_id);

        
        let cleared_id = clear_agent_flag(flagged_id);
        assert_eq!(cleared_id, node_id);
        assert!(!is_agent_node(cleared_id));

        
        assert!(!is_agent_node(node_id));
    }

    #[test]
    fn test_wire_id_conversion() {
        
        let node_id = 42u32;
        let wire_id = to_wire_id(node_id);
        assert_eq!(wire_id, 42u32); 
        assert_eq!(from_wire_id(wire_id), node_id);

        
        let agent_id = set_agent_flag(node_id);
        let agent_wire_id = to_wire_id(agent_id);
        assert_eq!(agent_wire_id & NODE_ID_MASK, 42u32);
        assert!((agent_wire_id & AGENT_NODE_FLAG) != 0);
        assert_eq!(from_wire_id(agent_wire_id), agent_id);


        let knowledge_id = set_knowledge_flag(node_id);
        let knowledge_wire_id = to_wire_id(knowledge_id);
        assert_eq!(knowledge_wire_id & NODE_ID_MASK, 42u32);
        assert!((knowledge_wire_id & KNOWLEDGE_NODE_FLAG) != 0);
        assert_eq!(from_wire_id(knowledge_wire_id), knowledge_id);

        
        let large_id = 0x5432u32;
        let wire_id = to_wire_id(large_id);
        assert_eq!(wire_id, 0x5432u32); 
        assert_eq!(from_wire_id(wire_id), large_id);
    }

    #[test]
    fn test_encode_with_agent_flags() {
        let nodes = vec![
            (
                1u32,
                BinaryNodeData {
                    node_id: 1,
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    vx: 0.1,
                    vy: 0.2,
                    vz: 0.3,
                },
            ),
            (
                2u32,
                BinaryNodeData {
                    node_id: 2,
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                    vx: 0.4,
                    vy: 0.5,
                    vz: 0.6,
                },
            ),
        ];

        // Mark node 2 as agent
        let agent_ids = vec![2u32];
        let encoded = encode_node_data_extended(&nodes, &agent_ids, &[], &[], &[], &[]);

        // V3 format: 1 header + nodes * 48 bytes
        assert_eq!(encoded.len(), 1 + nodes.len() * WIRE_V3_ITEM_SIZE);

        let decoded = decode_node_data(&encoded).unwrap();
        assert_eq!(nodes.len(), decoded.len());

        
        for ((orig_id, orig_data), (dec_id, dec_data)) in nodes.iter().zip(decoded.iter()) {
            assert_eq!(orig_id, dec_id); 
            assert_eq!(orig_data.position(), dec_data.position());
            assert_eq!(orig_data.velocity(), dec_data.velocity());
        }
    }

    #[test]
    fn test_large_node_id_no_truncation() {
        
        let large_nodes = vec![
            (
                20000u32,
                BinaryNodeData {
                    node_id: 20000,
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    vx: 0.1,
                    vy: 0.2,
                    vz: 0.3,
                },
            ),
            (
                100000u32,
                BinaryNodeData {
                    node_id: 100000,
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                    vx: 0.4,
                    vy: 0.5,
                    vz: 0.6,
                },
            ),
        ];

        let encoded = encode_node_data_extended(&large_nodes, &[], &[], &[], &[], &[]);

        // V3 is now the default protocol
        assert_eq!(encoded[0], PROTOCOL_V3);

        let decoded = decode_node_data(&encoded).unwrap();
        assert_eq!(large_nodes.len(), decoded.len());

        
        assert_eq!(decoded[0].0, 20000u32);
        assert_eq!(decoded[1].0, 100000u32);
    }

    #[test]
    fn test_ontology_node_flags() {
        let node_id = 123u32;

        // Test ontology class flag
        let class_id = set_ontology_class_flag(node_id);
        assert!(is_ontology_class(class_id));
        assert!(is_ontology_node(class_id));
        assert!(!is_ontology_individual(class_id));
        assert!(!is_ontology_property(class_id));
        // get_actual_node_id masks out all flags including ontology flags
        // The flagged ID includes the ontology bits, but actual ID strips them
        assert_eq!(get_actual_node_id(class_id), node_id);
        assert_eq!(get_node_type(class_id), NodeType::OntologyClass);

        // Test ontology individual flag
        let individual_id = set_ontology_individual_flag(node_id);
        assert!(is_ontology_individual(individual_id));
        assert!(is_ontology_node(individual_id));
        assert!(!is_ontology_class(individual_id));
        assert!(!is_ontology_property(individual_id));
        assert_eq!(get_actual_node_id(individual_id), node_id);
        assert_eq!(get_node_type(individual_id), NodeType::OntologyIndividual);

        // Test ontology property flag
        let property_id = set_ontology_property_flag(node_id);
        assert!(is_ontology_property(property_id));
        assert!(is_ontology_node(property_id));
        assert!(!is_ontology_class(property_id));
        assert!(!is_ontology_individual(property_id));
        assert_eq!(get_actual_node_id(property_id), node_id);
        assert_eq!(get_node_type(property_id), NodeType::OntologyProperty);

        // Test that unflagged node is not an ontology node
        assert!(!is_ontology_node(node_id));
        assert!(!is_ontology_class(node_id));
        assert!(!is_ontology_individual(node_id));
        assert!(!is_ontology_property(node_id));
    }

    #[test]
    fn test_encode_with_ontology_types() {
        let nodes = vec![
            (
                1u32,
                BinaryNodeData {
                    node_id: 1,
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    vx: 0.1,
                    vy: 0.2,
                    vz: 0.3,
                },
            ),
            (
                2u32,
                BinaryNodeData {
                    node_id: 2,
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                    vx: 0.4,
                    vy: 0.5,
                    vz: 0.6,
                },
            ),
            (
                3u32,
                BinaryNodeData {
                    node_id: 3,
                    x: 7.0,
                    y: 8.0,
                    z: 9.0,
                    vx: 0.7,
                    vy: 0.8,
                    vz: 0.9,
                },
            ),
        ];

        // Mark nodes with ontology types
        let class_ids = vec![1u32];
        let individual_ids = vec![2u32];
        let property_ids = vec![3u32];

        let encoded =
            encode_node_data_extended(&nodes, &[], &[], &class_ids, &individual_ids, &property_ids);

        // V3 format: 1 header + nodes * 48 bytes
        assert_eq!(encoded.len(), 1 + nodes.len() * WIRE_V3_ITEM_SIZE);

        let decoded = decode_node_data(&encoded).unwrap();
        assert_eq!(nodes.len(), decoded.len());

        // After decoding, the actual node IDs should match (flags are stripped)
        // decode_node_data strips flags via get_actual_node_id
        for ((orig_id, orig_data), (dec_id, dec_data)) in nodes.iter().zip(decoded.iter()) {
            assert_eq!(*orig_id, *dec_id);
            assert_eq!(orig_data.position(), dec_data.position());
            assert_eq!(orig_data.velocity(), dec_data.velocity());
        }
    }

    #[test]
    fn test_ontology_flags_preserved_in_wire_format() {
        let nodes = vec![(
            100u32,
            BinaryNodeData {
                node_id: 100,
                x: 1.0,
                y: 2.0,
                z: 3.0,
                vx: 0.1,
                vy: 0.2,
                vz: 0.3,
            },
        )];

        let class_ids = vec![100u32];
        let encoded = encode_node_data_extended(&nodes, &[], &[], &class_ids, &[], &[]);

        // V3 is now the default protocol
        assert_eq!(encoded[0], PROTOCOL_V3);

        // Wire ID is at offset 1
        let wire_id = u32::from_le_bytes([encoded[1], encoded[2], encoded[3], encoded[4]]);

        // Verify ontology flag is set in the wire format
        assert_eq!(wire_id & ONTOLOGY_TYPE_MASK, ONTOLOGY_CLASS_FLAG);
        // Verify the actual node ID is preserved (using NODE_ID_MASK to extract it)
        assert_eq!(wire_id & NODE_ID_MASK, 100u32);
    }

    #[test]
    fn test_v1_protocol_rejected() {
        // V1 protocol should be rejected with clear error message
        let v1_encoded = vec![1u8]; // Protocol version 1
        let result = decode_node_data(&v1_encoded);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no longer supported"));
    }

    // ========================================================================
    // Regression tests: post-refactor validation of canonical encoding surface
    // Validates that the 3 remaining encode functions (encode_node_data_extended,
    // encode_node_data_extended_with_sssp, encode_node_data_with_live_analytics)
    // produce correct V3 frames after removal of 6 wrapper functions.
    // ========================================================================

    #[test]
    fn test_encode_extended_no_type_arrays() {
        // GIVEN: Two nodes with no type array membership (empty agent/knowledge/ontology arrays)
        let nodes = vec![
            (
                10u32,
                BinaryNodeData {
                    node_id: 10,
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    vx: 0.1,
                    vy: 0.2,
                    vz: 0.3,
                },
            ),
            (
                20u32,
                BinaryNodeData {
                    node_id: 20,
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                    vx: 0.4,
                    vy: 0.5,
                    vz: 0.6,
                },
            ),
        ];

        // WHEN: Encoding with empty type arrays via encode_node_data_extended
        let encoded = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);

        // THEN: Output is a valid V3 frame
        assert_eq!(encoded[0], PROTOCOL_V3, "Version header must be V3 (3)");
        assert_eq!(
            encoded.len(),
            1 + nodes.len() * WIRE_V3_ITEM_SIZE,
            "Frame size must be 1 header + N*48"
        );

        // THEN: Wire IDs have NO type flags set (bits 26-31 all zero)
        for i in 0..nodes.len() {
            let offset = 1 + i * WIRE_V3_ITEM_SIZE;
            let wire_id = u32::from_le_bytes([
                encoded[offset],
                encoded[offset + 1],
                encoded[offset + 2],
                encoded[offset + 3],
            ]);
            assert_eq!(
                wire_id & !NODE_ID_MASK,
                0,
                "Node at index {} should have no type flags set, but wire_id=0x{:08X}",
                i,
                wire_id
            );
            assert_eq!(
                wire_id & NODE_ID_MASK,
                nodes[i].0,
                "Node ID must be preserved without flags"
            );
        }

        // THEN: Roundtrip decode succeeds with correct IDs and positions
        let decoded = decode_node_data(&encoded).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].0, 10);
        assert_eq!(decoded[1].0, 20);
        assert_eq!(decoded[0].1.position(), nodes[0].1.position());
        assert_eq!(decoded[1].1.velocity(), nodes[1].1.velocity());
    }

    #[test]
    fn test_encode_with_live_analytics_empty_type_arrays() {
        // GIVEN: Nodes with analytics data (simulating the position_updates.rs hot path)
        let nodes = vec![
            (
                5u32,
                BinaryNodeData {
                    node_id: 5,
                    x: 10.0,
                    y: 20.0,
                    z: 30.0,
                    vx: 1.0,
                    vy: 2.0,
                    vz: 3.0,
                },
            ),
            (
                15u32,
                BinaryNodeData {
                    node_id: 15,
                    x: 40.0,
                    y: 50.0,
                    z: 60.0,
                    vx: 4.0,
                    vy: 5.0,
                    vz: 6.0,
                },
            ),
        ];
        let mut analytics = HashMap::new();
        analytics.insert(5u32, (7u32, 0.85f32, 3u32));   // cluster=7, anomaly=0.85, community=3
        analytics.insert(15u32, (12u32, 0.10f32, 8u32));  // cluster=12, anomaly=0.10, community=8

        // WHEN: Encoding via the live analytics convenience wrapper
        let encoded = encode_node_data_with_live_analytics(&nodes, Some(&analytics));

        // THEN: Output is valid V3 frame with correct size
        assert_eq!(encoded[0], PROTOCOL_V3);
        assert_eq!(encoded.len(), 1 + 2 * WIRE_V3_ITEM_SIZE);

        // THEN: All type arrays are empty, so wire IDs have no flags
        let wire_id_0 = u32::from_le_bytes([encoded[1], encoded[2], encoded[3], encoded[4]]);
        assert_eq!(wire_id_0, 5, "First node wire ID should be raw 5 (no flags)");

        let wire_id_1 = u32::from_le_bytes([
            encoded[1 + WIRE_V3_ITEM_SIZE],
            encoded[2 + WIRE_V3_ITEM_SIZE],
            encoded[3 + WIRE_V3_ITEM_SIZE],
            encoded[4 + WIRE_V3_ITEM_SIZE],
        ]);
        assert_eq!(wire_id_1, 15, "Second node wire ID should be raw 15 (no flags)");

        // THEN: Analytics data is embedded in the wire bytes at the correct offsets
        // V3 layout per node: id(4) + pos(12) + vel(12) + sssp_dist(4) + sssp_parent(4) + cluster(4) + anomaly(4) + community(4)
        // Analytics starts at byte 36 within each node's 48-byte block
        let analytics_offset_0 = 1 + 36; // first node: header(1) + 36 bytes into node block
        let cluster_0 = u32::from_le_bytes([
            encoded[analytics_offset_0],
            encoded[analytics_offset_0 + 1],
            encoded[analytics_offset_0 + 2],
            encoded[analytics_offset_0 + 3],
        ]);
        let anomaly_0 = f32::from_le_bytes([
            encoded[analytics_offset_0 + 4],
            encoded[analytics_offset_0 + 5],
            encoded[analytics_offset_0 + 6],
            encoded[analytics_offset_0 + 7],
        ]);
        let community_0 = u32::from_le_bytes([
            encoded[analytics_offset_0 + 8],
            encoded[analytics_offset_0 + 9],
            encoded[analytics_offset_0 + 10],
            encoded[analytics_offset_0 + 11],
        ]);
        assert_eq!(cluster_0, 7, "Node 5 cluster_id must be 7");
        assert!((anomaly_0 - 0.85).abs() < f32::EPSILON, "Node 5 anomaly_score must be 0.85");
        assert_eq!(community_0, 3, "Node 5 community_id must be 3");

        // THEN: Roundtrip decode recovers positions (analytics is discarded by basic decode)
        let decoded = decode_node_data(&encoded).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].0, 5);
        assert_eq!(decoded[1].0, 15);
        assert_eq!(decoded[0].1.x, 10.0);
        assert_eq!(decoded[1].1.y, 50.0);
    }

    #[test]
    fn test_encode_extended_with_sssp_full() {
        // GIVEN: Nodes with ALL parameters populated
        let nodes = vec![
            (
                1u32,
                BinaryNodeData {
                    node_id: 1,
                    x: 100.0,
                    y: 200.0,
                    z: 300.0,
                    vx: 10.0,
                    vy: 20.0,
                    vz: 30.0,
                },
            ),
            (
                2u32,
                BinaryNodeData {
                    node_id: 2,
                    x: 400.0,
                    y: 500.0,
                    z: 600.0,
                    vx: 40.0,
                    vy: 50.0,
                    vz: 60.0,
                },
            ),
            (
                3u32,
                BinaryNodeData {
                    node_id: 3,
                    x: 700.0,
                    y: 800.0,
                    z: 900.0,
                    vx: 70.0,
                    vy: 80.0,
                    vz: 90.0,
                },
            ),
        ];

        let agent_ids = vec![1u32];
        let knowledge_ids = vec![2u32];
        let ontology_class_ids = vec![3u32];
        let ontology_individual_ids: Vec<u32> = vec![];
        let ontology_property_ids: Vec<u32> = vec![];

        let mut sssp = HashMap::new();
        sssp.insert(1u32, (1.5f32, 0i32));    // distance=1.5, parent=root(0)
        sssp.insert(2u32, (3.7f32, 1i32));    // distance=3.7, parent=node 1
        sssp.insert(3u32, (5.2f32, 2i32));    // distance=5.2, parent=node 2

        let mut analytics = HashMap::new();
        analytics.insert(1u32, (0u32, 0.1f32, 10u32));   // cluster=0, anomaly=0.1, community=10
        analytics.insert(2u32, (1u32, 0.95f32, 20u32));  // cluster=1, anomaly=0.95, community=20
        analytics.insert(3u32, (2u32, 0.5f32, 30u32));   // cluster=2, anomaly=0.5, community=30

        // WHEN: Encoding with ALL parameters via canonical encoder
        let encoded = encode_node_data_extended_with_sssp(
            &nodes,
            &agent_ids,
            &knowledge_ids,
            &ontology_class_ids,
            &ontology_individual_ids,
            &ontology_property_ids,
            Some(&sssp),
            Some(&analytics),
        );

        // THEN: Valid V3 frame
        assert_eq!(encoded[0], PROTOCOL_V3);
        assert_eq!(encoded.len(), 1 + 3 * WIRE_V3_ITEM_SIZE);

        // THEN: Node 1 has agent flag
        let wire_id_0 = u32::from_le_bytes([encoded[1], encoded[2], encoded[3], encoded[4]]);
        assert!(
            is_agent_node(wire_id_0),
            "Node 1 must have agent flag set, wire_id=0x{:08X}",
            wire_id_0
        );
        assert_eq!(get_actual_node_id(wire_id_0), 1);

        // THEN: Node 2 has knowledge flag
        let node2_offset = 1 + WIRE_V3_ITEM_SIZE;
        let wire_id_1 = u32::from_le_bytes([
            encoded[node2_offset],
            encoded[node2_offset + 1],
            encoded[node2_offset + 2],
            encoded[node2_offset + 3],
        ]);
        assert!(
            is_knowledge_node(wire_id_1),
            "Node 2 must have knowledge flag set, wire_id=0x{:08X}",
            wire_id_1
        );
        assert_eq!(get_actual_node_id(wire_id_1), 2);

        // THEN: Node 3 has ontology class flag
        let node3_offset = 1 + 2 * WIRE_V3_ITEM_SIZE;
        let wire_id_2 = u32::from_le_bytes([
            encoded[node3_offset],
            encoded[node3_offset + 1],
            encoded[node3_offset + 2],
            encoded[node3_offset + 3],
        ]);
        assert!(
            is_ontology_class(wire_id_2),
            "Node 3 must have ontology class flag, wire_id=0x{:08X}",
            wire_id_2
        );
        assert_eq!(get_actual_node_id(wire_id_2), 3);

        // THEN: SSSP data is correctly embedded for node 1
        // SSSP starts at offset 28 within each node block (id=4 + pos=12 + vel=12 = 28)
        let sssp_offset_0 = 1 + 28;
        let sssp_dist_0 = f32::from_le_bytes([
            encoded[sssp_offset_0],
            encoded[sssp_offset_0 + 1],
            encoded[sssp_offset_0 + 2],
            encoded[sssp_offset_0 + 3],
        ]);
        let sssp_parent_0 = i32::from_le_bytes([
            encoded[sssp_offset_0 + 4],
            encoded[sssp_offset_0 + 5],
            encoded[sssp_offset_0 + 6],
            encoded[sssp_offset_0 + 7],
        ]);
        assert!((sssp_dist_0 - 1.5).abs() < f32::EPSILON, "Node 1 SSSP distance must be 1.5");
        assert_eq!(sssp_parent_0, 0, "Node 1 SSSP parent must be 0 (root)");

        // THEN: Analytics data is correctly embedded for node 2
        let analytics_offset_1 = 1 + WIRE_V3_ITEM_SIZE + 36;
        let cluster_1 = u32::from_le_bytes([
            encoded[analytics_offset_1],
            encoded[analytics_offset_1 + 1],
            encoded[analytics_offset_1 + 2],
            encoded[analytics_offset_1 + 3],
        ]);
        let anomaly_1 = f32::from_le_bytes([
            encoded[analytics_offset_1 + 4],
            encoded[analytics_offset_1 + 5],
            encoded[analytics_offset_1 + 6],
            encoded[analytics_offset_1 + 7],
        ]);
        let community_1 = u32::from_le_bytes([
            encoded[analytics_offset_1 + 8],
            encoded[analytics_offset_1 + 9],
            encoded[analytics_offset_1 + 10],
            encoded[analytics_offset_1 + 11],
        ]);
        assert_eq!(cluster_1, 1, "Node 2 cluster_id must be 1");
        assert!((anomaly_1 - 0.95).abs() < f32::EPSILON, "Node 2 anomaly_score must be 0.95");
        assert_eq!(community_1, 20, "Node 2 community_id must be 20");

        // THEN: Roundtrip decode recovers correct positions
        let decoded = decode_node_data(&encoded).unwrap();
        assert_eq!(decoded.len(), 3);
        for ((_orig_id, orig_data), (dec_id, dec_data)) in nodes.iter().zip(decoded.iter()) {
            assert_eq!(*dec_id, get_actual_node_id(dec_data.node_id));
            assert_eq!(orig_data.position(), dec_data.position());
            assert_eq!(orig_data.velocity(), dec_data.velocity());
        }
    }

    #[test]
    fn test_v3_frame_always_48_bytes_per_node() {
        // GIVEN: Various node counts from 0 to 5
        let make_node = |id: u32| -> (u32, BinaryNodeData) {
            (
                id,
                BinaryNodeData {
                    node_id: id,
                    x: id as f32,
                    y: id as f32 * 2.0,
                    z: id as f32 * 3.0,
                    vx: 0.0,
                    vy: 0.0,
                    vz: 0.0,
                },
            )
        };

        for count in 0..=5 {
            let nodes: Vec<_> = (1..=count).map(|i| make_node(i as u32)).collect();
            let expected_size = 1 + nodes.len() * 48;

            // WHEN/THEN: encode_node_data_extended produces 1 + N*48 bytes
            let enc1 = encode_node_data_extended(&nodes, &[], &[], &[], &[], &[]);
            assert_eq!(
                enc1.len(),
                expected_size,
                "encode_node_data_extended with {} nodes: expected {} bytes, got {}",
                count,
                expected_size,
                enc1.len()
            );
            if !enc1.is_empty() {
                assert_eq!(enc1[0], PROTOCOL_V3, "Version byte must be 3");
            }

            // WHEN/THEN: encode_node_data_extended_with_sssp produces 1 + N*48 bytes
            let enc2 = encode_node_data_extended_with_sssp(
                &nodes, &[], &[], &[], &[], &[], None, None,
            );
            assert_eq!(
                enc2.len(),
                expected_size,
                "encode_node_data_extended_with_sssp with {} nodes: expected {} bytes, got {}",
                count,
                expected_size,
                enc2.len()
            );

            // WHEN/THEN: encode_node_data_with_live_analytics produces 1 + N*48 bytes
            let enc3 = encode_node_data_with_live_analytics(&nodes, None);
            assert_eq!(
                enc3.len(),
                expected_size,
                "encode_node_data_with_live_analytics with {} nodes: expected {} bytes, got {}",
                count,
                expected_size,
                enc3.len()
            );

            // WHEN/THEN: All three functions produce byte-identical output for same input
            assert_eq!(enc1, enc2, "extended and extended_with_sssp(None,None) must be identical for {} nodes", count);
            assert_eq!(enc1, enc3, "extended and with_live_analytics(None) must be identical for {} nodes", count);
        }
    }

    #[test]
    fn test_type_flags_preserved_through_encode_decode() {
        // GIVEN: Five nodes, each assigned a different type flag
        let nodes = vec![
            (
                1u32,
                BinaryNodeData { node_id: 1, x: 1.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0 },
            ),
            (
                2u32,
                BinaryNodeData { node_id: 2, x: 2.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0 },
            ),
            (
                3u32,
                BinaryNodeData { node_id: 3, x: 3.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0 },
            ),
            (
                4u32,
                BinaryNodeData { node_id: 4, x: 4.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0 },
            ),
            (
                5u32,
                BinaryNodeData { node_id: 5, x: 5.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0 },
            ),
        ];

        let agent_ids = vec![1u32];
        let knowledge_ids = vec![2u32];
        let class_ids = vec![3u32];
        let individual_ids = vec![4u32];
        let property_ids = vec![5u32];

        // WHEN: Encoding with all type arrays populated
        let encoded = encode_node_data_extended(
            &nodes,
            &agent_ids,
            &knowledge_ids,
            &class_ids,
            &individual_ids,
            &property_ids,
        );

        // THEN: Verify each node's wire ID has the correct flag in the raw bytes
        let expected_flags: Vec<(u32, &str, Box<dyn Fn(u32) -> bool>)> = vec![
            (1, "Agent", Box::new(|id| is_agent_node(id))),
            (2, "Knowledge", Box::new(|id| is_knowledge_node(id))),
            (3, "OntologyClass", Box::new(|id| is_ontology_class(id))),
            (4, "OntologyIndividual", Box::new(|id| is_ontology_individual(id))),
            (5, "OntologyProperty", Box::new(|id| is_ontology_property(id))),
        ];

        for (i, (expected_id, flag_name, check_fn)) in expected_flags.iter().enumerate() {
            let offset = 1 + i * WIRE_V3_ITEM_SIZE;
            let wire_id = u32::from_le_bytes([
                encoded[offset],
                encoded[offset + 1],
                encoded[offset + 2],
                encoded[offset + 3],
            ]);

            // Flag is present in wire format
            assert!(
                check_fn(wire_id),
                "Node {} (index {}) must have {} flag in wire format, wire_id=0x{:08X}",
                expected_id, i, flag_name, wire_id
            );

            // Actual node ID is recoverable after masking
            assert_eq!(
                get_actual_node_id(wire_id),
                *expected_id,
                "Node {} actual ID must survive flag encoding",
                expected_id
            );
        }

        // THEN: Decode recovers the correct actual node IDs (flags stripped)
        let decoded = decode_node_data(&encoded).unwrap();
        assert_eq!(decoded.len(), 5);
        for (i, (dec_id, dec_data)) in decoded.iter().enumerate() {
            let expected_id = (i + 1) as u32;
            assert_eq!(
                *dec_id, expected_id,
                "Decoded node at index {} must have ID {}",
                i, expected_id
            );
            assert_eq!(
                dec_data.x,
                expected_id as f32,
                "Decoded node {} must have correct x position",
                expected_id
            );
        }
    }
}

// ============================================================================
// AGENT ACTION EVENTS (Protocol 0x23) - Ephemeral Connection Visualization
// ============================================================================

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

    /// Binary position updates using Protocol V3 (48 bytes/node)
    BinaryPositions = 0,

    VoiceData = 0x02,

    ControlFrame = 0x03,

    /// Delta-encoded position updates (Protocol V4)
    /// Frame 0: FULL state, Frames 1-59: DELTA, Frame 60: FULL resync
    PositionDelta = 0x04,

    /// Client acknowledgement of position broadcast (Protocol V3 backpressure)
    /// Enables true end-to-end flow control vs queue-only confirmation
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
    
    pub fn positions(node_data: &[(u32, BinaryNodeData)]) -> Self {
        Self {
            msg_type: MessageType::BinaryPositions,
            data: encode_node_data_extended_with_sssp(node_data, &[], &[], &[], &[], &[], None, None),
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
            0x04 => MessageType::PositionDelta,
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

#[cfg(test)]
mod control_frame_tests {
    use super::*;
    use crate::models::constraints::ConstraintKind;

    #[test]
    fn test_control_frame_serialization() {
        let constraint = Constraint {
            kind: ConstraintKind::Separation,
            node_indices: vec![1, 2],
            params: vec![100.0],
            weight: 0.8,
            active: true,
        };

        let frame = ControlFrame::constraints_update(vec![constraint], None);
        let bytes = frame.to_bytes().expect("Serialization failed");
        let decoded = ControlFrame::from_bytes(&bytes).expect("Deserialization failed");

        match decoded {
            ControlFrame::ConstraintsUpdate {
                version,
                constraints,
                ..
            } => {
                assert_eq!(version, 1);
                assert_eq!(constraints.len(), 1);
                assert_eq!(constraints[0].kind, ConstraintKind::Separation);
            }
            _ => panic!("Wrong frame type"),
        }
    }

    #[test]
    fn test_multiplexed_message() {
        let nodes = vec![(
            1u32,
            BinaryNodeData {
                node_id: 1,
                x: 1.0,
                y: 2.0,
                z: 3.0,
                vx: 0.1,
                vy: 0.2,
                vz: 0.3,
            },
        )];

        let msg = MultiplexedMessage::positions(&nodes);
        let encoded = msg.encode();

        assert_eq!(encoded[0], 0); 

        let decoded = MultiplexedMessage::decode(&encoded).expect("Decode failed");
        assert_eq!(decoded.msg_type, MessageType::BinaryPositions);
    }

    #[test]
    fn test_simplified_protocol_voice_data() {
        let audio = vec![0x12, 0x34, 0x56, 0x78];

        let encoded = BinaryProtocol::encode_voice_data(&audio);
        assert_eq!(encoded[0], 0x02); 
        assert_eq!(encoded.len(), 1 + audio.len());

        let decoded = BinaryProtocol::decode_message(&encoded).expect("Message decode failed");
        match decoded {
            Message::VoiceData {
                audio: decoded_audio,
            } => {
                assert_eq!(decoded_audio, audio);
            }
            _ => panic!("Expected VoiceData message"),
        }
    }

    #[test]
    fn test_protocol_error_handling() {
        // Empty message
        let result = BinaryProtocol::decode_message(&[]);
        assert!(matches!(result, Err(ProtocolError::DecodingError(_))));

        // Invalid message type
        let result = BinaryProtocol::decode_message(&[0xFF]);
        assert!(matches!(
            result,
            Err(ProtocolError::InvalidMessageType(0xFF))
        ));
    }

    #[test]
    fn test_agent_action_type_conversion() {
        assert_eq!(AgentActionType::from(0), AgentActionType::Query);
        assert_eq!(AgentActionType::from(1), AgentActionType::Update);
        assert_eq!(AgentActionType::from(2), AgentActionType::Create);
        assert_eq!(AgentActionType::from(3), AgentActionType::Delete);
        assert_eq!(AgentActionType::from(4), AgentActionType::Link);
        assert_eq!(AgentActionType::from(5), AgentActionType::Transform);
        assert_eq!(AgentActionType::from(255), AgentActionType::Query); // Default
    }

    #[test]
    fn test_agent_action_event_encode_decode() {
        let event = AgentActionEvent::new(
            42,   // source_agent_id
            100,  // target_node_id
            AgentActionType::Update,
            500,  // duration_ms
        );

        let encoded = event.encode();

        // Verify message type header
        assert_eq!(encoded[0], MessageType::AgentAction as u8);
        assert_eq!(encoded[0], 0x23);

        // Verify header size: 1 (msg type) + 15 (header) = 16 bytes minimum
        assert!(encoded.len() >= 16);

        // Decode (skip msg type byte)
        let decoded = AgentActionEvent::decode(&encoded[1..]).expect("Decode failed");

        assert_eq!(decoded.source_agent_id, 42);
        assert_eq!(decoded.target_node_id, 100);
        assert_eq!(decoded.get_action_type(), AgentActionType::Update);
        assert_eq!(decoded.duration_ms, 500);
        assert!(decoded.payload.is_empty());
    }

    #[test]
    fn test_agent_action_event_with_payload() {
        let mut event = AgentActionEvent::new(
            1,
            2,
            AgentActionType::Create,
            1000,
        );
        event.payload = vec![0xDE, 0xAD, 0xBE, 0xEF];

        let encoded = event.encode();

        // 1 (msg type) + 15 (header) + 4 (payload) = 20 bytes
        assert_eq!(encoded.len(), 20);

        let decoded = AgentActionEvent::decode(&encoded[1..]).expect("Decode failed");
        assert_eq!(decoded.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_agent_action_batch_encode_decode() {
        let events = vec![
            AgentActionEvent::new(1, 10, AgentActionType::Query, 100),
            AgentActionEvent::new(2, 20, AgentActionType::Update, 200),
            AgentActionEvent::new(3, 30, AgentActionType::Delete, 300),
        ];

        let encoded = encode_agent_actions(&events);

        // First byte is message type
        assert_eq!(encoded[0], MessageType::AgentAction as u8);

        // Decode batch (skip msg type byte)
        let decoded = decode_agent_actions(&encoded[1..]).expect("Batch decode failed");

        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0].source_agent_id, 1);
        assert_eq!(decoded[0].target_node_id, 10);
        assert_eq!(decoded[0].get_action_type(), AgentActionType::Query);

        assert_eq!(decoded[1].source_agent_id, 2);
        assert_eq!(decoded[1].get_action_type(), AgentActionType::Update);

        assert_eq!(decoded[2].source_agent_id, 3);
        assert_eq!(decoded[2].get_action_type(), AgentActionType::Delete);
    }

    #[test]
    fn test_agent_action_decode_error() {
        // Data too small
        let result = AgentActionEvent::decode(&[0; 10]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too small"));
    }

    #[test]
    fn test_multiplexed_agent_action() {
        let event = AgentActionEvent::new(5, 50, AgentActionType::Link, 750);
        let encoded = event.encode();

        let msg = MultiplexedMessage::decode(&encoded).expect("Decode failed");
        assert_eq!(msg.msg_type, MessageType::AgentAction);
    }

    #[test]
    fn test_message_type_values() {
        // Verify message type constants match spec
        assert_eq!(MessageType::BinaryPositions as u8, 0x00);
        assert_eq!(MessageType::VoiceData as u8, 0x02);
        assert_eq!(MessageType::ControlFrame as u8, 0x03);
        assert_eq!(MessageType::PositionDelta as u8, 0x04);
        assert_eq!(MessageType::AgentAction as u8, 0x23);
        assert_eq!(MessageType::BroadcastAck as u8, 0x34);
    }
}
