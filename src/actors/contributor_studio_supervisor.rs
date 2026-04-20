//! `ContributorStudioSupervisor` — BC18 supervision root.
//!
//! Sits under `AppSupervisor` per ADR-057 §Actor Topology and supervises the
//! BC18 child actors:
//!
//! - [`ContextAssemblyActor`]        — workspace-context composer
//! - [`OntologyGuidanceActor`]       — Sensei (NudgeComposer stub)
//! - `PartnerOrchestrationActor`     — (future, wired by C4/C5 sprints)
//! - `ShareOrchestratorActor`        — owned by agent C4, registered here when ready
//!
//! Supervision policy: `RestartWithBackoff`, initial 100 ms, cap 5 s, 3× mult
//! — matches the enterprise actor supervision profile used elsewhere in the
//! tree.
//!
//! This supervisor holds the `Addr` handles so API handlers (future) can
//! dispatch without touching the global registry.

use std::sync::Arc;
use std::time::Duration;

use actix::prelude::*;
use log::{error, info, warn};

use crate::actors::context_assembly_actor::ContextAssemblyActor;
use crate::actors::ontology_guidance_actor::OntologyGuidanceActor;
use crate::actors::supervisor::{
    ActorFactory, RegisterActor, SupervisionStrategy, SupervisorActor,
};
use crate::domain::contributor::{
    context_assembly::{
        StubEpisodicMemoryAdapter, StubGraphSelectionAdapter, StubOntologyNeighbourAdapter,
        StubPodContributorAdapter,
    },
    EpisodicMemoryPort, GraphSelectionPort, OntologyNeighbourPort, PodContributorPort,
};
use crate::errors::VisionFlowError;

/// Registry of live child addresses supervised by the studio supervisor.
#[derive(Clone)]
pub struct StudioActorRegistry {
    pub context_assembly: Addr<ContextAssemblyActor>,
    pub ontology_guidance: Addr<OntologyGuidanceActor>,
}

pub struct ContributorStudioSupervisor {
    registry: Option<StudioActorRegistry>,
    parent_supervisor: Option<Addr<SupervisorActor>>,
    /// Bundle of ports injected at spawn; swapped out at runtime via
    /// `SetContextPorts` when real adapters come online.
    pod_port: Arc<dyn PodContributorPort>,
    graph_port: Arc<dyn GraphSelectionPort>,
    ontology_port: Arc<dyn OntologyNeighbourPort>,
    episodic_port: Arc<dyn EpisodicMemoryPort>,
}

impl ContributorStudioSupervisor {
    /// Build a supervisor with stub ports — the default for Phase 1 bring-up
    /// and for tests. Production startup replaces ports via the injection
    /// constructor below.
    pub fn with_stub_ports() -> Self {
        Self {
            registry: None,
            parent_supervisor: None,
            pod_port: Arc::new(StubPodContributorAdapter::default()),
            graph_port: Arc::new(StubGraphSelectionAdapter::default()),
            ontology_port: Arc::new(StubOntologyNeighbourAdapter::default()),
            episodic_port: Arc::new(StubEpisodicMemoryAdapter::default()),
        }
    }

    pub fn with_ports(
        pod: Arc<dyn PodContributorPort>,
        graph: Arc<dyn GraphSelectionPort>,
        ontology: Arc<dyn OntologyNeighbourPort>,
        episodic: Arc<dyn EpisodicMemoryPort>,
    ) -> Self {
        Self {
            registry: None,
            parent_supervisor: None,
            pod_port: pod,
            graph_port: graph,
            ontology_port: ontology,
            episodic_port: episodic,
        }
    }

    pub fn attach_parent(&mut self, parent: Addr<SupervisorActor>) {
        self.parent_supervisor = Some(parent);
    }

    pub fn registry(&self) -> Option<StudioActorRegistry> {
        self.registry.clone()
    }

    /// Register a child with the parent supervisor. Factories here return
    /// opaque handles; the parent supervisor only inspects liveness.
    fn register_with_parent(
        parent: &Addr<SupervisorActor>,
        actor_name: &str,
        factory: ActorFactory,
    ) {
        let msg = RegisterActor {
            actor_name: actor_name.to_string(),
            strategy: SupervisionStrategy::RestartWithBackoff {
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(5),
                multiplier: 3.0,
            },
            max_restart_count: 5,
            restart_window: Duration::from_secs(60),
            actor_factory: Some(factory),
        };
        parent.do_send(msg);
    }
}

impl Actor for ContributorStudioSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("[ContributorStudioSupervisor] started");
        let ctx_actor = ContextAssemblyActor::new(
            self.pod_port.clone(),
            self.graph_port.clone(),
            self.ontology_port.clone(),
            self.episodic_port.clone(),
        )
        .start();
        let guidance_actor = OntologyGuidanceActor::new().start();

        self.registry = Some(StudioActorRegistry {
            context_assembly: ctx_actor,
            ontology_guidance: guidance_actor,
        });

        if let Some(parent) = &self.parent_supervisor {
            let pod = self.pod_port.clone();
            let graph = self.graph_port.clone();
            let ontology = self.ontology_port.clone();
            let episodic = self.episodic_port.clone();
            let ctx_factory: ActorFactory = Arc::new(move || {
                let addr = ContextAssemblyActor::new(
                    pod.clone(),
                    graph.clone(),
                    ontology.clone(),
                    episodic.clone(),
                )
                .start();
                Box::new(addr) as Box<dyn std::any::Any + Send>
            });
            Self::register_with_parent(parent, "ContextAssemblyActor", ctx_factory);

            let guidance_factory: ActorFactory = Arc::new(|| {
                let addr = OntologyGuidanceActor::new().start();
                Box::new(addr) as Box<dyn std::any::Any + Send>
            });
            Self::register_with_parent(parent, "OntologyGuidanceActor", guidance_factory);
        } else {
            warn!("[ContributorStudioSupervisor] no parent supervisor attached — children will not be restarted on failure");
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("[ContributorStudioSupervisor] stopped");
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Fetch the current child registry. Returns `None` until `started()` has
/// finished wiring children.
#[derive(Message)]
#[rtype(result = "Option<StudioActorRegistry>")]
pub struct GetStudioRegistry;

impl Handler<GetStudioRegistry> for ContributorStudioSupervisor {
    type Result = Option<StudioActorRegistry>;

    fn handle(&mut self, _msg: GetStudioRegistry, _ctx: &mut Self::Context) -> Self::Result {
        self.registry.clone()
    }
}

/// Attach a parent supervisor post-start. If children were already spawned
/// without a parent, this registers them retroactively.
#[derive(Message)]
#[rtype(result = "Result<(), VisionFlowError>")]
pub struct AttachParentSupervisor {
    pub parent: Addr<SupervisorActor>,
}

impl Handler<AttachParentSupervisor> for ContributorStudioSupervisor {
    type Result = Result<(), VisionFlowError>;

    fn handle(
        &mut self,
        msg: AttachParentSupervisor,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.parent_supervisor = Some(msg.parent.clone());
        if self.registry.is_some() {
            let pod = self.pod_port.clone();
            let graph = self.graph_port.clone();
            let ontology = self.ontology_port.clone();
            let episodic = self.episodic_port.clone();
            let ctx_factory: ActorFactory = Arc::new(move || {
                let addr = ContextAssemblyActor::new(
                    pod.clone(),
                    graph.clone(),
                    ontology.clone(),
                    episodic.clone(),
                )
                .start();
                Box::new(addr) as Box<dyn std::any::Any + Send>
            });
            Self::register_with_parent(&msg.parent, "ContextAssemblyActor", ctx_factory);

            let guidance_factory: ActorFactory = Arc::new(|| {
                let addr = OntologyGuidanceActor::new().start();
                Box::new(addr) as Box<dyn std::any::Any + Send>
            });
            Self::register_with_parent(&msg.parent, "OntologyGuidanceActor", guidance_factory);
        } else {
            error!("[ContributorStudioSupervisor] AttachParentSupervisor called before children wired");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::context_assembly_actor::AssembleContext;
    use crate::actors::ontology_guidance_actor::ComposeNudge;
    use crate::domain::contributor::{GraphSelection, WorkspaceFocus};

    #[actix::test]
    async fn supervisor_spawns_children_with_stub_ports() {
        let sup = ContributorStudioSupervisor::with_stub_ports().start();
        let registry = sup.send(GetStudioRegistry).await.unwrap();
        assert!(registry.is_some(), "registry populated after started()");
        let registry = registry.unwrap();

        // ContextAssembly works end-to-end with stub ports
        let focus = registry
            .context_assembly
            .send(AssembleContext::new("did:alice", "ws-1", 5))
            .await
            .unwrap()
            .unwrap();
        assert!(focus.graph_selection.is_empty());

        // Guidance actor composes a three-suggestion nudge
        let envelope = registry
            .ontology_guidance
            .send(ComposeNudge::new(
                "sess-1",
                WorkspaceFocus::new(None, GraphSelection::default(), vec![], vec![]),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(envelope.suggestions.len(), 3);
    }

    #[actix::test]
    async fn supervisor_registers_children_with_parent() {
        let parent = SupervisorActor::new("test-app".into()).start();
        let sup = ContributorStudioSupervisor::with_stub_ports().start();
        sup.send(AttachParentSupervisor { parent: parent.clone() })
            .await
            .unwrap()
            .unwrap();
        // `AttachParent` re-registers; the supervisor mailbox processes it
        // synchronously so the parent has seen the RegisterActor messages by
        // the time this test returns. We only assert no panic.
        let status = parent
            .send(crate::actors::supervisor::GetSupervisionStatus)
            .await
            .unwrap()
            .unwrap();
        assert!(status.total_actors >= 2);
    }
}
