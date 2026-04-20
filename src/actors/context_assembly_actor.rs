//! `ContextAssemblyActor` — BC18 workspace-context composer.
//!
//! Wraps the domain `ContextAssemblyService` behind an Actix actor so the
//! Studio HTTP/WS surface can request workspace contexts without holding
//! onto async ports directly. The actor is a thin dispatch shim — the
//! business logic lives in the domain service.
//!
//! Supervised by [`crate::actors::ContributorStudioSupervisor`] per
//! ADR-057 §Actor Topology.
//!
//! Follow-up wiring (not in this commit):
//! - replace stub adapters with real Solid pod / BC2 / BC7 / BC30 adapters
//! - plug into `/api/studio/workspaces/:id/context` HTTP handler
//! - subscribe to Solid Notifications to invalidate cached focus
//! - cache the last focus per workspace keyed on `focus_token()`

use std::sync::Arc;

use actix::prelude::*;
use log::{debug, info, warn};

use crate::domain::contributor::{
    ContextAssemblyError, ContextAssemblyService, EpisodicMemoryPort, GraphSelectionPort,
    OntologyNeighbourPort, PodContributorPort, WorkspaceFocus,
};

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Request a freshly-assembled workspace focus.
#[derive(Message)]
#[rtype(result = "Result<WorkspaceFocus, ContextAssemblyError>")]
pub struct AssembleContext {
    pub webid: String,
    pub workspace_id: String,
    pub episode_limit: usize,
}

impl AssembleContext {
    pub fn new(
        webid: impl Into<String>,
        workspace_id: impl Into<String>,
        episode_limit: usize,
    ) -> Self {
        Self {
            webid: webid.into(),
            workspace_id: workspace_id.into(),
            episode_limit,
        }
    }
}

/// Swap the port bundle at runtime — used by the supervisor when real
/// adapters come online to replace stubs without restarting the actor.
#[derive(Message)]
#[rtype(result = "()")]
pub struct SetContextPorts {
    pub pod: Arc<dyn PodContributorPort>,
    pub graph: Arc<dyn GraphSelectionPort>,
    pub ontology: Arc<dyn OntologyNeighbourPort>,
    pub episodic: Arc<dyn EpisodicMemoryPort>,
}

// ---------------------------------------------------------------------------
// Actor
// ---------------------------------------------------------------------------

pub struct ContextAssemblyActor {
    service: ContextAssemblyService,
    pod: Arc<dyn PodContributorPort>,
    graph: Arc<dyn GraphSelectionPort>,
    ontology: Arc<dyn OntologyNeighbourPort>,
    episodic: Arc<dyn EpisodicMemoryPort>,
}

impl ContextAssemblyActor {
    pub fn new(
        pod: Arc<dyn PodContributorPort>,
        graph: Arc<dyn GraphSelectionPort>,
        ontology: Arc<dyn OntologyNeighbourPort>,
        episodic: Arc<dyn EpisodicMemoryPort>,
    ) -> Self {
        Self {
            service: ContextAssemblyService::new(),
            pod,
            graph,
            ontology,
            episodic,
        }
    }
}

impl Actor for ContextAssemblyActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("[ContextAssemblyActor] started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("[ContextAssemblyActor] stopped");
    }
}

impl Handler<AssembleContext> for ContextAssemblyActor {
    type Result = ResponseFuture<Result<WorkspaceFocus, ContextAssemblyError>>;

    fn handle(&mut self, msg: AssembleContext, _ctx: &mut Self::Context) -> Self::Result {
        let pod = self.pod.clone();
        let graph = self.graph.clone();
        let ontology = self.ontology.clone();
        let episodic = self.episodic.clone();
        // service is stateless — construct a fresh one per call, matches the
        // `Default` contract in `ContextAssemblyService`.
        let service = self.service.clone();
        Box::pin(async move {
            debug!(
                "[ContextAssemblyActor] assembling context webid={} workspace={}",
                msg.webid, msg.workspace_id
            );
            service
                .assemble(
                    &msg.webid,
                    &msg.workspace_id,
                    msg.episode_limit,
                    pod.as_ref(),
                    graph.as_ref(),
                    ontology.as_ref(),
                    episodic.as_ref(),
                )
                .await
        })
    }
}

impl Handler<SetContextPorts> for ContextAssemblyActor {
    type Result = ();

    fn handle(&mut self, msg: SetContextPorts, _ctx: &mut Self::Context) -> Self::Result {
        info!("[ContextAssemblyActor] swapping port bundle");
        self.pod = msg.pod;
        self.graph = msg.graph;
        self.ontology = msg.ontology;
        self.episodic = msg.episodic;
    }
}

#[allow(dead_code)]
fn _supervision_warn_placeholder() {
    warn!("ContextAssemblyActor supervision hook reserved");
}
