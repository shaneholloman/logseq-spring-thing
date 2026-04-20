//! ADR-050 / ADR-055 Sprint-2: bit 29 reaches the wire.
//!
//! Closes QE audit findings H1 + H2 (docs/audits/2026-04-19-sovereign-mesh-qe/02-privacy.md):
//!
//! - H1: `visibility_allows` path MUST opacify cross-user private nodes
//!       instead of dropping them. Nodes remain in the response body with
//!       `label` / `metadataId` / `metadata` cleared, `opaqueId` populated,
//!       `ownerPubkey` preserved, topology preserved.
//!
//! - H2: `encode_positions_v3_with_privacy` callers MUST pass a real
//!       `HashSet<u32>` of private node ids. Bit 29 (`PRIVATE_OPAQUE_FLAG`,
//!       0x20000000) must appear on the wire id of exactly those nodes,
//!       and must NOT appear on nodes owned by the caller.
//!
//! Both are gated by `SOVEREIGN_SCHEMA=true|false`. H2 asserts the gated
//! vs. ungated behaviour symmetrically.

use std::collections::{HashMap, HashSet};

use webxr::handlers::api_handler::graph::{opacify_for_caller, NodeWithPosition};
use webxr::types::vec3::Vec3Data;
use webxr::utils::binary_protocol::{
    encode_positions_v3_with_privacy, is_private_opaque, node_id_base, sovereign_schema_enabled,
    PRIVATE_OPAQUE_FLAG,
};
use webxr::utils::opaque_id::opaque_id;
use webxr::utils::socket_flow_messages::BinaryNodeData;

const OWNER_ALICE: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const OWNER_BOB: &str = "0000000000000000000000000000000000000000000000000000000000000001";

// ---------------------------------------------------------------------------
// Helpers: build `NodeWithPosition` fixtures for H1 cases.
// ---------------------------------------------------------------------------

fn public_node(id: u32, label: &str) -> NodeWithPosition {
    let mut metadata = HashMap::new();
    metadata.insert("visibility".into(), "public".into());
    NodeWithPosition {
        id,
        metadata_id: format!("public-{}.md", id),
        label: label.to_string(),
        position: Vec3Data::new(id as f32, id as f32 * 2.0, id as f32 * 3.0),
        velocity: Vec3Data::zero(),
        metadata,
        node_type: Some("knowledge".into()),
        size: Some(1.0),
        color: Some("#ffffff".into()),
        weight: Some(0.5),
        group: None,
        owner_pubkey: None,
        opaque_id: None,
        pod_url: None,
    }
}

fn private_node(id: u32, label: &str, owner: &str) -> NodeWithPosition {
    let mut metadata = HashMap::new();
    metadata.insert("visibility".into(), "private".into());
    metadata.insert("owner_pubkey".into(), owner.to_string());
    metadata.insert(
        "canonical_iri".into(),
        format!("visionclaw:owner:{}/kg/{}", owner, id),
    );
    NodeWithPosition {
        id,
        metadata_id: format!("private-{}.md", id),
        label: label.to_string(),
        position: Vec3Data::new(id as f32 * 10.0, id as f32 * 20.0, id as f32 * 30.0),
        velocity: Vec3Data::new(0.1, 0.2, 0.3),
        metadata,
        node_type: Some("knowledge".into()),
        size: Some(1.0),
        color: Some("#ff0000".into()),
        weight: Some(2.0),
        group: None,
        owner_pubkey: Some(owner.to_string()),
        opaque_id: None,
        pod_url: Some(format!("https://pod.example/{}/kg/", owner)),
    }
}

// ===========================================================================
// H1: opacify instead of drop at the REST boundary
// ===========================================================================

/// H1a: anonymous caller + private node -> opacified stub in response.
///
/// `opacify_for_caller` must clear `label` / `metadata_id` / `metadata`,
/// populate `opaque_id`, preserve the position/velocity/shape metrics, and
/// preserve `owner_pubkey` for future delegation. `pod_url` must be cleared
/// so anonymous callers cannot dereference the authoritative payload.
#[test]
fn h1a_anonymous_caller_gets_opacified_stub_not_drop() {
    let salt: &[u8] = b"test-salt-for-h1a-XXXXXXXXXXXX";
    let node = private_node(42, "Top Secret Plan", OWNER_ALICE);

    let opaque = opacify_for_caller(&node, Some(salt));

    // Identifying fields cleared.
    assert_eq!(opaque.label, "");
    assert_eq!(opaque.metadata_id, "");
    assert!(opaque.metadata.is_empty());
    assert!(opaque.pod_url.is_none());

    // Opaque id populated, owner preserved.
    assert!(
        opaque.opaque_id.is_some(),
        "opacified node must carry an opaque id"
    );
    assert_eq!(opaque.owner_pubkey.as_deref(), Some(OWNER_ALICE));

    // Topology preserved (visible per ADR-050 §three-tier).
    assert_eq!(opaque.position, node.position);
    assert_eq!(opaque.velocity, node.velocity);

    // Shape metrics preserved.
    assert_eq!(opaque.size, Some(1.0));
    assert_eq!(opaque.color.as_deref(), Some("#ff0000"));
    assert_eq!(opaque.weight, Some(2.0));
    assert_eq!(opaque.node_type.as_deref(), Some("knowledge"));

    // JSON round-trip: wire shape has cleared label / empty metadata,
    // non-null opaqueId and ownerPubkey, no podUrl.
    let v = serde_json::to_value(&opaque).unwrap();
    assert_eq!(v["label"], "");
    assert_eq!(v["metadataId"], "");
    assert!(v.get("metadata").is_none() || v["metadata"].as_object().map_or(true, |o| o.is_empty()));
    assert!(v["opaqueId"].is_string());
    assert_eq!(v["ownerPubkey"], OWNER_ALICE);
    assert!(v.get("podUrl").is_none());
}

/// H1b: owner caller + own private node -> returned as-is, NOT opacified.
///
/// The graph handler checks `visibility_allows(...)` before calling
/// `opacify_for_caller`. For the owner, visibility_allows returns true and
/// the node is emitted verbatim with its real label and metadata.
#[test]
fn h1b_owner_caller_gets_full_fidelity_not_opacified() {
    // The handler's contract: `visibility_allows(metadata, Some(owner)) == true`
    // when caller == owner, so opacify_for_caller is never invoked for this
    // case. This test pins the contract by checking a non-opacified clone
    // matches the original.
    use webxr::handlers::api_handler::graph::visibility_allows;

    let node = private_node(7, "Alice's real label", OWNER_ALICE);
    assert!(
        visibility_allows(&node.metadata, Some(OWNER_ALICE)),
        "owner must be allowed through the visibility gate"
    );

    // Same `NodeWithPosition` would be surfaced by the handler: no stripping.
    assert_eq!(node.label, "Alice's real label");
    assert_eq!(node.metadata_id, "private-7.md");
    assert!(node.metadata.contains_key("owner_pubkey"));
}

/// H1c: signed caller + OTHER user's private node -> opacified.
///
/// Bob (signed) asks for the graph; Alice owns a private node. Bob must
/// receive an opacified placeholder — `visibility_allows` returns false, the
/// handler routes through `opacify_for_caller`.
#[test]
fn h1c_signed_caller_sees_other_user_private_as_opacified() {
    use webxr::handlers::api_handler::graph::visibility_allows;

    let salt: &[u8] = b"test-salt-for-h1c-XXXXXXXXXXXX";
    let alices_node = private_node(99, "Alice's secret", OWNER_ALICE);

    // Bob is NOT permitted.
    assert!(!visibility_allows(&alices_node.metadata, Some(OWNER_BOB)));

    // Bob receives an opacified clone.
    let opaque = opacify_for_caller(&alices_node, Some(salt));
    assert_eq!(opaque.label, "");
    assert!(opaque.metadata.is_empty());
    assert!(opaque.opaque_id.is_some());
    // Owner preserved so Bob can identify-delegate later.
    assert_eq!(opaque.owner_pubkey.as_deref(), Some(OWNER_ALICE));

    // The opaque id is deterministic: same salt + same owner + same IRI =
    // same id. This matches what the client uses for frame-to-frame diffing.
    let canonical_iri = format!("visionclaw:owner:{}/kg/{}", OWNER_ALICE, 99);
    let expected = opaque_id(salt, OWNER_ALICE, &canonical_iri);
    assert_eq!(opaque.opaque_id.as_deref(), Some(expected.as_str()));
}

// ===========================================================================
// H2: bit 29 on the wire
// ===========================================================================

/// H2a: 3 nodes, 2 owned by caller + 1 cross-user private -> exactly 1 wire id
/// has bit 29 set when SOVEREIGN_SCHEMA=true.
#[test]
fn h2a_three_nodes_mixed_visibility_produces_exactly_one_bit29() {
    // Build 3 bare BinaryNodeData records. Their ids are compact (0..2).
    let nodes: Vec<(u32, BinaryNodeData)> = (0..3u32)
        .map(|id| {
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
        })
        .collect();

    // Node 2 is Alice's private, owned by somebody other than our caller.
    let mut private_set = HashSet::new();
    private_set.insert(2u32);

    let encoded = encode_positions_v3_with_privacy(
        &nodes, &[], &[], &[], &[], &[], None, None, Some(&private_set),
    );

    // Parse the 3 wire ids. V3 frame layout: 1 header + N * 48 bytes, with
    // the id at offset 0 inside each 48-byte node block.
    let payload = &encoded[1..];
    assert_eq!(payload.len(), 3 * 48);

    let wire_ids: Vec<u32> = (0..3)
        .map(|i| {
            let base = i * 48;
            u32::from_le_bytes([
                payload[base], payload[base + 1], payload[base + 2], payload[base + 3],
            ])
        })
        .collect();

    // Count bit-29 hits.
    let flagged = wire_ids.iter().filter(|w| is_private_opaque(**w)).count();

    if sovereign_schema_enabled() {
        assert_eq!(
            flagged, 1,
            "with SOVEREIGN_SCHEMA=true, exactly 1 of 3 wire ids must have bit 29 set (got {}): {:?}",
            flagged, wire_ids
        );
        // And that 1 must be node_id == 2.
        assert!(is_private_opaque(wire_ids[2]));
        assert_eq!(node_id_base(wire_ids[2]), 2);
        // The other two must be clean.
        assert!(!is_private_opaque(wire_ids[0]));
        assert!(!is_private_opaque(wire_ids[1]));
    } else {
        // Feature flag off: bit 29 MUST NOT appear on any id, even though
        // the private set was passed. This preserves legacy wire behaviour.
        assert_eq!(
            flagged, 0,
            "with SOVEREIGN_SCHEMA off, bit 29 must never appear (got {}): {:?}",
            flagged, wire_ids
        );
    }

    // Byte-level proof: under SOVEREIGN_SCHEMA=true, the byte at offset
    // (1 + 2*48 + 3) carries the high nibble of the wire id, which must
    // contain the bit-29 bit (0x20 of byte 3 in little-endian).
    if sovereign_schema_enabled() {
        let node2_id_byte3 = payload[2 * 48 + 3];
        assert_ne!(
            node2_id_byte3 & 0x20,
            0,
            "byte-3 of node2's wire id must have 0x20 set (PRIVATE_OPAQUE_FLAG byte)"
        );
    }
}

/// H2b: when the caller owns every node in the private set (private_set
/// is empty from the caller's perspective), zero bit-29 hits must appear.
#[test]
fn h2b_caller_owns_all_private_no_bit29_set() {
    let nodes: Vec<(u32, BinaryNodeData)> = (0..3u32)
        .map(|id| {
            (
                id,
                BinaryNodeData {
                    node_id: id,
                    x: 0.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0,
                },
            )
        })
        .collect();

    // Empty private set: caller owns (or is authorised for) every node.
    let private_set: HashSet<u32> = HashSet::new();

    let encoded = encode_positions_v3_with_privacy(
        &nodes, &[], &[], &[], &[], &[], None, None, Some(&private_set),
    );

    let payload = &encoded[1..];
    for i in 0..3 {
        let base = i * 48;
        let wire_id = u32::from_le_bytes([
            payload[base], payload[base + 1], payload[base + 2], payload[base + 3],
        ]);
        assert!(
            !is_private_opaque(wire_id),
            "caller-owned node {} must NOT have bit 29 set (wire_id=0x{:08X})",
            i, wire_id
        );
    }
}

/// H2c: opaque-id generator determinism + salt rotation.
///
/// Under a fixed salt, (owner, iri) -> id must be constant. Under a
/// different salt the same (owner, iri) must produce a different id.
/// This is the property the rotating `SessionSalt` relies on for
/// unlinkability across salt windows.
#[test]
fn h2c_opaque_id_is_deterministic_and_rotates_on_salt_change() {
    let owner = OWNER_ALICE;
    let iri = "visionclaw:owner:test/kg/deadbeef";

    let salt_a = b"salt-week-01-XXXXXXXXXXXXXXXX";
    let salt_b = b"salt-week-02-XXXXXXXXXXXXXXXX";

    // Determinism: same (salt, owner, iri) -> identical id.
    let a1 = opaque_id(salt_a, owner, iri);
    let a2 = opaque_id(salt_a, owner, iri);
    assert_eq!(a1, a2, "opaque_id must be deterministic under a fixed salt");

    // Shape: 24 lowercase hex chars.
    assert_eq!(a1.len(), 24);
    assert!(a1.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

    // Rotation: changing the salt produces a different id.
    let b = opaque_id(salt_b, owner, iri);
    assert_ne!(a1, b, "salt rotation must change the opaque id (unlinkability)");

    // Isolation: different owner under same salt -> different id.
    let other = opaque_id(salt_a, OWNER_BOB, iri);
    assert_ne!(a1, other);

    // Isolation: different iri under same salt -> different id.
    let other_iri = opaque_id(salt_a, owner, "visionclaw:owner:test/kg/cafef00d");
    assert_ne!(a1, other_iri);
}

// ===========================================================================
// Cross-check: PRIVATE_OPAQUE_FLAG definition
// ===========================================================================

#[test]
fn private_opaque_flag_is_bit_29_reminder() {
    // Pinning the constant so accidental renumbering breaks this test
    // before it breaks clients.
    assert_eq!(PRIVATE_OPAQUE_FLAG, 0x2000_0000);
    assert_eq!(PRIVATE_OPAQUE_FLAG, 1u32 << 29);
}
