//! Canonical IRI generation for sovereign-model KG nodes (ADR-050).
//!
//! Every node in the sovereign data plane is addressed by a canonical IRI of
//! the form
//!
//! ```text
//! visionclaw:owner:{npub}/kg/{sha256_hex(relative_path)}
//! ```
//!
//! - `{npub}` is the owner's Nostr public key encoded per NIP-19 (bech32,
//!   human-readable prefix `npub`). The project already depends on
//!   `nostr-sdk`, which provides the canonical implementation via
//!   `PublicKey::to_bech32()`.
//! - `{sha256_hex(relative_path)}` is the lowercase hex SHA-256 of the
//!   owner-scoped relative path to the node's source document. Hashing the
//!   path means the IRI does not leak the original filename or directory
//!   structure; the client keeps a local `path -> iri` map if it needs to
//!   reverse this for UI display.
//!
//! The IRI is stable for a given `(owner_pubkey, relative_path)` pair and is
//! therefore safe to use as a Neo4j identifier and as a reference from one
//! Pod to another.

use nostr_sdk::{PublicKey, ToBech32};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CanonicalIriError {
    #[error("invalid owner pubkey hex: {0}")]
    InvalidPubkey(String),
    #[error("failed to bech32-encode pubkey as npub: {0}")]
    Bech32(String),
}

/// Build the canonical IRI for a KG node.
///
/// `owner_pubkey_hex` must be a 64-char lowercase (or mixed-case) hex string —
/// the same format already used throughout `nostr_service.rs` and the Solid
/// Pod handler. `relative_path` is the owner-scoped, forward-slash-normalised
/// path to the source document (e.g. `logseq/pages/My Note.md`).
///
/// **Deprecated**: new code should use [`crate::uri::mint_owned_kg`] which
/// produces the 12-hex API alias form. This shim survives so the existing
/// `canonical_iri` Neo4j column values stay byte-identical (PRD-006 §5.1).
#[deprecated(
    since = "0.2.0",
    note = "use crate::uri::mint_owned_kg for new code; this fn preserves \
            ADR-054 row values for the canonical_iri column"
)]
pub fn canonical_iri(
    owner_pubkey_hex: &str,
    relative_path: &str,
) -> Result<String, CanonicalIriError> {
    let npub = encode_npub(owner_pubkey_hex)?;
    let hash = sha256_hex(relative_path.as_bytes());
    Ok(format!("visionclaw:owner:{}/kg/{}", npub, hash))
}

/// NIP-19 `npub` encoding for a hex Nostr public key.
///
/// Delegates to `nostr_sdk::PublicKey::to_bech32` (bech32-m with HRP `npub`).
pub fn encode_npub(pubkey_hex: &str) -> Result<String, CanonicalIriError> {
    let pk = PublicKey::from_hex(pubkey_hex)
        .map_err(|e| CanonicalIriError::InvalidPubkey(e.to_string()))?;
    pk.to_bech32().map_err(|e| CanonicalIriError::Bech32(e.to_string()))
}

/// Lowercase hex SHA-256 of the given bytes. Separate function so callers and
/// tests can verify the path-hashing step in isolation.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    // Manual lowercase hex encoding — avoids pulling the `hex` crate as a
    // direct dependency. `sha2::digest::generic_array::GenericArray` deref's
    // to `[u8]` so iteration is straightforward.
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        out.push(nibble_to_hex(b >> 4));
        out.push(nibble_to_hex(b & 0x0F));
    }
    out
}

#[inline]
fn nibble_to_hex(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + (n - 10)) as char,
        _ => unreachable!(),
    }
}

#[cfg(test)]
#[allow(deprecated)] // Tests intentionally call the deprecated fn to verify legacy column values.
mod tests {
    use super::*;

    // Deterministic 32-byte hex test vector (all zeros bar last byte) — a
    // real, parseable secp256k1 x-only public key isn't required because
    // `nostr_sdk::PublicKey::from_hex` accepts any 32-byte hex string as an
    // x-only pubkey at the API surface.
    const TEST_PUBKEY_HEX: &str =
        "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    #[test]
    fn sha256_hex_is_deterministic() {
        let a = sha256_hex(b"pages/Note.md");
        let b = sha256_hex(b"pages/Note.md");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        // Independently verifiable: echo -n "abc" | sha256sum
        let got = sha256_hex(b"abc");
        assert_eq!(
            got,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn encode_npub_starts_with_npub_prefix() {
        let npub = encode_npub(TEST_PUBKEY_HEX).expect("encode ok");
        assert!(npub.starts_with("npub1"), "got {}", npub);
        // bech32-encoded 32-byte payload is 63 chars incl. prefix and checksum.
        assert!(npub.len() > 10 && npub.len() < 100);
    }

    #[test]
    fn encode_npub_rejects_garbage() {
        assert!(encode_npub("not-hex").is_err());
        assert!(encode_npub("1234").is_err());
    }

    #[test]
    fn canonical_iri_is_deterministic() {
        let iri1 = canonical_iri(TEST_PUBKEY_HEX, "pages/Note.md").unwrap();
        let iri2 = canonical_iri(TEST_PUBKEY_HEX, "pages/Note.md").unwrap();
        assert_eq!(iri1, iri2);
    }

    #[test]
    fn canonical_iri_differs_for_different_paths() {
        let iri1 = canonical_iri(TEST_PUBKEY_HEX, "pages/A.md").unwrap();
        let iri2 = canonical_iri(TEST_PUBKEY_HEX, "pages/B.md").unwrap();
        assert_ne!(iri1, iri2);
    }

    #[test]
    fn canonical_iri_uses_npub_and_path_hash() {
        let iri = canonical_iri(TEST_PUBKEY_HEX, "pages/Note.md").unwrap();
        let npub = encode_npub(TEST_PUBKEY_HEX).unwrap();
        let expected_hash = sha256_hex(b"pages/Note.md");
        let expected = format!("visionclaw:owner:{}/kg/{}", npub, expected_hash);
        assert_eq!(iri, expected);
    }
}
