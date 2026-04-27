//! Central URN minting and parsing library (PRD-006 phase 2).
//!
//! Single chokepoint for every URN that crosses an API boundary:
//!
//! | Form                                                    | Mint               | Class |
//! |---------------------------------------------------------|--------------------|-------|
//! | `urn:visionclaw:concept:<domain>:<slug>`                | `mint_concept`     | R3    |
//! | `urn:visionclaw:group:<team>#members`                   | `mint_group_members` | R3  |
//! | `urn:visionclaw:kg:<npub>:<sha256-12-hex>`              | `mint_owned_kg`    | R1+R2 |
//! | `urn:visionclaw:bead:<npub>:<sha256-12-hex>`            | `mint_bead`        | R1+R2 |
//! | `urn:visionclaw:execution:<sha256-12-hex>`              | `mint_execution`   | R1    |
//! | `did:nostr:<64-hex-pubkey>`                             | `mint_did_nostr`   | R3    |
//!
//! R-classes: R1 content-addressed, R2 owner-scoped, R3 stable on identity
//! (agentbox ADR-013).
//!
//! `vc:<domain>/<slug>` CURIE form is the database join key (ADR-048's
//! `:KGNode.iri` / `:OntologyClass.iri`); the URN form above is the API
//! alias. `to_curie` / `from_curie` translate between them. The resolver
//! at `/api/v1/uri/<urn>` looks up by `visionclaw_uri` column;
//! `/api/v1/uri/by-curie/<vc>` looks up by `iri` column.
//!
//! Anti-drift gate: a CI grep step rejects ad-hoc
//! `format!("urn:visionclaw:...")` outside this module. See PRD-006 §6.

pub mod errors;
pub mod kinds;
pub mod legacy;
pub mod mint;
pub mod parse;

// Public re-exports — every caller imports from `crate::uri::*` so the rest
// of the codebase doesn't depend on the internal module split.
pub use errors::UriError;
pub use kinds::{Kind, ParsedUri};
pub use mint::{
    mint_bead, mint_concept, mint_did_nostr, mint_execution, mint_group_members, mint_owned_kg,
};
pub use parse::{
    content_hash_12, decode_npub, from_curie, is_canonical, normalise_pubkey, parse, to_curie,
};
