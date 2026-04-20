//! Domain layer for DDD bounded contexts.
//!
//! Each bounded context owns a submodule. Currently houses:
//! - `broker` — BC11 Judgment Broker Workbench (ADR-041/042)
//! - `skills` — BC19 Skill Lifecycle (ADR-057)
//!
//! See `docs/explanation/ddd-contributor-enablement-context.md` and
//! `docs/explanation/ddd-enterprise-contexts.md`.

pub mod broker;
pub mod skills;
