// tests/v2_field_round_trip.rs
//! PRD-006 P1 / F1 regression: VisionClaw v2 ontology fields must survive a
//! Neo4j write→read round-trip.
//!
//! The bug fixed by F1 was that `Neo4jGraphRepository::load_all_nodes_filtered`
//! omitted v2 columns from its Cypher SELECT and hard-coded `None` for
//! `canonical_iri / visionclaw_uri / rdf_type / same_as / domain / content_hash /
//! quality_score / authority_score / preferred_term / graph_source` when
//! constructing `Node`. The write path in `neo4j_adapter.rs` was already
//! persisting these fields. This test forces the round-trip and asserts every
//! v2 field arrives intact.
//!
//! Convention follows `tests/bridge_edge_test.rs`: the live-Neo4j test is
//! `#[ignore]` and requires `NEO4J_URI / NEO4J_USER / NEO4J_PASSWORD`. Run via:
//!
//! ```shell
//! NEO4J_PASSWORD=... cargo test --test v2_field_round_trip -- --ignored
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use webxr::adapters::neo4j_adapter::{Neo4jAdapter, Neo4jConfig};
use webxr::adapters::neo4j_graph_repository::Neo4jGraphRepository;
use webxr::models::edge::Edge;
use webxr::models::graph::GraphData;
use webxr::models::node::{Node, Visibility};
use webxr::ports::graph_repository::GraphRepository;
use webxr::ports::knowledge_graph_repository::KnowledgeGraphRepository;
use webxr::settings::models::NodeFilterSettings;
use webxr::utils::socket_flow_messages::BinaryNodeData;

/// Test-node id chosen to avoid collisions with seeded fixtures (>1M).
const TEST_NODE_ID: u32 = 4_242_424;

/// Build a Node populated with every v2 field.
fn make_v2_node() -> Node {
    Node {
        id: TEST_NODE_ID,
        metadata_id: "v2-roundtrip-fixture".to_string(),
        label: "Smart Contract".to_string(),
        data: BinaryNodeData {
            node_id: TEST_NODE_ID,
            x: 1.5,
            y: 2.5,
            z: 3.5,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
        },
        x: Some(1.5),
        y: Some(2.5),
        z: Some(3.5),
        vx: Some(0.0),
        vy: Some(0.0),
        vz: Some(0.0),
        mass: Some(1.0),
        size: Some(1.0),
        color: Some("#abcdef".to_string()),
        weight: Some(1.0),
        node_type: Some("page".to_string()),
        group: None,
        metadata: HashMap::new(),
        owl_class_iri: None,
        file_size: 0,
        user_data: None,
        visibility: Visibility::Public,
        owner_pubkey: None,
        opaque_id: None,
        pod_url: None,
        // The v2 ontology fields under test:
        canonical_iri: Some("http://narrativegoldmine.com/ontology#SmartContract".to_string()),
        visionclaw_uri: Some("urn:visionclaw:concept:bc:smart-contract".to_string()),
        rdf_type: Some("owl:Class".to_string()),
        same_as: Some("https://schema.org/SmartContract".to_string()),
        domain: Some("blockchain".to_string()),
        content_hash: Some("sha256-12-deadbeef0001".to_string()),
        quality_score: Some(0.87),
        authority_score: Some(0.73),
        preferred_term: Some("Smart Contract".to_string()),
        graph_source: Some("mainKnowledgeGraph".to_string()),
    }
}

/// Build the writer adapter from environment, or `None` if unconfigured/unreachable.
async fn writer() -> Option<Neo4jAdapter> {
    let cfg = Neo4jConfig::from_env().ok()?;
    match Neo4jAdapter::new(cfg).await {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("skipping v2 round-trip test: {}", e);
            None
        }
    }
}

#[tokio::test]
#[ignore = "requires live Neo4j; set NEO4J_URI/USER/PASSWORD and run with --ignored"]
async fn v2_fields_survive_neo4j_round_trip() {
    let Some(adapter) = writer().await else { return };

    let original = make_v2_node();
    let graph_data = GraphData {
        nodes: vec![original.clone()],
        edges: Vec::<Edge>::new(),
        metadata: Default::default(),
        id_to_metadata: HashMap::new(),
    };

    // Write via the v2-aware path (sets canonical_iri, visionclaw_uri, …).
    adapter
        .save_graph(&graph_data)
        .await
        .expect("save_graph should succeed");

    // Read via the path under test. A fresh repository over the same Graph
    // bypasses any in-memory caches the writer may hold.
    let reader = Neo4jGraphRepository::new(Arc::clone(adapter.graph()));
    let filter = NodeFilterSettings::default();
    let loaded = reader
        .load_all_nodes_filtered(&filter)
        .await
        .expect("load_all_nodes_filtered should succeed");

    let read_back = loaded
        .into_iter()
        .find(|n| n.id == TEST_NODE_ID)
        .expect("test node must round-trip through Neo4j");

    // Identity
    assert_eq!(read_back.metadata_id, original.metadata_id, "metadata_id");
    assert_eq!(read_back.label, original.label, "label");

    // v2 ontology fields — the F1 surface area
    assert_eq!(read_back.canonical_iri, original.canonical_iri, "canonical_iri");
    assert_eq!(read_back.visionclaw_uri, original.visionclaw_uri, "visionclaw_uri");
    assert_eq!(read_back.rdf_type, original.rdf_type, "rdf_type");
    assert_eq!(read_back.same_as, original.same_as, "same_as");
    assert_eq!(read_back.domain, original.domain, "domain");
    assert_eq!(read_back.content_hash, original.content_hash, "content_hash");
    assert_eq!(read_back.preferred_term, original.preferred_term, "preferred_term (v2)");
    assert_eq!(read_back.graph_source, original.graph_source, "graph_source");

    // Numeric scores: tolerate a small float delta from f64↔f32 trips.
    let q_orig = original.quality_score.expect("orig quality_score set");
    let q_read = read_back.quality_score.expect("read-back quality_score must be Some");
    assert!(
        (q_orig - q_read).abs() < 1e-5,
        "quality_score: orig={} read={}",
        q_orig,
        q_read
    );
    let a_orig = original.authority_score.expect("orig authority_score set");
    let a_read = read_back.authority_score.expect("read-back authority_score must be Some");
    assert!(
        (a_orig - a_read).abs() < 1e-5,
        "authority_score: orig={} read={}",
        a_orig,
        a_read
    );
}

#[tokio::test]
#[ignore = "requires live Neo4j; set NEO4J_URI/USER/PASSWORD and run with --ignored"]
async fn v2_absent_fields_round_trip_as_none() {
    // Mirror of the happy-path test but with every v2 field set to None on
    // write. The write path persists empty strings via .unwrap_or_default(),
    // and the read path normalises empty strings back to None — this asserts
    // we don't leak `Some("")` to callers.
    let Some(adapter) = writer().await else { return };

    let mut bare = make_v2_node();
    bare.id = TEST_NODE_ID + 1;
    bare.metadata_id = "v2-roundtrip-bare".to_string();
    bare.canonical_iri = None;
    bare.visionclaw_uri = None;
    bare.rdf_type = None;
    bare.same_as = None;
    bare.domain = None;
    bare.content_hash = None;
    bare.preferred_term = None;
    bare.graph_source = None;

    adapter
        .save_graph(&GraphData {
            nodes: vec![bare.clone()],
            edges: Vec::<Edge>::new(),
            metadata: Default::default(),
            id_to_metadata: HashMap::new(),
        })
        .await
        .expect("save_graph should succeed");

    let reader = Neo4jGraphRepository::new(Arc::clone(adapter.graph()));
    let loaded = reader
        .load_all_nodes_filtered(&NodeFilterSettings::default())
        .await
        .expect("load should succeed");
    let read_back = loaded
        .into_iter()
        .find(|n| n.id == bare.id)
        .expect("bare node must round-trip");

    assert_eq!(read_back.canonical_iri, None, "canonical_iri must normalise empty→None");
    assert_eq!(read_back.visionclaw_uri, None, "visionclaw_uri must normalise empty→None");
    assert_eq!(read_back.rdf_type, None, "rdf_type must normalise empty→None");
    assert_eq!(read_back.same_as, None, "same_as must normalise empty→None");
    assert_eq!(read_back.domain, None, "domain must normalise empty→None");
    assert_eq!(read_back.content_hash, None, "content_hash must normalise empty→None");
    assert_eq!(read_back.preferred_term, None, "preferred_term must normalise empty→None");
    assert_eq!(read_back.graph_source, None, "graph_source must normalise empty→None");
}
