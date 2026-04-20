//! Domain-Driven Design — aggregate modules.
//!
//! Each submodule is a bounded context with aggregate roots, entities, value
//! objects, domain services, and domain events. Infrastructure and actors
//! live outside this module and treat aggregates as opaque facts.
//!
//! Current contexts:
//! - [`contributor`] — BC18 Contributor Enablement (ADR-057)

pub mod contributor;
