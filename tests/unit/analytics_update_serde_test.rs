//! Server unit tests for the `analytics_update` side-channel JSON payload.
//!
//! Pins (PRD-007 §4.2 / ADR-061 §D2 / DDD aggregate `AnalyticsUpdate`):
//!   - `type`: "analytics_update"
//!   - `source`: one of {"clustering", "community", "anomaly", "sssp"}
//!   - `generation`: u64, monotonic per source
//!   - `entries`: array of `AnalyticsEntry` with optional per-source fields
//!     using `#[serde(skip_serializing_if = "Option::is_none")]` so a
//!     clustering update emits ONLY `cluster_id`, etc.
//!
//! Implementation under test (Workstream B / A): the new types
//! `webxr::actors::messages::analytics_update::{AnalyticsUpdate,
//! AnalyticsSource, AnalyticsEntry, BroadcastAnalyticsUpdate}`. This file
//! will not compile until those land — RED phase.

use webxr::actors::messages::analytics_update::{
    AnalyticsEntry, AnalyticsSource, AnalyticsUpdate, BroadcastAnalyticsUpdate,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn entry_only_cluster(id: u32, cluster_id: u32) -> AnalyticsEntry {
    AnalyticsEntry {
        id,
        cluster_id: Some(cluster_id),
        community_id: None,
        anomaly_score: None,
        sssp_distance: None,
        sssp_parent: None,
    }
}

fn entry_only_community(id: u32, community_id: u32) -> AnalyticsEntry {
    AnalyticsEntry {
        id,
        cluster_id: None,
        community_id: Some(community_id),
        anomaly_score: None,
        sssp_distance: None,
        sssp_parent: None,
    }
}

fn entry_only_anomaly(id: u32, anomaly_score: f32) -> AnalyticsEntry {
    AnalyticsEntry {
        id,
        cluster_id: None,
        community_id: None,
        anomaly_score: Some(anomaly_score),
        sssp_distance: None,
        sssp_parent: None,
    }
}

fn entry_only_sssp(id: u32, distance: f32, parent: i32) -> AnalyticsEntry {
    AnalyticsEntry {
        id,
        cluster_id: None,
        community_id: None,
        anomaly_score: None,
        sssp_distance: Some(distance),
        sssp_parent: Some(parent),
    }
}

// ---------------------------------------------------------------------------
// Per-source serialisation tests
// ---------------------------------------------------------------------------

#[test]
fn clustering_update_round_trips_and_omits_other_fields() {
    // GIVEN: A clustering update with one entry carrying only cluster_id.
    let update = AnalyticsUpdate {
        source: AnalyticsSource::Clustering,
        generation: 7,
        entries: vec![entry_only_cluster(1, 2)],
    };

    // WHEN: Serialised to JSON.
    let json = serde_json::to_string(&update).expect("clustering update must serialise");

    // THEN: Wire envelope tags are correct.
    assert!(
        json.contains("\"type\":\"analytics_update\""),
        "envelope must declare type=\"analytics_update\", got: {json}"
    );
    assert!(
        json.contains("\"source\":\"clustering\""),
        "source must serialise as kebab/lowercase string, got: {json}"
    );
    assert!(
        json.contains("\"generation\":7"),
        "generation must be a JSON number, got: {json}"
    );
    assert!(
        json.contains("\"id\":1"),
        "entry id must serialise as a JSON number, got: {json}"
    );
    assert!(
        json.contains("\"cluster_id\":2"),
        "clustering source must include cluster_id, got: {json}"
    );

    // THEN: skip_serializing_if = "Option::is_none" — none of the other
    // optional analytics fields appear on the wire for this source.
    assert!(
        !json.contains("community_id"),
        "clustering update must NOT serialise community_id field, got: {json}"
    );
    assert!(
        !json.contains("anomaly_score"),
        "clustering update must NOT serialise anomaly_score field, got: {json}"
    );
    assert!(
        !json.contains("sssp_distance"),
        "clustering update must NOT serialise sssp_distance field, got: {json}"
    );
    assert!(
        !json.contains("sssp_parent"),
        "clustering update must NOT serialise sssp_parent field, got: {json}"
    );

    // THEN: Round-trip recovers structurally identical data.
    let back: AnalyticsUpdate = serde_json::from_str(&json).expect("must deserialise");
    assert_eq!(back.source, AnalyticsSource::Clustering);
    assert_eq!(back.generation, 7);
    assert_eq!(back.entries.len(), 1);
    assert_eq!(back.entries[0].id, 1);
    assert_eq!(back.entries[0].cluster_id, Some(2));
    assert_eq!(back.entries[0].community_id, None);
    assert_eq!(back.entries[0].anomaly_score, None);
    assert_eq!(back.entries[0].sssp_distance, None);
    assert_eq!(back.entries[0].sssp_parent, None);
}

#[test]
fn community_update_round_trips_and_omits_other_fields() {
    let update = AnalyticsUpdate {
        source: AnalyticsSource::Community,
        generation: 11,
        entries: vec![entry_only_community(99, 5)],
    };
    let json = serde_json::to_string(&update).unwrap();

    assert!(json.contains("\"type\":\"analytics_update\""));
    assert!(json.contains("\"source\":\"community\""));
    assert!(json.contains("\"generation\":11"));
    assert!(json.contains("\"community_id\":5"));

    assert!(!json.contains("cluster_id"));
    assert!(!json.contains("anomaly_score"));
    assert!(!json.contains("sssp_distance"));
    assert!(!json.contains("sssp_parent"));

    let back: AnalyticsUpdate = serde_json::from_str(&json).unwrap();
    assert_eq!(back.source, AnalyticsSource::Community);
    assert_eq!(back.entries[0].community_id, Some(5));
    assert_eq!(back.entries[0].cluster_id, None);
}

#[test]
fn anomaly_update_round_trips_and_omits_other_fields() {
    let update = AnalyticsUpdate {
        source: AnalyticsSource::Anomaly,
        generation: 1,
        entries: vec![entry_only_anomaly(42, 0.875)],
    };
    let json = serde_json::to_string(&update).unwrap();

    assert!(json.contains("\"type\":\"analytics_update\""));
    assert!(json.contains("\"source\":\"anomaly\""));
    assert!(json.contains("\"generation\":1"));
    assert!(json.contains("\"anomaly_score\":0.875"));

    assert!(!json.contains("cluster_id"));
    assert!(!json.contains("community_id"));
    assert!(!json.contains("sssp_distance"));
    assert!(!json.contains("sssp_parent"));

    let back: AnalyticsUpdate = serde_json::from_str(&json).unwrap();
    assert_eq!(back.source, AnalyticsSource::Anomaly);
    assert!((back.entries[0].anomaly_score.unwrap() - 0.875).abs() < 1e-6);
}

#[test]
fn sssp_update_round_trips_with_distance_and_parent_only() {
    let update = AnalyticsUpdate {
        source: AnalyticsSource::Sssp,
        generation: 3,
        entries: vec![entry_only_sssp(7, 12.5, 4)],
    };
    let json = serde_json::to_string(&update).unwrap();

    assert!(json.contains("\"type\":\"analytics_update\""));
    assert!(json.contains("\"source\":\"sssp\""));
    assert!(json.contains("\"generation\":3"));
    assert!(json.contains("\"sssp_distance\":12.5"));
    assert!(json.contains("\"sssp_parent\":4"));

    assert!(!json.contains("cluster_id"));
    assert!(!json.contains("community_id"));
    assert!(!json.contains("anomaly_score"));

    let back: AnalyticsUpdate = serde_json::from_str(&json).unwrap();
    assert_eq!(back.source, AnalyticsSource::Sssp);
    assert!((back.entries[0].sssp_distance.unwrap() - 12.5).abs() < 1e-6);
    assert_eq!(back.entries[0].sssp_parent, Some(4));
}

// ---------------------------------------------------------------------------
// Multi-entry & negative-parent edge case
// ---------------------------------------------------------------------------

#[test]
fn multi_entry_update_round_trips_in_order() {
    let entries = vec![
        entry_only_cluster(1, 10),
        entry_only_cluster(2, 20),
        entry_only_cluster(3, 30),
    ];
    let update = AnalyticsUpdate {
        source: AnalyticsSource::Clustering,
        generation: 100,
        entries: entries.clone(),
    };
    let json = serde_json::to_string(&update).unwrap();
    let back: AnalyticsUpdate = serde_json::from_str(&json).unwrap();

    assert_eq!(back.entries.len(), 3);
    for (orig, parsed) in entries.iter().zip(back.entries.iter()) {
        assert_eq!(orig.id, parsed.id);
        assert_eq!(orig.cluster_id, parsed.cluster_id);
    }
}

#[test]
fn sssp_parent_negative_one_is_unreachable_sentinel() {
    // GIVEN: SSSP convention — `-1` parent indicates a node not reachable
    // from the source. The wire MUST round-trip the i32 sign correctly.
    let update = AnalyticsUpdate {
        source: AnalyticsSource::Sssp,
        generation: 1,
        entries: vec![entry_only_sssp(7, f32::INFINITY, -1)],
    };
    // INFINITY is not directly representable in JSON; serializers usually
    // either error or emit `null`. The implementation MAY choose to skip
    // emission on infinite distance, but the parent=-1 path MUST round
    // trip. Here we test on a finite distance to keep the round-trip
    // unambiguous.
    let finite = AnalyticsUpdate {
        source: AnalyticsSource::Sssp,
        generation: 1,
        entries: vec![entry_only_sssp(7, 0.0, -1)],
    };
    let json = serde_json::to_string(&finite).unwrap();
    let back: AnalyticsUpdate = serde_json::from_str(&json).unwrap();
    assert_eq!(back.entries[0].sssp_parent, Some(-1));

    // Reference the infinite-distance variant so the compiler does not
    // dead-code-eliminate the construction (informational; not asserted).
    let _ = update;
}

// ---------------------------------------------------------------------------
// `BroadcastAnalyticsUpdate` actor message wrapper
// ---------------------------------------------------------------------------

#[test]
fn broadcast_analytics_update_wraps_the_inner_update() {
    // GIVEN: An `AnalyticsUpdate` ready to fan out from an analytics actor
    // to the `ClientCoordinatorActor`.
    let inner = AnalyticsUpdate {
        source: AnalyticsSource::Clustering,
        generation: 1,
        entries: vec![entry_only_cluster(1, 0)],
    };

    // WHEN: Wrapped in the actor message.
    let msg = BroadcastAnalyticsUpdate {
        update: inner.clone(),
    };

    // THEN: The wrapper carries the update by value (cheap: the entry list
    // is short) and surfaces it to the handler.
    assert_eq!(msg.update.source, inner.source);
    assert_eq!(msg.update.generation, inner.generation);
    assert_eq!(msg.update.entries.len(), 1);
}

// ---------------------------------------------------------------------------
// Source name vocabulary pin
// ---------------------------------------------------------------------------

#[test]
fn analytics_source_serialises_to_the_four_known_names_only() {
    // GIVEN: The four source names defined by ADR-061 §D2.
    let cases = [
        (AnalyticsSource::Clustering, "\"clustering\""),
        (AnalyticsSource::Community, "\"community\""),
        (AnalyticsSource::Anomaly, "\"anomaly\""),
        (AnalyticsSource::Sssp, "\"sssp\""),
    ];

    for (variant, expected) in cases.iter() {
        // WHEN: Serialised on its own.
        let json = serde_json::to_string(variant).expect("source must serialise");

        // THEN: Lowercase string, matching the contract.
        assert_eq!(
            &json, expected,
            "AnalyticsSource::{:?} must serialise to {}",
            variant, expected
        );

        // THEN: Round-trips back to the same variant.
        let back: AnalyticsSource = serde_json::from_str(&json).expect("source must deserialise");
        assert_eq!(&back, variant);
    }
}
