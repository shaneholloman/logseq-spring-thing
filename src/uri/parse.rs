//! URN/CURIE parsing + cross-form normalisation (PRD-006 §5.1).
//!
//! Two-direction translation:
//!   - `parse(s)` → `ParsedUri` for any URN form, plus `did:nostr:` and
//!     `vc:<domain>/<slug>` CURIEs.
//!   - `to_curie(parsed)` produces the agentbox/JSON-LD short form for any
//!     kind that has one (only Concept does today).
//!   - `from_curie(s)` is the reverse: rewrites a `vc:<domain>/<slug>` CURIE
//!     to a canonical `urn:visionclaw:concept:<domain>:<slug>` URN.
//!
//! The substrate-internal `vc:{domain}/{slug}` CURIE living on `:KGNode.iri`
//! and `:OntologyClass.iri` is the database join key (ADR-048). The
//! `urn:visionclaw:concept:...` form is the API-surface alias persisted on
//! `node.visionclaw_uri` and emitted in JSON-LD payloads. See PRD-006 §5.10.

use crate::uri::errors::UriError;
use crate::uri::kinds::ParsedUri;

/// Parse any URN or CURIE we mint into a `ParsedUri`.
///
/// Accepts:
///   - `urn:visionclaw:concept:<domain>:<slug>`
///   - `urn:visionclaw:group:<team>#members`
///   - `urn:visionclaw:kg:<npub>:<sha256-12-hex>`
///   - `urn:visionclaw:bead:<npub>:<sha256-12-hex>`
///   - `urn:visionclaw:execution:<sha256-12-hex>`
///   - `did:nostr:<64-hex>`
///   - `vc:<domain>/<slug>` (CURIE; resolves to `Concept`)
///
/// Rejects everything else with `UriError::ParseFailed` or `UnknownKind`.
pub fn parse(s: &str) -> Result<ParsedUri, UriError> {
    if s.is_empty() {
        return Err(UriError::ParseFailed("empty input".to_string()));
    }

    // CURIE (vc:<domain>/<slug>) — the database key form.
    if let Some(rest) = s.strip_prefix("vc:") {
        return parse_concept_curie(rest);
    }

    // did:nostr:<hex>
    if let Some(rest) = s.strip_prefix("did:nostr:") {
        return parse_did_nostr(rest);
    }

    // urn:visionclaw:<kind>:...
    let rest = s
        .strip_prefix("urn:visionclaw:")
        .ok_or_else(|| UriError::ParseFailed(format!("unrecognised URI scheme: {}", s)))?;

    // Find the kind segment (up to the first ':').
    let (kind, body) = rest
        .split_once(':')
        .ok_or_else(|| UriError::ParseFailed(format!("missing kind segment: {}", s)))?;

    match kind {
        "concept" => parse_concept_urn(body),
        "group" => parse_group(body),
        "kg" => parse_owned(body, /*is_bead=*/ false),
        "bead" => parse_owned(body, /*is_bead=*/ true),
        "execution" => parse_execution(body),
        other => Err(UriError::UnknownKind(other.to_string())),
    }
}

/// Convenience: true if `parse(s)` would succeed.
pub fn is_canonical(s: &str) -> bool {
    parse(s).is_ok()
}

// ----------------------------------------------------------------------------
// CURIE ↔ URN normalisation
// ----------------------------------------------------------------------------

/// Render the agentbox-style CURIE form for any kind that has one.
/// Today only `Concept` round-trips through CURIE. Other kinds passthrough
/// to the URN form (the caller can compare equal-semantics if it cares).
pub fn to_curie(parsed: &ParsedUri) -> String {
    match parsed {
        ParsedUri::Concept { domain, slug } => format!("vc:{}/{}", domain, slug),
        ParsedUri::Group { team } => format!("urn:visionclaw:group:{}#members", team),
        ParsedUri::OwnedKg { npub, hash12, .. } => {
            format!("urn:visionclaw:kg:{}:{}", npub, hash12)
        }
        ParsedUri::Bead { npub, hash12, .. } => {
            format!("urn:visionclaw:bead:{}:{}", npub, hash12)
        }
        ParsedUri::AgentExecution { hash12 } => {
            format!("urn:visionclaw:execution:{}", hash12)
        }
        ParsedUri::Did { pubkey_hex } => format!("did:nostr:{}", pubkey_hex),
    }
}

/// Translate a `vc:<domain>/<slug>` CURIE to the canonical URN form.
/// Passthrough for inputs that already start with `urn:` or `did:`.
pub fn from_curie(s: &str) -> Result<String, UriError> {
    if s.starts_with("urn:") || s.starts_with("did:") {
        return Ok(s.to_string());
    }
    let rest = s
        .strip_prefix("vc:")
        .ok_or_else(|| UriError::ParseFailed(format!("not a vc: CURIE: {}", s)))?;
    let (domain, slug) = rest
        .split_once('/')
        .ok_or_else(|| UriError::ParseFailed(format!("CURIE missing slug: {}", s)))?;
    if domain.is_empty() || slug.is_empty() {
        return Err(UriError::ParseFailed(format!("CURIE empty segment: {}", s)));
    }
    Ok(format!("urn:visionclaw:concept:{}:{}", domain, slug))
}

// ----------------------------------------------------------------------------
// Per-kind parse helpers
// ----------------------------------------------------------------------------

fn parse_concept_curie(rest: &str) -> Result<ParsedUri, UriError> {
    let (domain, slug) = rest
        .split_once('/')
        .ok_or_else(|| UriError::ParseFailed(format!("vc CURIE missing slug: vc:{}", rest)))?;
    if domain.is_empty() || slug.is_empty() {
        return Err(UriError::ParseFailed(format!(
            "vc CURIE empty segment: vc:{}",
            rest
        )));
    }
    Ok(ParsedUri::Concept {
        domain: domain.to_string(),
        slug: slug.to_string(),
    })
}

fn parse_concept_urn(body: &str) -> Result<ParsedUri, UriError> {
    let (domain, slug) = body
        .split_once(':')
        .ok_or_else(|| UriError::ParseFailed(format!("concept missing slug: {}", body)))?;
    if domain.is_empty() || slug.is_empty() {
        return Err(UriError::ParseFailed(format!(
            "concept empty segment: {}",
            body
        )));
    }
    Ok(ParsedUri::Concept {
        domain: domain.to_string(),
        slug: slug.to_string(),
    })
}

fn parse_group(body: &str) -> Result<ParsedUri, UriError> {
    let team = body
        .strip_suffix("#members")
        .ok_or_else(|| UriError::ParseFailed(format!("group missing #members suffix: {}", body)))?;
    if team.is_empty() {
        return Err(UriError::ParseFailed("group team is empty".to_string()));
    }
    Ok(ParsedUri::Group {
        team: team.to_string(),
    })
}

fn parse_owned(body: &str, is_bead: bool) -> Result<ParsedUri, UriError> {
    let (npub, hash12) = body
        .split_once(':')
        .ok_or_else(|| UriError::ParseFailed(format!("owned URN missing hash: {}", body)))?;
    validate_hash12(hash12)?;
    if !npub.starts_with("npub1") {
        return Err(UriError::ParseFailed(format!(
            "owned URN scope segment is not an npub: {}",
            npub
        )));
    }
    let pubkey_hex = decode_npub(npub)?;
    let parsed = if is_bead {
        ParsedUri::Bead {
            pubkey_hex,
            npub: npub.to_string(),
            hash12: hash12.to_string(),
        }
    } else {
        ParsedUri::OwnedKg {
            pubkey_hex,
            npub: npub.to_string(),
            hash12: hash12.to_string(),
        }
    };
    Ok(parsed)
}

fn parse_execution(body: &str) -> Result<ParsedUri, UriError> {
    validate_hash12(body)?;
    Ok(ParsedUri::AgentExecution {
        hash12: body.to_string(),
    })
}

fn parse_did_nostr(rest: &str) -> Result<ParsedUri, UriError> {
    validate_hex64(rest)?;
    Ok(ParsedUri::Did {
        pubkey_hex: rest.to_lowercase(),
    })
}

// ----------------------------------------------------------------------------
// Format / hex validators
// ----------------------------------------------------------------------------

/// `sha256-12-<12 lowercase hex chars>` — the byte-identical content-address
/// shape used by both VisionClaw and agentbox (PRD-006 F10).
pub fn validate_hash12(s: &str) -> Result<(), UriError> {
    let hex = s.strip_prefix("sha256-12-").ok_or_else(|| {
        UriError::ParseFailed(format!("hash must start with 'sha256-12-': {}", s))
    })?;
    if hex.len() != 12 {
        return Err(UriError::ParseFailed(format!(
            "hash12 expects 12 hex chars, got {}: {}",
            hex.len(),
            s
        )));
    }
    if !hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err(UriError::ParseFailed(format!(
            "hash12 must be lowercase hex: {}",
            s
        )));
    }
    Ok(())
}

fn validate_hex64(s: &str) -> Result<(), UriError> {
    if s.len() != 64 {
        return Err(UriError::InvalidPubkeyHex(format!(
            "expected 64 hex chars, got {}",
            s.len()
        )));
    }
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(UriError::InvalidPubkeyHex(format!(
            "non-hex characters in: {}",
            s
        )));
    }
    Ok(())
}

// ----------------------------------------------------------------------------
// Pubkey helpers
// ----------------------------------------------------------------------------

/// Normalise any accepted pubkey form to 64-char lowercase hex.
/// Accepts: 64-char hex (any case), `did:nostr:<hex>`, `npub1...`.
pub fn normalise_pubkey(input: &str) -> Result<String, UriError> {
    let stripped = input
        .strip_prefix("did:nostr:")
        .unwrap_or(input);
    if let Some(_npub) = stripped.strip_prefix("npub1") {
        return decode_npub(stripped);
    }
    validate_hex64(stripped)?;
    Ok(stripped.to_lowercase())
}

/// Bech32 (NIP-19) decode for `npub1...` → 64-char lowercase hex.
pub fn decode_npub(npub: &str) -> Result<String, UriError> {
    use nostr_sdk::{FromBech32, PublicKey};
    let pk = PublicKey::from_bech32(npub)
        .map_err(|e| UriError::Bech32Error(e.to_string()))?;
    Ok(pk.to_hex())
}

// ----------------------------------------------------------------------------
// Hashing
// ----------------------------------------------------------------------------

/// `sha256-12-<12 lowercase hex>` content-address of the input bytes.
/// This is the byte-identical form shared with agentbox (PRD-006 F10).
pub fn content_hash_12(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    // First 6 bytes → 12 hex chars.
    let mut hex = String::with_capacity(12);
    for b in &digest[..6] {
        hex.push(nibble(b >> 4));
        hex.push(nibble(b & 0x0F));
    }
    format!("sha256-12-{}", hex)
}

#[inline]
fn nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + (n - 10)) as char,
        _ => unreachable!(),
    }
}
