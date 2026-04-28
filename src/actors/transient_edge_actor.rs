//! Transient-edge registry + reaper for ADR-059 §4 (beam + gluon).
//!
//! Holds active `BeamEdge`s and `ChargeModulation`s spawned in response to
//! inbound `agent_action` events. A 100 ms tick reaps expired entries.
//! Snapshots are queried by the renderer/binary-encoder so beams appear in
//! the spring graph for `duration_ms` and disappear cleanly.

use crate::agent_events::transient::{BeamEdge, ChargeModulation};
use actix::{Actor, AsyncContext, Context, Handler, Message};
use std::collections::HashMap;
use std::time::Duration;

/// Actor registry of in-flight transient edges + per-agent charge mods.
#[derive(Default)]
pub struct TransientEdgeActor {
    beams: HashMap<String, BeamEdge>,
    /// Keyed on agent_node_id. Only one modulation per agent at a time;
    /// a new action coalesces (last-write-wins, per ADR-059 recommendation #3).
    modulations: HashMap<u32, ChargeModulation>,
}

impl TransientEdgeActor {
    pub fn new() -> Self {
        Self::default()
    }

    fn reap(&mut self) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.beams.retain(|_, b| !b.is_expired(now_ms));
        self.modulations.retain(|_, m| !m.is_expired());
    }
}

impl Actor for TransientEdgeActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        // Bounded reaper: ≤1 ms per tick at 10 Hz; per ADR-059.
        ctx.run_interval(Duration::from_millis(100), |act, _ctx| {
            act.reap();
        });
    }
}

/// Spawn a beam + charge modulation in one message.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SpawnBeam {
    pub beam: BeamEdge,
    pub modulation: ChargeModulation,
}

impl Handler<SpawnBeam> for TransientEdgeActor {
    type Result = ();

    fn handle(&mut self, msg: SpawnBeam, _: &mut Self::Context) -> Self::Result {
        // Coalesce: a new action against the same target replaces any prior
        // modulation on the same agent (ADR-059 backpressure recommendation).
        self.modulations.insert(msg.modulation.agent_node_id, msg.modulation);
        self.beams.insert(msg.beam.edge_id.clone(), msg.beam);
    }
}

/// Query message returning the current transient state.
#[derive(Message)]
#[rtype(result = "TransientSnapshot")]
pub struct GetTransients;

#[derive(Debug, Clone, Default)]
pub struct TransientSnapshot {
    pub beams: Vec<BeamEdge>,
    /// Agent node ID → charge multiplier (only entries with active modulation).
    pub charge_multipliers: HashMap<u32, f32>,
}

impl<A, M> actix::dev::MessageResponse<A, M> for TransientSnapshot
where
    A: Actor,
    M: Message<Result = TransientSnapshot>,
{
    fn handle(self, _ctx: &mut A::Context, tx: Option<actix::dev::OneshotSender<TransientSnapshot>>) {
        if let Some(tx) = tx {
            let _ = tx.send(self);
        }
    }
}

impl Handler<GetTransients> for TransientEdgeActor {
    type Result = TransientSnapshot;

    fn handle(&mut self, _msg: GetTransients, _: &mut Self::Context) -> Self::Result {
        let beams: Vec<BeamEdge> = self.beams.values().cloned().collect();
        let charge_multipliers: HashMap<u32, f32> = self
            .modulations
            .iter()
            .map(|(k, v)| (*k, v.multiplier))
            .collect();
        TransientSnapshot {
            beams,
            charge_multipliers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix::Actor;

    #[actix::test]
    async fn spawn_then_query() {
        let addr = TransientEdgeActor::new().start();
        let beam = BeamEdge::new(1, 2, 0, "#3b82f6".into(), 1000);
        let modulation = ChargeModulation::new(1, 2, 1000);
        addr.send(SpawnBeam { beam, modulation }).await.unwrap();
        let snap = addr.send(GetTransients).await.unwrap();
        assert_eq!(snap.beams.len(), 1);
        assert_eq!(snap.charge_multipliers.get(&1).copied(), Some(1.5));
    }

    #[actix::test]
    async fn reaper_removes_expired() {
        let addr = TransientEdgeActor::new().start();
        let mut beam = BeamEdge::new(1, 2, 0, "#3b82f6".into(), 10);
        beam.spawned_at_ms -= 1000; // already expired
        let mut modulation = ChargeModulation::new(1, 2, 0);
        modulation.spawned_at = std::time::Instant::now() - std::time::Duration::from_secs(1);
        modulation.duration = std::time::Duration::from_millis(0);
        addr.send(SpawnBeam { beam, modulation }).await.unwrap();
        // wait one tick of reaper
        tokio::time::sleep(Duration::from_millis(150)).await;
        let snap = addr.send(GetTransients).await.unwrap();
        assert_eq!(snap.beams.len(), 0);
        assert_eq!(snap.charge_multipliers.len(), 0);
    }
}
