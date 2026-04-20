//! Domain module — aggregate roots and domain services per DDD.
//!
//! Each bounded context owns a submodule. Currently houses:
//! - `broker` — BC11 Judgment Broker Workbench (ADR-041/042)

pub mod broker;
