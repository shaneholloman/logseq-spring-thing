//! ADR-060 (Phase 4 of ADR-059) — `PUBKEY_VISIBILITY_FILTER` integration test.
//!
//! Verifies the env-flag-gated drop filter at
//! `src/handlers/socket_flow_handler/position_updates.rs:231-260`.
//!
//! The production code mixes ID flag bits, an analytics layer, and the binary
//! encoder. We extract the *behavioural core* — "given a (node, owner_pubkey)
//! map and a caller pubkey, decide whether to drop or opacify private nodes"
//! — into a tiny harness that mirrors the production conditional. The harness
//! reuses the real `compute_private_opaque_ids` function from
//! `client_coordinator_actor` to ensure we test the same set logic that runs
//! in production.
//!
//! Coverage:
//!   1. Default (PUBKEY_VISIBILITY_FILTER unset) — all nodes pass through;
//!      the opaque set is computed but the encoder opacifies (does not drop).
//!   2. Flag enabled — nodes whose IDs are in `private_opaque_ids` are
//!      dropped from the wire payload.
//!   3. Anonymous caller + flag enabled — public-only graph (every private
//!      node dropped). This is the "fail-closed" behaviour documented in
//!      ADR-060.
//!   4. Owner caller + flag enabled — owner sees their own private nodes.

use std::collections::HashSet;
use webxr::actors::client_coordinator_actor::compute_private_opaque_ids;
use webxr::actors::messages::NodeTypeArrays;

// ---------------------------------------------------------------------------
// Tiny harness that mirrors lines 239-250 of position_updates.rs, decoupled
// from actix/the binary encoder so we can unit-test the filter decision.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct WireNode {
    id: u32,
}

/// Replicates the production conditional verbatim:
///
///   let nodes_to_send = if drop_filter_enabled {
///       all_nodes.iter().filter(|n| !private_opaque_ids.contains(&n.id)).collect()
///   } else {
///       all_nodes.clone()
///   };
fn apply_visibility_filter(
    all_nodes: &[WireNode],
    private_opaque_ids: &HashSet<u32>,
    drop_filter_enabled: bool,
) -> Vec<WireNode> {
    if drop_filter_enabled {
        all_nodes
            .iter()
            .filter(|n| !private_opaque_ids.contains(&n.id))
            .cloned()
            .collect()
    } else {
        all_nodes.to_vec()
    }
}

/// Replicates the env-flag parse from production (`==  "true"` or `== "1"`).
fn read_flag(env_value: Option<&str>) -> bool {
    env_value.map(|v| v == "true" || v == "1").unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

const ALICE_PUBKEY: &str = "alice-pubkey-hex";
const BOB_PUBKEY: &str = "bob-pubkey-hex";

fn make_nta() -> NodeTypeArrays {
    let mut nta = NodeTypeArrays::default();
    // Node 1 owned by Alice, node 2 owned by Bob, nodes 3 & 4 public.
    nta.private_node_owners.insert(1, ALICE_PUBKEY.to_string());
    nta.private_node_owners.insert(2, BOB_PUBKEY.to_string());
    nta
}

fn all_wire_nodes() -> Vec<WireNode> {
    vec![
        WireNode { id: 1 }, // Alice's private
        WireNode { id: 2 }, // Bob's private
        WireNode { id: 3 }, // Public
        WireNode { id: 4 }, // Public
    ]
}

// ---------------------------------------------------------------------------
// 1. Flag unset → all nodes pass through (opacify path).
// ---------------------------------------------------------------------------

#[test]
fn flag_unset_all_nodes_pass_through() {
    let nta = make_nta();
    let nodes = all_wire_nodes();
    let opaque_ids = compute_private_opaque_ids(&nta, Some(ALICE_PUBKEY));
    let flag = read_flag(None);
    assert!(!flag, "default flag must be off");

    let out = apply_visibility_filter(&nodes, &opaque_ids, flag);
    assert_eq!(out.len(), nodes.len(), "no nodes dropped when flag is off");
    assert_eq!(out, nodes, "input order is preserved");
}

// ---------------------------------------------------------------------------
// 2. Flag enabled → private-of-others nodes are dropped.
// ---------------------------------------------------------------------------

#[test]
fn flag_enabled_drops_private_of_others() {
    let nta = make_nta();
    let nodes = all_wire_nodes();
    // Alice is the caller. Bob's private (id=2) should be dropped, Alice's
    // own private (id=1) should be kept, public (3, 4) kept.
    let opaque_ids = compute_private_opaque_ids(&nta, Some(ALICE_PUBKEY));
    let flag = read_flag(Some("true"));
    assert!(flag);

    let out = apply_visibility_filter(&nodes, &opaque_ids, flag);
    let out_ids: HashSet<u32> = out.iter().map(|n| n.id).collect();
    assert!(out_ids.contains(&1), "owner sees their own private node");
    assert!(!out_ids.contains(&2), "private-of-other dropped");
    assert!(out_ids.contains(&3), "public retained");
    assert!(out_ids.contains(&4), "public retained");
    assert_eq!(out.len(), 3);
}

#[test]
fn flag_value_one_also_enables() {
    // Production parses both "true" and "1".
    assert!(read_flag(Some("1")));
    assert!(read_flag(Some("true")));
    assert!(!read_flag(Some("0")));
    assert!(!read_flag(Some("false")));
    assert!(!read_flag(Some("yes"))); // strict parse — "yes" is NOT enabled
    assert!(!read_flag(None));
}

// ---------------------------------------------------------------------------
// 3. Anonymous caller + flag enabled → public-only graph (fail-closed).
// ---------------------------------------------------------------------------

#[test]
fn anonymous_caller_with_flag_yields_public_only() {
    let nta = make_nta();
    let nodes = all_wire_nodes();
    // Anonymous caller — no pubkey. Every private node becomes opaque, then
    // the drop filter strips them from the wire.
    let opaque_ids = compute_private_opaque_ids(&nta, None);
    let flag = read_flag(Some("true"));

    let out = apply_visibility_filter(&nodes, &opaque_ids, flag);
    let out_ids: HashSet<u32> = out.iter().map(|n| n.id).collect();
    assert!(!out_ids.contains(&1), "Alice's private dropped (anon caller)");
    assert!(!out_ids.contains(&2), "Bob's private dropped (anon caller)");
    assert!(out_ids.contains(&3), "public retained");
    assert!(out_ids.contains(&4), "public retained");
    assert_eq!(out.len(), 2);
}

#[test]
fn anonymous_caller_without_flag_sees_everything_opacified() {
    // Document the baseline: without the flag, anonymous callers receive
    // every private node — the binary encoder opacifies them via bit 29
    // (ADR-050 H2) but does NOT drop them from the wire.
    let nta = make_nta();
    let nodes = all_wire_nodes();
    let opaque_ids = compute_private_opaque_ids(&nta, None);
    assert_eq!(opaque_ids.len(), 2, "both private nodes are opacified");
    let flag = read_flag(None);

    let out = apply_visibility_filter(&nodes, &opaque_ids, flag);
    assert_eq!(out.len(), nodes.len(), "encoder opacifies, does not drop");
}

// ---------------------------------------------------------------------------
// 4. Owner-of-everything caller + flag enabled → no drops.
// ---------------------------------------------------------------------------

#[test]
fn owner_of_all_private_nodes_sees_everything() {
    // Construct a graph where Alice owns BOTH private nodes.
    let mut nta = NodeTypeArrays::default();
    nta.private_node_owners.insert(1, ALICE_PUBKEY.to_string());
    nta.private_node_owners.insert(2, ALICE_PUBKEY.to_string());

    let nodes = all_wire_nodes();
    let opaque_ids = compute_private_opaque_ids(&nta, Some(ALICE_PUBKEY));
    assert!(opaque_ids.is_empty(), "owner has nothing opacified");
    let flag = read_flag(Some("true"));

    let out = apply_visibility_filter(&nodes, &opaque_ids, flag);
    assert_eq!(out.len(), nodes.len(), "owner sees full graph even with flag on");
}

#[test]
fn empty_pubkey_caller_treated_as_anonymous() {
    // ADR-050 documents: caller with `Some("")` (empty pubkey) is treated
    // identically to `None` — every private node is opacified.
    let nta = make_nta();
    let nodes = all_wire_nodes();
    let opaque_ids = compute_private_opaque_ids(&nta, Some(""));
    let flag = read_flag(Some("true"));

    let out = apply_visibility_filter(&nodes, &opaque_ids, flag);
    let out_ids: HashSet<u32> = out.iter().map(|n| n.id).collect();
    assert!(!out_ids.contains(&1));
    assert!(!out_ids.contains(&2));
    assert_eq!(out.len(), 2, "public-only when caller is empty pubkey");
}
