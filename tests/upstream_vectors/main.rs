// tests/upstream_vectors/main.rs
//! Entry point for L1 reference-vector test suite (ADR-082 D6 + ADR-077 P1).
//!
//! This file is required by Cargo's test discovery: `tests/<name>/main.rs`
//! is discovered as test target `<name>`. Without it, the individual modules
//! in this directory are not compiled or run.

mod bip340_schnorr;
mod did_doc;
mod is_envelope;
mod mesh_federation;
mod multibase;
mod nip01_events;
mod nip04_dm;
mod nip19_bech32;
mod nip26_delegation;
mod nip44;
mod nip59_gift_wrap;
mod nip98_tokens;
mod rfc8785_jcs;
