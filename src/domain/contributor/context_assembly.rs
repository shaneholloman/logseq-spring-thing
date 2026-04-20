//! `ContextAssemblyService` — composes a [`WorkspaceFocus`] from:
//!
//! - the contributor's pod (profile, workspace snapshot)    — [`PodContributorPort`]
//! - the current graph selection (BC2)                       — [`GraphSelectionPort`]
//! - ontology neighbours (BC7)                               — [`OntologyNeighbourPort`]
//! - recent episodic memory (BC30 / ADR-030)                 — [`EpisodicMemoryPort`]
//!
//! Side-effect-free and idempotent for a given input cursor. Ports are
//! defined here as traits; each has a stub adapter so the service can be
//! exercised end-to-end from a test. Production adapters live in
//! `src/adapters/` and are wired in by the actor layer.
//!
//! Per DDD §BC18 ACL 10 (GuidanceSession ← substrate), substrate identities
//! never leak through to stratum aggregates — ports return already-projected
//! payloads.

use async_trait::async_trait;
use thiserror::Error;

use super::value_objects::{GraphSelection, WorkspaceFocus};

// ---------------------------------------------------------------------------
// Ports
// ---------------------------------------------------------------------------

/// Pod adapter surface used by BC18. Contributor profile + workspace snapshot
/// come from here; writes go through this port too.
///
/// **STUB SURFACE.** Production adapter lives in `src/adapters/pod_contributor_adapter.rs`
/// (to be authored in a follow-up sprint). The wired pod write-master uses
/// `solid-pod-rs` against `/private/contributor-profile/`, `/private/workspaces/`,
/// `/private/automations/`, `/inbox/` per design doc §2.
#[async_trait]
pub trait PodContributorPort: Send + Sync {
    /// Reads the contributor's `profile.ttl` goals + active projects.
    async fn read_profile_summary(
        &self,
        webid: &str,
    ) -> Result<PodProfileSummary, ContextAssemblyError>;

    /// Reads a workspace snapshot from `/private/workspaces/{id}.jsonld` if
    /// present. `Ok(None)` is the legal "no snapshot yet" response.
    async fn read_workspace_snapshot(
        &self,
        webid: &str,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceSnapshot>, ContextAssemblyError>;
}

/// Graph-selection adapter surface. In production this resolves to the
/// `ActorGraphRepository` + BC2 Neo4j projection.
#[async_trait]
pub trait GraphSelectionPort: Send + Sync {
    async fn current_selection(
        &self,
        webid: &str,
        workspace_id: &str,
    ) -> Result<GraphSelection, ContextAssemblyError>;
}

/// Ontology-neighbour adapter surface. Walks BC7 for k-hop neighbours of the
/// nodes in the current selection. Returns canonical IRIs.
#[async_trait]
pub trait OntologyNeighbourPort: Send + Sync {
    async fn neighbours_for(
        &self,
        node_ids: &[String],
    ) -> Result<Vec<String>, ContextAssemblyError>;
}

/// Episodic-memory adapter surface. Reads the most recent BC30 (agent-memory)
/// entries relevant to the current workspace, PII-redacted.
#[async_trait]
pub trait EpisodicMemoryPort: Send + Sync {
    async fn recent_episodes(
        &self,
        webid: &str,
        limit: usize,
    ) -> Result<Vec<String>, ContextAssemblyError>;
}

// ---------------------------------------------------------------------------
// Port DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct PodProfileSummary {
    pub goals: Vec<String>,
    pub active_projects: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    pub project_ref: Option<String>,
    pub focus_hint: Option<String>,
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum ContextAssemblyError {
    #[error("pod port error: {0}")]
    PodPort(String),
    #[error("graph port error: {0}")]
    GraphPort(String),
    #[error("ontology port error: {0}")]
    OntologyPort(String),
    #[error("episodic memory port error: {0}")]
    EpisodicPort(String),
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// Composes a [`WorkspaceFocus`] from the four substrate ports.
///
/// The service is a zero-sized functional type: state lives in the injected
/// port implementations. This makes it cheap to construct and trivial to
/// test.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContextAssemblyService;

impl ContextAssemblyService {
    pub fn new() -> Self {
        Self {}
    }

    /// Composes the focus. Returns `Err` if any port fails; callers in the
    /// actor layer are responsible for falling back to a cached snapshot or
    /// surfacing the failure to the Studio.
    ///
    /// The episode limit (`episode_limit`) is a guard against unbounded pod
    /// reads; default production value is 20 (see PRD-003 §7.1).
    pub async fn assemble(
        &self,
        webid: &str,
        workspace_id: &str,
        episode_limit: usize,
        pod: &dyn PodContributorPort,
        graph: &dyn GraphSelectionPort,
        ontology: &dyn OntologyNeighbourPort,
        episodic: &dyn EpisodicMemoryPort,
    ) -> Result<WorkspaceFocus, ContextAssemblyError> {
        let _profile = pod.read_profile_summary(webid).await?;
        let snapshot = pod.read_workspace_snapshot(webid, workspace_id).await?;
        let selection = graph.current_selection(webid, workspace_id).await?;
        let ontology_context = ontology.neighbours_for(&selection.node_ids).await?;
        let recent_episodes = episodic.recent_episodes(webid, episode_limit).await?;
        let project_ref = snapshot.as_ref().and_then(|s| s.project_ref.clone());
        Ok(WorkspaceFocus::new(
            project_ref,
            selection,
            ontology_context,
            recent_episodes,
        ))
    }
}

// ---------------------------------------------------------------------------
// Stub adapters — used by the actor until real adapters land.
// ---------------------------------------------------------------------------

/// In-memory stub of [`PodContributorPort`]. Returns empty data unless seeded.
/// Production adapter will translate to/from Solid pod Turtle/JSON-LD over
/// `solid-pod-rs` at `/private/contributor-profile/*` + `/private/workspaces/*`.
#[derive(Debug, Default, Clone)]
pub struct StubPodContributorAdapter {
    pub profile: PodProfileSummary,
    pub snapshot: Option<WorkspaceSnapshot>,
}

#[async_trait]
impl PodContributorPort for StubPodContributorAdapter {
    async fn read_profile_summary(
        &self,
        _webid: &str,
    ) -> Result<PodProfileSummary, ContextAssemblyError> {
        Ok(self.profile.clone())
    }

    async fn read_workspace_snapshot(
        &self,
        _webid: &str,
        _workspace_id: &str,
    ) -> Result<Option<WorkspaceSnapshot>, ContextAssemblyError> {
        Ok(self.snapshot.clone())
    }
}

/// In-memory stub of [`GraphSelectionPort`].
#[derive(Debug, Default, Clone)]
pub struct StubGraphSelectionAdapter {
    pub selection: GraphSelection,
}

#[async_trait]
impl GraphSelectionPort for StubGraphSelectionAdapter {
    async fn current_selection(
        &self,
        _webid: &str,
        _workspace_id: &str,
    ) -> Result<GraphSelection, ContextAssemblyError> {
        Ok(self.selection.clone())
    }
}

/// In-memory stub of [`OntologyNeighbourPort`].
#[derive(Debug, Default, Clone)]
pub struct StubOntologyNeighbourAdapter {
    pub neighbours: Vec<String>,
}

#[async_trait]
impl OntologyNeighbourPort for StubOntologyNeighbourAdapter {
    async fn neighbours_for(
        &self,
        _node_ids: &[String],
    ) -> Result<Vec<String>, ContextAssemblyError> {
        Ok(self.neighbours.clone())
    }
}

/// In-memory stub of [`EpisodicMemoryPort`].
#[derive(Debug, Default, Clone)]
pub struct StubEpisodicMemoryAdapter {
    pub episodes: Vec<String>,
}

#[async_trait]
impl EpisodicMemoryPort for StubEpisodicMemoryAdapter {
    async fn recent_episodes(
        &self,
        _webid: &str,
        limit: usize,
    ) -> Result<Vec<String>, ContextAssemblyError> {
        Ok(self.episodes.iter().take(limit).cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn assemble_with_stubs_produces_full_focus() {
        let pod = StubPodContributorAdapter {
            profile: PodProfileSummary {
                goals: vec!["ship BC18".into()],
                active_projects: vec!["contributor-studio".into()],
            },
            snapshot: Some(WorkspaceSnapshot {
                project_ref: Some("proj:bc18".into()),
                focus_hint: Some("negentropy".into()),
            }),
        };
        let graph = StubGraphSelectionAdapter {
            selection: GraphSelection {
                node_ids: vec!["n1".into(), "n2".into()],
                edge_ids: vec!["e1".into()],
            },
        };
        let ontology = StubOntologyNeighbourAdapter {
            neighbours: vec!["vc:Negentropy".into(), "vc:InformationTheory".into()],
        };
        let episodic = StubEpisodicMemoryAdapter {
            episodes: (0..10).map(|i| format!("episode-{i}")).collect(),
        };

        let svc = ContextAssemblyService::new();
        let focus = svc
            .assemble("did:alice", "ws-1", 5, &pod, &graph, &ontology, &episodic)
            .await
            .unwrap();

        assert_eq!(focus.project_ref.as_deref(), Some("proj:bc18"));
        assert_eq!(focus.graph_selection.node_ids.len(), 2);
        assert_eq!(focus.ontology_context.len(), 2);
        assert_eq!(focus.recent_episodes.len(), 5);
        assert!(!focus.focus_token().is_empty());
    }

    #[tokio::test]
    async fn assemble_with_empty_snapshot_returns_none_project_ref() {
        let pod = StubPodContributorAdapter::default();
        let graph = StubGraphSelectionAdapter::default();
        let ontology = StubOntologyNeighbourAdapter::default();
        let episodic = StubEpisodicMemoryAdapter::default();
        let svc = ContextAssemblyService::new();
        let focus = svc
            .assemble("did:alice", "ws-1", 20, &pod, &graph, &ontology, &episodic)
            .await
            .unwrap();
        assert!(focus.project_ref.is_none());
        assert!(focus.graph_selection.is_empty());
    }
}
