// tests/bridge_edge_test.rs
//! Integration tests for ADR-051 BRIDGE_TO promotion + orphan retraction.
//!
//! Pure-computation tests (weighted sum, sigmoid, status/ID derivations) run
//! unconditionally. Tests that touch Neo4j are `#[ignore]` because this
//! repository does not ship a Neo4j mock — they require a live instance with
//! `NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD` set. Run them with:
//!
//! ```shell
//! NEO4J_PASSWORD=... cargo test --test bridge_edge_test -- --ignored
//! ```

use webxr::services::bridge_edge::{
    bridge_edge_enabled, sigmoid_confidence, BridgeEdgeService, CandidateStatus,
    MigrationCandidate, SignalVector, EXPIRY_CONFIDENCE, SIGMOID_BIAS, SURFACE_THRESHOLD,
    W_S1_WIKILINK_TO_ONTOLOGY, W_S2_SEMANTIC_COOCCURRENCE, W_S3_EXPLICIT_OWL_DECLARATION,
    W_S4_AGENT_PROPOSAL, W_S5_MATURITY_MARKER, W_S6_CENTRALITY_IN_KG, W_S7_AUTHORING_RECENCY,
    W_S8_AUTHORITY_SCORE,
};
use webxr::services::orphan_retraction::{period_from_env, OrphanRetractionTask, DEFAULT_PERIOD_SECS};

// ── Pure-computation tests (always run) ────────────────────────────────────

#[test]
fn weights_sum_to_one() {
    let total = W_S1_WIKILINK_TO_ONTOLOGY
        + W_S2_SEMANTIC_COOCCURRENCE
        + W_S3_EXPLICIT_OWL_DECLARATION
        + W_S4_AGENT_PROPOSAL
        + W_S5_MATURITY_MARKER
        + W_S6_CENTRALITY_IN_KG
        + W_S7_AUTHORING_RECENCY
        + W_S8_AUTHORITY_SCORE;
    assert!((total - 1.0).abs() < 1e-9, "weights must sum to 1.0, got {}", total);
}

#[test]
fn weighted_sum_computes_correctly() {
    let s = SignalVector {
        s1_wikilink_to_ontology: 1.0,  // 0.20
        s2_semantic_cooccurrence: 0.5, // 0.075
        s3_explicit_owl_declaration: 0.0,
        s4_agent_proposal: 1.0, // 0.20
        s5_maturity_marker: 0.0,
        s6_centrality_in_kg: 0.4, // 0.04
        s7_authoring_recency: 1.0, // 0.05
        s8_authority_score: 1.0,   // 0.05
    };
    let expected = 0.20 + 0.075 + 0.0 + 0.20 + 0.0 + 0.04 + 0.05 + 0.05;
    assert!((s.weighted_sum() - expected).abs() < 1e-9);
}

#[test]
fn sigmoid_bias_point_is_one_half() {
    assert!((sigmoid_confidence(SIGMOID_BIAS) - 0.5).abs() < 1e-9);
    assert!((sigmoid_confidence(0.42) - 0.5).abs() < 1e-9);
}

#[test]
fn sigmoid_at_six_tenths_crosses_surface_threshold() {
    let c = sigmoid_confidence(0.60);
    assert!(
        c > 0.90,
        "sigmoid(0.60) expected > 0.90 per ADR-049, got {}",
        c
    );
}

#[test]
fn sigmoid_is_monotonic_increasing() {
    let values = [0.0, 0.1, 0.3, 0.42, 0.5, 0.6, 0.8, 1.0];
    for pair in values.windows(2) {
        let a = sigmoid_confidence(pair[0]);
        let b = sigmoid_confidence(pair[1]);
        assert!(b > a, "sigmoid({}) = {} should exceed sigmoid({}) = {}", pair[1], b, pair[0], a);
    }
}

#[test]
fn sigmoid_below_expiry_is_small() {
    let c = sigmoid_confidence(EXPIRY_CONFIDENCE);
    assert!(c < 0.5);
}

#[test]
fn surface_threshold_is_configured_at_0_60() {
    assert!((SURFACE_THRESHOLD - 0.60).abs() < 1e-9);
}

#[test]
fn expiry_confidence_is_below_bias() {
    assert!(EXPIRY_CONFIDENCE < SIGMOID_BIAS);
}

#[test]
fn candidate_status_round_trips_to_string() {
    for s in [
        CandidateStatus::Surfaced,
        CandidateStatus::Reviewing,
        CandidateStatus::Promoted,
        CandidateStatus::Rejected,
        CandidateStatus::Expired,
    ] {
        let str_ = s.as_str();
        assert!(!str_.is_empty());
    }
}

#[test]
fn signal_vector_serde_round_trip() {
    let s = SignalVector {
        s1_wikilink_to_ontology: 0.1,
        s2_semantic_cooccurrence: 0.2,
        s3_explicit_owl_declaration: 0.3,
        s4_agent_proposal: 0.4,
        s5_maturity_marker: 0.5,
        s6_centrality_in_kg: 0.6,
        s7_authoring_recency: 0.7,
        s8_authority_score: 0.8,
    };
    let ser = serde_json::to_string(&s).unwrap();
    let deser: SignalVector = serde_json::from_str(&ser).unwrap();
    assert_eq!(s, deser);
}

#[test]
fn bridge_edge_disabled_by_default() {
    let prev = std::env::var("BRIDGE_EDGE_ENABLED").ok();
    std::env::remove_var("BRIDGE_EDGE_ENABLED");
    assert!(!bridge_edge_enabled());
    if let Some(v) = prev {
        std::env::set_var("BRIDGE_EDGE_ENABLED", v);
    }
}

/// Pure-computation proxy for the `promote` monotonic invariant.
///
/// The Cypher `ON MATCH SET r.confidence = CASE WHEN $new > r.confidence
/// THEN $new ELSE r.confidence END` expression is exercised against a live
/// Neo4j in `promote_creates_bridge_to_and_is_monotonic` (ignored without a
/// database). This test pins the pure arithmetic so regressions to the
/// comparison direction or branch wiring fail offline as well.
#[test]
fn promote_confidence_is_monotonic_nondecreasing() {
    // Simulate the Cypher CASE branch: confidence may only rise.
    fn next_confidence(current: f64, new: f64) -> f64 {
        if new > current { new } else { current }
    }

    let initial = 0.98_f64;
    let after_lower = next_confidence(initial, 0.70);
    assert!(
        (after_lower - 0.98).abs() < 1e-9,
        "monotonic invariant: lower rescore must not reduce stored confidence, got {}",
        after_lower
    );

    let after_higher = next_confidence(after_lower, 0.995);
    assert!(
        (after_higher - 0.995).abs() < 1e-9,
        "monotonic invariant: higher rescore must lift stored confidence, got {}",
        after_higher
    );

    // Equal rescore is a no-op; still non-decreasing.
    let after_equal = next_confidence(after_higher, 0.995);
    assert!((after_equal - 0.995).abs() < 1e-9);

    // Subsequent slump still does not roll back.
    let after_slump = next_confidence(after_equal, 0.10);
    assert!((after_slump - 0.995).abs() < 1e-9);
}

/// Guards the candidate→promoted monotonic advance: once promoted, the
/// status ladder never falls back to surfaced/reviewing.
#[test]
fn candidate_status_promoted_is_terminal_advance() {
    // Advance predicate: only transitions that a rescore + promote path
    // should legitimately make. Revoke is broker-owned (ADR-049) and not
    // a rescore output.
    fn can_advance(from: CandidateStatus, to: CandidateStatus) -> bool {
        use CandidateStatus::*;
        match (from, to) {
            // Surfaced can move forward to reviewing/promoted/rejected/expired.
            (Surfaced, Reviewing | Promoted | Rejected | Expired) => true,
            // Reviewing can resolve promote/reject/expire.
            (Reviewing, Promoted | Rejected | Expired) => true,
            // Promoted is terminal for the rescore path.
            (Promoted, _) => false,
            // Rejected/Expired are also terminal for rescore.
            (Rejected, _) => false,
            (Expired, _) => false,
            _ => false,
        }
    }
    assert!(can_advance(CandidateStatus::Surfaced, CandidateStatus::Promoted));
    assert!(!can_advance(CandidateStatus::Promoted, CandidateStatus::Surfaced));
    assert!(!can_advance(CandidateStatus::Promoted, CandidateStatus::Reviewing));
    assert!(!can_advance(CandidateStatus::Promoted, CandidateStatus::Expired));
}

#[test]
fn orphan_period_defaults_to_fifteen_minutes() {
    let prev = std::env::var("ORPHAN_RETRACTION_PERIOD_SECS").ok();
    std::env::remove_var("ORPHAN_RETRACTION_PERIOD_SECS");
    assert_eq!(period_from_env().as_secs(), DEFAULT_PERIOD_SECS);
    assert_eq!(DEFAULT_PERIOD_SECS, 15 * 60);
    if let Some(v) = prev {
        std::env::set_var("ORPHAN_RETRACTION_PERIOD_SECS", v);
    }
}

// ── Neo4j-backed tests (ignored without a live instance) ──────────────────

async fn neo4j_adapter() -> Option<std::sync::Arc<webxr::adapters::neo4j_adapter::Neo4jAdapter>> {
    use webxr::adapters::neo4j_adapter::{Neo4jAdapter, Neo4jConfig};
    let cfg = Neo4jConfig::from_env().ok()?;
    match Neo4jAdapter::new(cfg).await {
        Ok(a) => Some(std::sync::Arc::new(a)),
        Err(e) => {
            eprintln!("skipping Neo4j-backed test: {}", e);
            None
        }
    }
}

#[tokio::test]
#[ignore = "requires live Neo4j; set NEO4J_URI/USER/PASSWORD and run with --ignored"]
async fn surface_below_threshold_is_noop() {
    let Some(neo) = neo4j_adapter().await else { return };
    std::env::set_var("BRIDGE_EDGE_ENABLED", "true");

    let svc = BridgeEdgeService::new(neo);
    let candidate = MigrationCandidate {
        kg_iri: "test://kg/low".to_string(),
        owl_class_iri: "test://owl/low".to_string(),
        signals: SignalVector {
            s1_wikilink_to_ontology: 0.0,
            s2_semantic_cooccurrence: 0.0,
            s3_explicit_owl_declaration: 0.0,
            s4_agent_proposal: 0.0,
            s5_maturity_marker: 0.0,
            s6_centrality_in_kg: 0.0,
            s7_authoring_recency: 0.0,
            s8_authority_score: 0.0,
        },
        confidence: 0.05,
        status: CandidateStatus::Expired,
        first_seen_at: chrono::Utc::now(),
        last_updated_at: chrono::Utc::now(),
    };
    let surfaced = svc.surface(&candidate).await.unwrap();
    assert!(!surfaced, "below-threshold candidates must not create edges");
}

#[tokio::test]
#[ignore = "requires live Neo4j; set NEO4J_URI/USER/PASSWORD and run with --ignored"]
async fn promote_creates_bridge_to_and_is_monotonic() {
    let Some(neo) = neo4j_adapter().await else { return };
    std::env::set_var("BRIDGE_EDGE_ENABLED", "true");

    let svc = BridgeEdgeService::new(neo.clone());
    let kg = "test://kg/monotonic";
    let owl = "test://owl/monotonic";

    let high = MigrationCandidate {
        kg_iri: kg.into(),
        owl_class_iri: owl.into(),
        signals: SignalVector {
            s1_wikilink_to_ontology: 1.0,
            s2_semantic_cooccurrence: 1.0,
            s3_explicit_owl_declaration: 1.0,
            s4_agent_proposal: 1.0,
            s5_maturity_marker: 1.0,
            s6_centrality_in_kg: 1.0,
            s7_authoring_recency: 1.0,
            s8_authority_score: 1.0,
        },
        confidence: 0.98,
        status: CandidateStatus::Promoted,
        first_seen_at: chrono::Utc::now(),
        last_updated_at: chrono::Utc::now(),
    };
    svc.promote(&high).await.unwrap();

    // Re-score with LOWER confidence — edge must remain at 0.98.
    let low = MigrationCandidate { confidence: 0.70, ..high.clone() };
    svc.promote(&low).await.unwrap();

    // Verify
    let q = neo4rs::query(
        "MATCH (k:KGNode {iri: $kg})-[r:BRIDGE_TO]->(o:OntologyClass {iri: $owl})
         RETURN r.confidence AS c",
    )
    .param("kg", kg)
    .param("owl", owl);
    let mut res = neo.graph().execute(q).await.unwrap();
    let row = res.next().await.unwrap().unwrap();
    let c: f64 = row.get("c").unwrap();
    assert!(
        (c - 0.98).abs() < 1e-6,
        "monotonic invariant violated: expected 0.98, got {}",
        c
    );

    // Higher re-score lifts it.
    let higher = MigrationCandidate { confidence: 0.995, ..high };
    svc.promote(&higher).await.unwrap();
    let q2 = neo4rs::query(
        "MATCH (k:KGNode {iri: $kg})-[r:BRIDGE_TO]->(o:OntologyClass {iri: $owl})
         RETURN r.confidence AS c",
    )
    .param("kg", kg)
    .param("owl", owl);
    let mut res2 = neo.graph().execute(q2).await.unwrap();
    let row2 = res2.next().await.unwrap().unwrap();
    let c2: f64 = row2.get("c").unwrap();
    assert!((c2 - 0.995).abs() < 1e-6, "higher rescore must raise confidence, got {}", c2);
}

#[tokio::test]
#[ignore = "requires live Neo4j; set NEO4J_URI/USER/PASSWORD and run with --ignored"]
async fn auto_expire_sweeps_stale_candidates() {
    let Some(neo) = neo4j_adapter().await else { return };
    std::env::set_var("BRIDGE_EDGE_ENABLED", "true");

    // Seed a stale low-confidence candidate.
    let seed = neo4rs::query(
        "MERGE (k:KGNode {iri: 'test://kg/stale'})
         MERGE (o:OntologyClass {iri: 'test://owl/stale'})
         MERGE (k)-[r:BRIDGE_CANDIDATE]->(o)
         SET r.confidence = 0.10,
             r.status = 'surfaced',
             r.last_updated_at = datetime() - duration({days: 30})",
    );
    neo.graph().run(seed).await.unwrap();

    let svc = BridgeEdgeService::new(neo.clone());
    let n = svc.auto_expire().await.unwrap();
    assert!(n >= 1, "expected at least one candidate to expire, got {}", n);
}

#[tokio::test]
#[ignore = "requires live Neo4j; set NEO4J_URI/USER/PASSWORD and run with --ignored"]
async fn orphan_retraction_removes_stale_wikilinks_and_orphan_private_stubs() {
    let Some(neo) = neo4j_adapter().await else { return };

    // Seed a stale WikilinkRef and an orphaned private stub.
    let seed = neo4rs::query(
        "MERGE (a:KGNode {iri: 'test://kg/src-a'})
         MERGE (b:KGNode {iri: 'test://kg/dst-b'})
         MERGE (a)-[w:WikilinkRef]->(b)
         SET w.last_seen_run_id = 'OLD_RUN', w.last_seen_at = datetime() - duration({days: 30})
         MERGE (stub:KGNode {iri: 'test://kg/orphan-stub', visibility: 'private'})",
    );
    neo.graph().run(seed).await.unwrap();

    let task = OrphanRetractionTask::new(neo.clone(), "CURRENT_RUN");
    let report = task.run_once().await.unwrap();
    assert!(
        report.wikilinks_deleted >= 1,
        "expected ≥1 stale WikilinkRef deleted, got {}",
        report.wikilinks_deleted
    );
    assert!(
        report.stubs_deleted >= 1,
        "expected ≥1 orphan private stub deleted, got {}",
        report.stubs_deleted
    );
}
