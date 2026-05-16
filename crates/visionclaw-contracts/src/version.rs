//! Schema-version constants for every envelope in this crate.
//!
//! Versioning policy is owned by ADR-10 §D8:
//!
//! - Adding a backwards-compatible field keeps `SCHEMA_VERSION = 1`. Producers
//!   may emit; consumers must ignore unknown fields.
//! - Renaming / removing / changing enum variants / changing transport
//!   semantics bumps to `SCHEMA_VERSION = 2`. Consumers refuse unsupported
//!   versions (close frame `4001 schema_version_unsupported` for telemetry,
//!   structured log + drop for action envelopes).
//! - Both sides keep one back-version of support so the deploy window is
//!   non-zero.
//!
//! This module is the single source of truth for the version literal. Every
//! envelope re-exports its own typed `SCHEMA_VERSION` constant from here so
//! call sites use one symbol, not a magic number.

/// Current schema version of all envelopes published by this crate.
///
/// Bump in lockstep with the contract-test fixtures under
/// `tests/contracts/external-integrations/`.
pub const SCHEMA_VERSION: u32 = 1;

/// Human-readable form of the schema version, suitable for `User-Agent`-style
/// headers and the npm package's `version` field tag (e.g. `"v1"`).
pub const SCHEMA_VERSION_STRING: &str = "v1";
