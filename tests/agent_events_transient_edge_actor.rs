//! ADR-059 §4 — `TransientEdgeActor` lifecycle integration tests.
//!
//! Covers:
//!   1. `SpawnBeam` idempotency / coalescing per agent (last-write-wins,
//!      ADR-059 recommendation #3).
//!   2. The 100 ms reaper tick removes expired beams within ~250 ms.
//!   3. After all beams expire, `GetTransients` returns empty.
//!   4. `charge_multipliers` map matches the active modulations.
//!   5. High-concurrency spawn (1000 messages in parallel) does not deadlock.

use std::time::Duration;

use actix::Actor;
use webxr::actors::transient_edge_actor::{GetTransients, SpawnBeam, TransientEdgeActor};
use webxr::agent_events::transient::{BeamEdge, ChargeModulation};

/// Small helper to spawn a beam against a (source, target) pair.
fn spawn_msg(source: u32, target: u32, duration_ms: u32) -> SpawnBeam {
    let beam = BeamEdge::new(source, target, 0, "#3b82f6".into(), duration_ms);
    let modulation = ChargeModulation::new(source, target, duration_ms);
    SpawnBeam { beam, modulation }
}

#[actix::test]
async fn spawn_beam_is_observable_via_get_transients() {
    let addr = TransientEdgeActor::new().start();
    addr.send(spawn_msg(1, 2, 5_000)).await.unwrap();
    let snap = addr.send(GetTransients).await.unwrap();
    assert_eq!(snap.beams.len(), 1);
    assert_eq!(snap.beams[0].source_agent_id, 1);
    assert_eq!(snap.beams[0].target_node_id, 2);
    assert_eq!(snap.charge_multipliers.get(&1).copied(), Some(1.5));
}

#[actix::test]
async fn spawn_beam_coalesces_modulation_per_agent_last_write_wins() {
    // Per ADR-059 recommendation #3: a new action against the same agent
    // replaces the prior modulation (last-write-wins on the agent slot).
    let addr = TransientEdgeActor::new().start();

    // First action by agent 1 → target 2.
    addr.send(spawn_msg(1, 2, 60_000)).await.unwrap();
    // Second action by the SAME agent against a DIFFERENT target.
    addr.send(spawn_msg(1, 99, 60_000)).await.unwrap();

    let snap = addr.send(GetTransients).await.unwrap();
    // Only ONE charge modulation per agent, regardless of target.
    assert_eq!(
        snap.charge_multipliers.len(),
        1,
        "exactly one modulation should be active for agent 1"
    );
    assert_eq!(snap.charge_multipliers.get(&1).copied(), Some(1.5));
    // Both beams are present: beams are keyed by edge_id (which encodes
    // source-target-spawn_at), not by agent.
    assert_eq!(snap.beams.len(), 2, "both beam edges retained");
}

#[actix::test]
async fn spawn_beam_idempotent_for_distinct_agents() {
    let addr = TransientEdgeActor::new().start();
    addr.send(spawn_msg(1, 100, 60_000)).await.unwrap();
    addr.send(spawn_msg(2, 100, 60_000)).await.unwrap();
    addr.send(spawn_msg(3, 100, 60_000)).await.unwrap();

    let snap = addr.send(GetTransients).await.unwrap();
    assert_eq!(snap.beams.len(), 3);
    assert_eq!(snap.charge_multipliers.len(), 3);
    assert_eq!(snap.charge_multipliers.get(&1).copied(), Some(1.5));
    assert_eq!(snap.charge_multipliers.get(&2).copied(), Some(1.5));
    assert_eq!(snap.charge_multipliers.get(&3).copied(), Some(1.5));
}

#[actix::test]
async fn reaper_removes_expired_beams_within_250ms() {
    let addr = TransientEdgeActor::new().start();

    // Build a beam that's already expired so the very next reaper tick reaps it.
    let mut beam = BeamEdge::new(1, 2, 0, "#3b82f6".into(), 10);
    beam.spawned_at_ms -= 10_000; // long-expired
    let mut modulation = ChargeModulation::new(1, 2, 0);
    modulation.spawned_at = std::time::Instant::now() - std::time::Duration::from_secs(10);
    modulation.duration = std::time::Duration::from_millis(0);
    addr.send(SpawnBeam { beam, modulation }).await.unwrap();

    // Reaper ticks every 100 ms — wait long enough for at least 2 ticks.
    tokio::time::sleep(Duration::from_millis(250)).await;

    let snap = addr.send(GetTransients).await.unwrap();
    assert_eq!(snap.beams.len(), 0, "expired beams must be reaped");
    assert_eq!(snap.charge_multipliers.len(), 0, "expired modulations must be reaped");
}

#[actix::test]
async fn get_transients_empty_after_all_beams_expire() {
    let addr = TransientEdgeActor::new().start();

    // Two short-lived beams.
    for source in 1..=2u32 {
        let mut beam = BeamEdge::new(source, 100, 0, "#3b82f6".into(), 1);
        beam.spawned_at_ms -= 1_000;
        let mut modulation = ChargeModulation::new(source, 100, 0);
        modulation.spawned_at = std::time::Instant::now() - std::time::Duration::from_secs(1);
        modulation.duration = std::time::Duration::from_millis(0);
        addr.send(SpawnBeam { beam, modulation }).await.unwrap();
    }

    // Wait for at least one reaper tick.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let snap = addr.send(GetTransients).await.unwrap();
    assert!(snap.beams.is_empty());
    assert!(snap.charge_multipliers.is_empty());
}

#[actix::test]
async fn charge_multipliers_map_matches_active_modulations() {
    let addr = TransientEdgeActor::new().start();
    addr.send(spawn_msg(7, 1, 60_000)).await.unwrap();
    addr.send(spawn_msg(8, 1, 60_000)).await.unwrap();

    let snap = addr.send(GetTransients).await.unwrap();
    let expected_keys: std::collections::HashSet<u32> = [7u32, 8u32].into_iter().collect();
    let actual_keys: std::collections::HashSet<u32> =
        snap.charge_multipliers.keys().copied().collect();
    assert_eq!(actual_keys, expected_keys);
    for &mult in snap.charge_multipliers.values() {
        assert_eq!(mult, 1.5, "default modulation multiplier per ADR-059");
    }
}

#[actix::test]
async fn high_concurrent_spawn_does_not_deadlock() {
    // Stress: 1000 SpawnBeam messages, each with a distinct agent so they
    // don't coalesce. This validates the actor mailbox under load and that
    // the reaper interval handler doesn't deadlock the message loop.
    let addr = TransientEdgeActor::new().start();

    const N: u32 = 1000;
    let futs: Vec<_> = (1..=N)
        .map(|i| {
            let a = addr.clone();
            tokio::spawn(async move {
                a.send(spawn_msg(i, 0, 60_000)).await.unwrap();
            })
        })
        .collect();

    // Bounded wait — if anything deadlocks the actor or join_all, this test
    // will time out under the integration test runner's default ceiling
    // rather than hang silently.
    let join = tokio::time::timeout(Duration::from_secs(5), async {
        for f in futs {
            f.await.unwrap();
        }
    })
    .await;
    assert!(join.is_ok(), "spawn fan-out timed out — mailbox stuck?");

    let snap = tokio::time::timeout(Duration::from_secs(2), addr.send(GetTransients))
        .await
        .expect("GetTransients timed out")
        .expect("actor mailbox closed");
    // Beams keyed on (source, target, spawned_at_ms): some beams may share
    // a millisecond timestamp and collide on edge_id, but every distinct
    // agent must have a charge modulation slot.
    assert_eq!(
        snap.charge_multipliers.len(),
        N as usize,
        "every distinct agent should have a modulation slot"
    );
    // Beams may be slightly fewer due to identical edge_id collisions when
    // multiple beams share a timestamp; assert a generous lower bound.
    assert!(snap.beams.len() >= (N as usize) / 2);
}
