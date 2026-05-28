//! Wire-protocol encode/decode for VisionClaw.
//!
//! ADR-090 Phase A5: extracted from the webxr monolith.
//!
//! # Modules
//!
//! - [`protocol`]  — V3 binary frame (28-byte broadcast format, ADR-02 D1/D4).
//!   Used by the `/wss` broadcast path and `GET /api/graph/positions`.
//! - [`protocols`] — Binary settings protocol with delta encoding + zlib compression.
//! - [`binary_protocol`] — Node-flag bit helpers (agent/knowledge/ontology flags),
//!   V3/V4/V5 wire encode/decode, `BinaryProtocol` multiplexed-message decoder.
//!   Uses [`visionclaw_domain::BinaryNodeData`] as the canonical data type.
//!
//! # Webxr shim
//!
//! `src/utils/binary_protocol.rs` remains in the webxr crate because it depends on
//! `socket_flow_messages::BinaryNodeDataClient` (a webxr-local type). That file is NOT
//! shadowed; the new crate's `binary_protocol` module is the domain-aligned version.

pub mod protocol;
pub mod protocols;
pub mod binary_protocol;

// Convenience re-exports.
pub use protocol::v3_frame::{BinaryV3Frame, NodeRow, V3DecodeError, V3_MAGIC, V3_NODE_BYTES};
pub use protocols::binary_settings_protocol::{
    BinaryMessage, BinarySettingsProtocol, BinaryValue, PathRegistry,
};
pub use binary_protocol::{
    BinaryNodeDataWireExt,
    BinaryProtocol, ControlFrame, DeltaNodeData, Message, MessageType, MultiplexedMessage,
    NodeType, ProtocolError, WireNodeDataItem, WireNodeDataItemV3,
    // Flag helpers
    clear_agent_flag, clear_all_flags, from_wire_id, from_wire_id_v2,
    get_actual_node_id, get_node_type,
    is_agent_node, is_knowledge_node, is_ontology_class, is_ontology_individual,
    is_ontology_node, is_ontology_property,
    needs_v2_protocol,
    set_agent_flag, set_knowledge_flag, set_ontology_class_flag,
    set_ontology_individual_flag, set_ontology_property_flag,
    to_wire_id, to_wire_id_v2,
    // Encode/decode functions (operate on visionclaw_domain::BinaryNodeData)
    calculate_message_size,
    decode_node_data,
    encode_node_data,
    encode_node_data_extended,
    encode_node_data_extended_with_sssp,
    encode_node_data_with_all,
    encode_node_data_with_analytics,
    encode_node_data_with_flags,
    encode_node_data_with_live_analytics,
    encode_node_data_with_types,
};
