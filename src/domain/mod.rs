//! Domain layer — aggregate modules per DDD bounded contexts.
//!
//! Each submodule is a bounded context with aggregate roots, entities, value
//! objects, domain services, and domain events. Infrastructure and actors
//! live outside this module and treat aggregates as opaque facts.
//!
//! Current contexts:
//! - [`broker`]      — BC11 Judgment Broker Workbench (ADR-041/042)
//! - [`contributor`] — BC18 Contributor Enablement (ADR-057)
//! - [`skills`]      — BC19 Skill Lifecycle (ADR-057)
//!
//! See `docs/explanation/ddd-contributor-enablement-context.md` and
//! `docs/explanation/ddd-enterprise-contexts.md`.

pub mod broker;
pub mod contributor;
pub mod skills;
