//! Central URN minting (PRD-006 §5.1).
//!
//! Every URN that crosses an API boundary in VisionClaw is minted here. A
//! clippy-style grep gate in CI rejects ad-hoc `format!("urn:visionclaw:...")`
//! anywhere outside `src/uri/`. See PRD-006 §6 (Anti-Drift Gate).

use crate::uri::errors::UriError;
use crate::uri::parse::{content_hash_12, normalise_pubkey};

/// `urn:visionclaw:concept:<domain>:<slug>` — R3 (stable on identity).
///
/// No validation beyond non-empty segments — domain/slug come from a
/// curated taxonomy and are validated upstream by the parser.
pub fn mint_concept(domain: &str, slug: &str) -> String {
    debug_assert!(!domain.is_empty(), "concept domain must not be empty");
    debug_assert!(!slug.is_empty(), "concept slug must not be empty");
    format!("urn:visionclaw:concept:{}:{}", domain, slug)
}

/// `urn:visionclaw:group:<team>#members` — R3.
pub fn mint_group_members(team: &str) -> String {
    debug_assert!(!team.is_empty(), "group team must not be empty");
    format!("urn:visionclaw:group:{}#members", team)
}

/// `urn:visionclaw:kg:<hex-pubkey>:<sha256-12-hex>` — R1 + R2 (content-addressed +
/// owner-scoped). API-alias form stored on `node.visionclaw_uri`.
///
/// The legacy `visionclaw:owner:<npub>/kg/<sha256-64>` form lives on the
/// `canonical_iri` column and is minted via `legacy::canonical_iri_npub`.
/// They co-exist; the resolver looks up by either column.
pub fn mint_owned_kg(pubkey_hex: &str, payload_bytes: &[u8]) -> Result<String, UriError> {
    if pubkey_hex.is_empty() {
        return Err(UriError::EmptyPubkey);
    }
    let normalised = normalise_pubkey(pubkey_hex)?;
    let hash12 = content_hash_12(payload_bytes);
    Ok(format!("urn:visionclaw:kg:{}:{}", normalised, hash12))
}

/// `did:nostr:<64-hex-pubkey>` — R3.
///
/// Accepts hex / `did:nostr:<hex>` / `npub1...` and re-emits the canonical
/// `did:nostr:` form on every accepted input.
pub fn mint_did_nostr(pubkey_hex: &str) -> Result<String, UriError> {
    if pubkey_hex.is_empty() {
        return Err(UriError::EmptyPubkey);
    }
    let hex = normalise_pubkey(pubkey_hex)?;
    Ok(format!("did:nostr:{}", hex))
}

/// `urn:visionclaw:bead:<hex-pubkey>:<sha256-12-hex>` — R1 + R2.
pub fn mint_bead(pubkey_hex: &str, payload: &serde_json::Value) -> Result<String, UriError> {
    if pubkey_hex.is_empty() {
        return Err(UriError::EmptyPubkey);
    }
    let normalised = normalise_pubkey(pubkey_hex)?;
    let bytes = serde_json::to_vec(payload).map_err(|e| {
        UriError::ParseFailed(format!("bead payload serialisation: {}", e))
    })?;
    let hash12 = content_hash_12(&bytes);
    Ok(format!("urn:visionclaw:bead:{}:{}", normalised, hash12))
}

/// `urn:visionclaw:execution:<sha256-12-hex>` — R1.
///
/// Hash domain is `<action>|<slot>|<pubkey>|<unix_ts>`; pipe is a separator
/// that cannot appear in any of the components by construction.
pub fn mint_execution(
    action: &str,
    slot: &str,
    pubkey_hex: &str,
    ts: i64,
) -> Result<String, UriError> {
    if pubkey_hex.is_empty() {
        return Err(UriError::EmptyPubkey);
    }
    let normalised = normalise_pubkey(pubkey_hex)?;
    let composite = format!("{}|{}|{}|{}", action, slot, normalised, ts);
    let hash12 = content_hash_12(composite.as_bytes());
    Ok(format!("urn:visionclaw:execution:{}", hash12))
}

