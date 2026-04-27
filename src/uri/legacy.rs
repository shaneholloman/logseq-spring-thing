//! Backwards-compatibility shims for legacy URN forms (PRD-006 §5.1
//! migration).
//!
//! Two pre-existing call sites mint a `canonical_iri` column value with
//! divergent grammar:
//!
//!   1. `src/utils/canonical_iri.rs::canonical_iri()` →
//!      `visionclaw:owner:<npub>/kg/<sha256-64>` (NIP-19 bech32 npub).
//!   2. `src/services/parsers/knowledge_graph_parser.rs::canonical_iri()` →
//!      `visionclaw:owner:<raw-hex-pubkey>/kg/<sha256-64>` (raw hex).
//!
//! Both forms are present in production data on the `canonical_iri` column.
//! `opaque_id.rs:166` derives bit29 binary-protocol opaque ids from this
//! column, so the existing values must NOT change. P2 wraps both in this
//! module with a `#[deprecated]` marker so callers route through here, and
//! the resolver looks up by either column (`iri` or `visionclaw_uri`).
//!
//! Eventual cleanup (post-P2): a one-shot data migration that backfills
//! `visionclaw_uri` with the new 12-hex form on every owner-scoped row,
//! after which the resolver can prefer the new column. Out of scope for P2.

use crate::uri::errors::UriError;
use crate::uri::parse::normalise_pubkey;
use nostr_sdk::{PublicKey, ToBech32};
use sha2::{Digest, Sha256};

/// Legacy form #1: `visionclaw:owner:<npub>/kg/<sha256-64>`.
///
/// Used by `src/utils/canonical_iri.rs::canonical_iri`. Encoded with NIP-19
/// bech32 (`npub1...`) and a full 64-char path-hash.
#[deprecated(
    since = "0.2.0",
    note = "use crate::uri::mint_owned_kg for the 12-hex API form; this \
            shim only exists so existing callers and ADR-054 rows keep \
            their column values"
)]
pub fn canonical_iri_npub(
    pubkey_hex: &str,
    relative_path: &str,
) -> Result<String, UriError> {
    let normalised = normalise_pubkey(pubkey_hex)?;
    let pk = PublicKey::from_hex(&normalised)
        .map_err(|e| UriError::InvalidPubkeyHex(e.to_string()))?;
    let npub = pk
        .to_bech32()
        .map_err(|e| UriError::Bech32Error(e.to_string()))?;
    let path_hash = sha256_full_hex(relative_path.as_bytes());
    Ok(format!("visionclaw:owner:{}/kg/{}", npub, path_hash))
}

/// Legacy form #2: `visionclaw:owner:<raw-hex-pubkey>/kg/<sha256-64>`.
///
/// Used by `src/services/parsers/knowledge_graph_parser.rs::canonical_iri`.
/// Same grammar as form #1 but the pubkey is raw 64-char hex, NOT bech32.
/// This was an oversight — the divergence is real and persists in the live
/// `canonical_iri` column. Preserve verbatim.
#[deprecated(
    since = "0.2.0",
    note = "use crate::uri::mint_owned_kg for the 12-hex API form; this \
            shim only exists so existing callers and existing rows keep \
            their column values. Note: pubkey is raw hex, not bech32 npub."
)]
pub fn canonical_iri_raw_hex(owner_pubkey_hex: &str, relative_path: &str) -> String {
    let path_hash = sha256_full_hex(relative_path.as_bytes());
    format!("visionclaw:owner:{}/kg/{}", owner_pubkey_hex, path_hash)
}

/// Lowercase hex SHA-256 of bytes. Module-private helper kept here (rather
/// than in `parse.rs::content_hash_12`) because `content_hash_12` returns
/// the 12-char API form, while these legacy callers need the full 64-char
/// digest.
fn sha256_full_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        out.push(nibble(b >> 4));
        out.push(nibble(b & 0x0F));
    }
    out
}

#[inline]
fn nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + (n - 10)) as char,
        _ => unreachable!(),
    }
}
