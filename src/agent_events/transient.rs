//! Transient beam edges for agent-action visualisation (ADR-059 §4).
//!
//! Beam edges are spawned on inbound `agent_action` and reaped after
//! `duration_ms`. They are NOT persisted to Neo4j and NOT returned by
//! `load_all_edges()` — they live entirely in the runtime spring graph.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// A transient edge between an agent and a target node, plus a charge
/// modulation that pulls the agent toward the target for the duration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamEdge {
    pub edge_id: String,
    pub source_agent_id: u32,
    pub target_node_id: u32,
    pub action_type: u8,
    pub color: String,
    pub spawned_at_ms: i64,
    pub duration_ms: u32,
}

impl BeamEdge {
    pub fn new(
        source_agent_id: u32,
        target_node_id: u32,
        action_type: u8,
        color: String,
        duration_ms: u32,
    ) -> Self {
        let spawned_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            edge_id: format!("beam-{}-{}-{}", source_agent_id, target_node_id, spawned_at_ms),
            source_agent_id,
            target_node_id,
            action_type,
            color,
            spawned_at_ms,
            duration_ms,
        }
    }

    /// Returns true if this beam has outlived its duration and should be reaped.
    pub fn is_expired(&self, now_ms: i64) -> bool {
        now_ms - self.spawned_at_ms >= self.duration_ms as i64
    }
}

/// Per-agent charge modulation that pulls the agent toward a target node
/// for the duration of an action. Modulates the per-node `class_charge`
/// buffer consumed by `semantic_forces_actor.rs`.
#[derive(Debug, Clone)]
pub struct ChargeModulation {
    pub agent_node_id: u32,
    pub target_node_id: u32,
    pub multiplier: f32,
    pub spawned_at: Instant,
    pub duration: Duration,
}

impl ChargeModulation {
    pub fn new(agent_node_id: u32, target_node_id: u32, duration_ms: u32) -> Self {
        Self {
            agent_node_id,
            target_node_id,
            // Empirical: 1.5× class_charge gives a visible drift over a 250ms beam
            // without perturbing nearby capsules. Tunable via settings later.
            multiplier: 1.5,
            spawned_at: Instant::now(),
            duration: Duration::from_millis(duration_ms as u64),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.spawned_at.elapsed() >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_expires() {
        let mut beam = BeamEdge::new(1, 2, 0, "#3b82f6".into(), 100);
        // Pretend we spawned in the past.
        beam.spawned_at_ms -= 200;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!(beam.is_expired(now));
    }

    #[test]
    fn charge_mod_expires() {
        let m = ChargeModulation::new(1, 2, 0);
        std::thread::sleep(Duration::from_millis(5));
        assert!(m.is_expired());
    }
}
