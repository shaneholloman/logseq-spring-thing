//! BC18 Contributor Enablement — bounded context.
//!
//! Ref: ADR-057 (Contributor Enablement Platform),
//! `docs/explanation/ddd-contributor-enablement-context.md` §BC18,
//! `docs/design/2026-04-20-contributor-studio/`.
//!
//! This module owns the **Core** aggregates that turn day-to-day contributor
//! work into governed institutional assets:
//!
//! | Aggregate | Role |
//! |-----------|------|
//! | [`ContributorWorkspace`] | Aggregate root. Live multi-pane session; owns artefacts, sessions, share intents, focus. |
//! | [`GuidanceSession`] | Append-only "Sensei" episode scoped by a [`WorkspaceFocus`]. |
//! | [`WorkArtifact`] | Pod-resident unit of work advancing through [`ShareState`]. |
//! | [`ShareIntent`] | Stubbed intent (BC18-local model); orchestration lives in C4 `ShareOrchestratorActor`. |
//! | [`ContributorProfile`] | Pod-first profile; Neo4j projection is derived. |
//!
//! Invariants enforced in the aggregates (see DDD §BC18):
//!
//! 1. A `WorkArtifact` has exactly one current `ShareState` and one canonical pod URI.
//! 2. `ShareIntent` transitions are monotonic (`Private → Team → Mesh`). Any downward
//!    move is an auditable revocation, not an edit.
//! 3. A `ShareIntent` targeting `Mesh` MUST be dispatched to exactly one downstream
//!    channel (Broker / Workflow / Migration). (Enforced downstream in C4.)
//! 4. `ContributorProfile` is pod-first; Neo4j is a derived read model.
//! 5. `GuidanceSession`s are append-only.
//! 6. Partner delegation scope is a strict subset of the contributor's session scope.
//! 7. Every artefact lineage roots in a [`ContributorWorkspace`].
//!
//! Public API re-exports the aggregates, value objects and the context-assembly
//! domain service; domain events are re-exported as `events`.

pub mod contributor_profile;
pub mod contributor_workspace;
pub mod context_assembly;
pub mod events;
pub mod guidance_session;
pub mod share_intent;
pub mod value_objects;
pub mod work_artifact;

pub use contributor_profile::ContributorProfile;
pub use contributor_workspace::ContributorWorkspace;
pub use context_assembly::{
    ContextAssemblyError, ContextAssemblyService, EpisodicMemoryPort, GraphSelectionPort,
    OntologyNeighbourPort, PodContributorPort,
};
pub use guidance_session::{GuidanceSession, GuidanceSessionError};
pub use share_intent::{ShareIntent, ShareIntentError, ShareIntentStatus};
pub use value_objects::{
    ArtifactKind, ArtifactLineage, ArtifactRef, GraphSelection, GuidanceSuggestion,
    LineageEntry, NudgeEnvelope, PartnerBinding, PartnerKind, ShareState, SuggestionKind,
    WorkspaceFocus,
};
pub use work_artifact::{WorkArtifact, WorkArtifactError};
