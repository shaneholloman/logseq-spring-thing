//! Server integration test: analytics stream rate-cap and per-source
//! coalescing.
//!
//! Pins (PRD-007 §4.2 / ADR-061 §D2 risk R4 / DDD invariant I02 + I05):
//!   - Per-source rate cap: max 1 broadcast / second / source. Rapid-fire
//!     sends with the same source get coalesced.
//!   - Out-of-order generations are dropped (last-wins by `generation`).
//!   - The rate cap is per-source: a different source emitted in the
//!     same window is NOT dropped.
//!
//! Implementation under test (Workstream A + B):
//!   - `BroadcastAnalyticsUpdate` actor message handled by
//!     `ClientCoordinatorActor`.
//!   - Per-source rate-cap state on the actor (last-emit timestamp +
//!     last-seen generation per source).
//!   - `GetAnalyticsBroadcastCount { source }` test-shim message
//!     exposing the per-source emit counter so this test can assert
//!     coalescing behaviour deterministically.
//!
//! This file will not compile until those messages and types land —
//! RED phase.

use actix::prelude::*;

use webxr::actors::client_coordinator_actor::ClientCoordinatorActor;
use webxr::actors::messages::analytics_update::{
    AnalyticsEntry, AnalyticsSource, AnalyticsUpdate, BroadcastAnalyticsUpdate,
};
use webxr::actors::messages::GetAnalyticsBroadcastCount;

fn cluster_entry(id: u32, cluster_id: u32) -> AnalyticsEntry {
    AnalyticsEntry {
        id,
        cluster_id: Some(cluster_id),
        community_id: None,
        anomaly_score: None,
        sssp_distance: None,
        sssp_parent: None,
    }
}

fn anomaly_entry(id: u32, score: f32) -> AnalyticsEntry {
    AnalyticsEntry {
        id,
        cluster_id: None,
        community_id: None,
        anomaly_score: Some(score),
        sssp_distance: None,
        sssp_parent: None,
    }
}

fn clustering_update(generation: u64) -> AnalyticsUpdate {
    AnalyticsUpdate {
        source: AnalyticsSource::Clustering,
        generation,
        entries: vec![cluster_entry(1, generation as u32)],
    }
}

fn anomaly_update(generation: u64) -> AnalyticsUpdate {
    AnalyticsUpdate {
        source: AnalyticsSource::Anomaly,
        generation,
        entries: vec![anomaly_entry(1, generation as f32 / 100.0)],
    }
}

// ---------------------------------------------------------------------------
// Rate cap: 10 rapid-fire same-source updates coalesce
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn rapid_fire_clustering_updates_are_rate_capped_to_1_per_second() {
    // GIVEN: A live ClientCoordinatorActor.
    let coord = ClientCoordinatorActor::new().start();

    // WHEN: 10 BroadcastAnalyticsUpdate messages are sent in immediate
    // succession with source=Clustering and ascending generation.
    for gen in 1u64..=10 {
        coord.do_send(BroadcastAnalyticsUpdate {
            update: clustering_update(gen),
        });
    }

    // Allow actix to drain the queue. The whole loop completes far
    // inside one second, so the rate cap should suppress most.
    actix_rt::time::sleep(std::time::Duration::from_millis(100)).await;

    // THEN: At most 2 updates were broadcast for clustering (1 from the
    // rate-cap allowance + at most 1 trailing flush). Per ADR-061 §D2
    // risk R4, the cap is `max 1/sec` with coalescing by generation.
    let count = coord
        .send(GetAnalyticsBroadcastCount {
            source: AnalyticsSource::Clustering,
        })
        .await
        .expect("count request must succeed");

    assert!(
        count <= 2,
        "10 rapid-fire clustering updates must coalesce to <= 2 broadcasts \
         within a 100 ms window (got {count})"
    );
    assert!(
        count >= 1,
        "at least 1 broadcast must have been emitted (the rate cap allows the first)"
    );
}

// ---------------------------------------------------------------------------
// Out-of-order generations are dropped server-side at the rate cap
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn out_of_order_generations_are_dropped() {
    // GIVEN: A live ClientCoordinatorActor.
    let coord = ClientCoordinatorActor::new().start();

    // WHEN: We emit gen=5, then gen=3 (out of order — older generation
    // arriving after a newer one).
    coord.do_send(BroadcastAnalyticsUpdate {
        update: clustering_update(5),
    });
    actix_rt::time::sleep(std::time::Duration::from_millis(20)).await;
    coord.do_send(BroadcastAnalyticsUpdate {
        update: clustering_update(3),
    });
    actix_rt::time::sleep(std::time::Duration::from_millis(20)).await;

    // THEN: The high-water mark for clustering is gen=5; gen=3 was dropped.
    // Workstream B's coalescer drops stale generations rather than
    // queueing them. We observe this via the count: only the gen=5
    // broadcast registered (gen=3 was dropped, not coalesced).
    let count = coord
        .send(GetAnalyticsBroadcastCount {
            source: AnalyticsSource::Clustering,
        })
        .await
        .expect("count request must succeed");

    assert_eq!(
        count, 1,
        "gen=5 broadcasts; gen=3 (older) is dropped, not emitted (got {count})"
    );
}

// ---------------------------------------------------------------------------
// Rate cap is PER source — anomaly is NOT throttled by clustering's window
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn rate_cap_is_per_source_anomaly_not_dropped_after_clustering() {
    // GIVEN: A live ClientCoordinatorActor.
    let coord = ClientCoordinatorActor::new().start();

    // WHEN: We emit a clustering update (consuming clustering's
    // 1/sec budget) and immediately follow with an anomaly update.
    coord.do_send(BroadcastAnalyticsUpdate {
        update: clustering_update(1),
    });
    coord.do_send(BroadcastAnalyticsUpdate {
        update: anomaly_update(1),
    });
    actix_rt::time::sleep(std::time::Duration::from_millis(50)).await;

    // THEN: BOTH sources broadcast exactly once. The clustering cap
    // does not throttle the anomaly stream — the rate window is
    // independent per source.
    let clustering_count = coord
        .send(GetAnalyticsBroadcastCount {
            source: AnalyticsSource::Clustering,
        })
        .await
        .expect("count request must succeed");
    let anomaly_count = coord
        .send(GetAnalyticsBroadcastCount {
            source: AnalyticsSource::Anomaly,
        })
        .await
        .expect("count request must succeed");

    assert_eq!(
        clustering_count, 1,
        "exactly 1 clustering broadcast (got {clustering_count})"
    );
    assert_eq!(
        anomaly_count, 1,
        "anomaly is NOT dropped despite a recent clustering broadcast (got {anomaly_count})"
    );
}

// ---------------------------------------------------------------------------
// Cross-source isolation: each source has its own generation counter
// ---------------------------------------------------------------------------

#[actix_rt::test]
async fn each_source_keeps_its_own_generation_high_water() {
    // GIVEN: A live ClientCoordinatorActor.
    let coord = ClientCoordinatorActor::new().start();

    // WHEN: clustering reaches gen=5, then anomaly emits gen=1.
    coord.do_send(BroadcastAnalyticsUpdate {
        update: clustering_update(5),
    });
    actix_rt::time::sleep(std::time::Duration::from_millis(20)).await;
    coord.do_send(BroadcastAnalyticsUpdate {
        update: anomaly_update(1),
    });
    actix_rt::time::sleep(std::time::Duration::from_millis(20)).await;

    // THEN: Anomaly's gen=1 is NOT dropped just because clustering's
    // high-water is gen=5. Generations are tracked per source.
    let anomaly_count = coord
        .send(GetAnalyticsBroadcastCount {
            source: AnalyticsSource::Anomaly,
        })
        .await
        .expect("count request must succeed");
    assert_eq!(
        anomaly_count, 1,
        "anomaly gen=1 is the first anomaly update — must broadcast (got {anomaly_count})"
    );
}
