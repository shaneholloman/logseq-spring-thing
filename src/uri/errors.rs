//! Error types for the central URI minting library (PRD-006 §5.1).

use thiserror::Error;

/// All failures the URI module can produce. Variant names map 1:1 with the
/// invariant they protect (R1 content-addressed, R2 scope-bearing, R3
/// stable-on-identity per agentbox ADR-013).
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum UriError {
    /// Owner-scoped kinds (`OwnedKg`, `Bead`) require a non-empty pubkey at
    /// mint time. R2 invariant.
    #[error("owner-scoped URN requires a non-empty pubkey")]
    EmptyPubkey,

    /// The provided pubkey is not parseable as a 32-byte secp256k1 public
    /// key in any accepted form (64-char hex, `did:nostr:<hex>`, `npub1...`).
    #[error("invalid pubkey: {0}")]
    InvalidPubkeyHex(String),

    /// `npub1...` decoding failed (malformed bech32 or wrong HRP).
    #[error("bech32 decode failed: {0}")]
    Bech32Error(String),

    /// `parse()` could not match the input against any known URN/CURIE shape.
    #[error("parse failed: {0}")]
    ParseFailed(String),

    /// Recognised URN form but the kind discriminator is not in the
    /// `KINDS` table (e.g. `urn:visionclaw:foo:...`).
    #[error("unknown URN kind: {0}")]
    UnknownKind(String),
}
