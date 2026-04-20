//! Domain layer for DDD bounded contexts.
//!
//! Houses stratum aggregates per `docs/explanation/ddd-contributor-enablement-context.md`.
//! Currently implements BC19 Skill Lifecycle; BC18 Contributor Enablement is agent C1's
//! scope (see `project-state::contributor-nexus-impl-swarm-plan-2026-04-20`).

pub mod skills;
