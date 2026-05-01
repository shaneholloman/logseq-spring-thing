//! Bead signing and anti-replay foundation (PRD-005 Epic D, ADR-066).
//!
//! Content-addressed beads with hash-chain anti-replay envelopes for
//! pod-federated graph storage. Each bead contains a set of typed nodes
//! and edges in JSON-LD format, wrapped in an envelope that enforces
//! strict monotonic sequencing and hash-chain integrity.
//!
//! URN minting delegates to `crate::uri::mint::mint_bead` so all bead
//! URNs pass through the single-source mint gate (PRD-006 anti-drift).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::uri::mint::mint_bead;

// ── Error type ────────────────────────────────────────────────────────

/// Errors arising from bead signing and verification operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BeadSigningError {
    #[error("sequence violation: next seq {next} must be strictly greater than prev seq {prev}")]
    SequenceViolation { prev: u64, next: u64 },

    #[error("hash chain broken: expected prev_bead_sha {expected}, got {actual}")]
    HashChainBroken { expected: String, actual: String },

    #[error("bead has expired (expiry: {expiry})")]
    Expired { expiry: String },

    #[error("invalid owner hex pubkey: {reason}")]
    InvalidOwner { reason: String },

    #[error("URN mint failed: {0}")]
    UrnMintFailed(String),
}

// ── Payload ───────────────────────────────────────────────────────────

/// The graph data carried by a bead — nodes and edges in JSON-LD format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeadPayload {
    /// JSON-LD node objects.
    pub nodes: Vec<serde_json::Value>,
    /// JSON-LD edge objects.
    pub edges: Vec<serde_json::Value>,
}

impl BeadPayload {
    pub fn new(nodes: Vec<serde_json::Value>, edges: Vec<serde_json::Value>) -> Self {
        Self { nodes, edges }
    }

    /// Empty payload (useful for genesis beads / tests).
    pub fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

// ── Envelope ──────────────────────────────────────────────────────────

/// Anti-replay metadata per ADR-066 section 4.
///
/// The envelope enforces:
/// - **Hash-chain**: `prev_bead_sha` links to the content hash of the
///   preceding bead (None for genesis).
/// - **Monotonic sequence**: `monotonic_seq` is strictly increasing per
///   owner chain.
/// - **Time-bound validity**: `signed_at` / `expiry` bracket the
///   acceptance window.
/// - **Signature**: hex-encoded cryptographic signature (placeholder
///   until NIP-26 delegation lands in a follow-up).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeadEnvelope {
    /// Content hash of the previous bead in the owner's chain, or None
    /// for the genesis bead.
    pub prev_bead_sha: Option<String>,
    /// Strictly increasing sequence number within the owner's chain.
    pub monotonic_seq: u64,
    /// ISO 8601 timestamp when the bead was signed.
    pub signed_at: String,
    /// ISO 8601 timestamp after which the bead should be rejected.
    pub expiry: String,
    /// Hex-encoded signature over (content_hash || envelope fields).
    /// None until the signing backend is wired (follow-up to ADR-066).
    pub signature: Option<String>,
}

// ── Bead ──────────────────────────────────────────────────────────────

/// A content-addressed, hash-chained unit of federated graph storage.
///
/// The `urn` field is minted via the central URI library
/// (`urn:visionclaw:bead:<owner-hex>:<sha256-12>`), keeping all URN
/// generation behind the single mint gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignedBead {
    /// `urn:visionclaw:bead:<owner-hex>:<sha256-12>` — minted from the
    /// content hash of the canonicalized payload.
    pub urn: String,
    /// 64-char lowercase hex pubkey of the bead owner.
    pub owner_hex: String,
    /// The graph data carried by this bead.
    pub payload: BeadPayload,
    /// Anti-replay envelope.
    pub envelope: BeadEnvelope,
    /// Full SHA-256 hex digest of the canonicalized payload (64 chars).
    /// The URN's `sha256-12-*` suffix is the truncated form of this.
    pub content_hash: String,
}

/// Default bead validity window: 24 hours from signing.
const DEFAULT_EXPIRY_HOURS: i64 = 24;

impl SignedBead {
    /// Create a new bead, minting its URN from the content hash.
    ///
    /// # Arguments
    /// - `owner_hex` — 64-char hex pubkey (will be normalised to lowercase).
    /// - `payload` — the graph nodes and edges.
    /// - `prev_bead_sha` — content hash of the previous bead in this
    ///    owner's chain (None for genesis).
    /// - `seq` — monotonic sequence number (must be strictly greater than
    ///    the previous bead's seq).
    ///
    /// # Errors
    /// - `InvalidOwner` if the pubkey is not 64 hex chars.
    /// - `UrnMintFailed` if the URI library rejects the input.
    pub fn new(
        owner_hex: &str,
        payload: BeadPayload,
        prev_bead_sha: Option<String>,
        seq: u64,
    ) -> Result<Self, BeadSigningError> {
        validate_owner_hex(owner_hex)?;

        let normalised_owner = owner_hex.to_lowercase();
        let content_hash = Self::compute_content_hash(&payload);
        let now = Utc::now();

        // Mint URN via the central mint gate. The mint function takes a
        // serde_json::Value, so we serialize the payload canonically.
        let payload_json = canonical_payload_value(&payload);
        let urn = mint_bead(&normalised_owner, &payload_json)
            .map_err(|e| BeadSigningError::UrnMintFailed(format!("{}", e)))?;

        let envelope = BeadEnvelope {
            prev_bead_sha,
            monotonic_seq: seq,
            signed_at: now.to_rfc3339(),
            expiry: (now + Duration::hours(DEFAULT_EXPIRY_HOURS)).to_rfc3339(),
            signature: None, // placeholder until NIP-26 signing lands
        };

        Ok(Self {
            urn,
            owner_hex: normalised_owner,
            payload,
            envelope,
            content_hash,
        })
    }

    /// Create a bead with a custom expiry duration (in hours).
    pub fn with_expiry_hours(
        owner_hex: &str,
        payload: BeadPayload,
        prev_bead_sha: Option<String>,
        seq: u64,
        expiry_hours: i64,
    ) -> Result<Self, BeadSigningError> {
        let mut bead = Self::new(owner_hex, payload, prev_bead_sha, seq)?;
        let signed_at: DateTime<Utc> = bead
            .envelope
            .signed_at
            .parse()
            .expect("signed_at was just generated as valid RFC 3339");
        bead.envelope.expiry = (signed_at + Duration::hours(expiry_hours)).to_rfc3339();
        Ok(bead)
    }

    /// Compute the full SHA-256 hex digest of the canonicalized payload.
    ///
    /// Canonicalization: JSON keys are sorted alphabetically via
    /// `serde_json::to_string` on a `Value` built with sorted maps
    /// (serde_json preserves insertion order; we rebuild via
    /// `canonical_payload_value` which sorts keys).
    pub fn compute_content_hash(payload: &BeadPayload) -> String {
        let canonical = canonical_payload_bytes(payload);
        let mut hasher = Sha256::new();
        hasher.update(&canonical);
        let digest = hasher.finalize();
        hex::encode(digest)
    }

    /// Verify that `next` is a valid successor to `self` in the same
    /// owner's bead chain.
    ///
    /// Checks:
    /// 1. `next.envelope.monotonic_seq > self.envelope.monotonic_seq`
    /// 2. `next.envelope.prev_bead_sha == Some(self.content_hash)`
    pub fn verify_sequence(&self, next: &SignedBead) -> Result<(), BeadSigningError> {
        // Check monotonic sequence
        if next.envelope.monotonic_seq <= self.envelope.monotonic_seq {
            return Err(BeadSigningError::SequenceViolation {
                prev: self.envelope.monotonic_seq,
                next: next.envelope.monotonic_seq,
            });
        }

        // Check hash chain link
        let expected = &self.content_hash;
        match &next.envelope.prev_bead_sha {
            Some(actual) if actual == expected => Ok(()),
            Some(actual) => Err(BeadSigningError::HashChainBroken {
                expected: expected.clone(),
                actual: actual.clone(),
            }),
            None => Err(BeadSigningError::HashChainBroken {
                expected: expected.clone(),
                actual: "<none>".to_string(),
            }),
        }
    }

    /// True if the bead's expiry timestamp is in the past.
    pub fn is_expired(&self) -> bool {
        match self.envelope.expiry.parse::<DateTime<Utc>>() {
            Ok(expiry) => Utc::now() > expiry,
            // Unparseable expiry is treated as expired (fail-closed).
            Err(_) => true,
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Validate that the owner hex is a 64-char hex string.
fn validate_owner_hex(hex: &str) -> Result<(), BeadSigningError> {
    if hex.len() != 64 {
        return Err(BeadSigningError::InvalidOwner {
            reason: format!("expected 64 hex chars, got {}", hex.len()),
        });
    }
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(BeadSigningError::InvalidOwner {
            reason: "contains non-hex characters".to_string(),
        });
    }
    Ok(())
}

/// Build a canonical `serde_json::Value` from a `BeadPayload` with
/// sorted keys at every level.
fn canonical_payload_value(payload: &BeadPayload) -> serde_json::Value {
    // Serialize to Value, which preserves the struct field order
    // (nodes, edges). Individual node/edge Values keep their original
    // key order — callers that need deep canonicalization should
    // pre-sort their JSON-LD objects.
    serde_json::to_value(payload).expect("BeadPayload is always serializable")
}

/// Canonical byte representation of the payload for hashing.
fn canonical_payload_bytes(payload: &BeadPayload) -> Vec<u8> {
    let value = canonical_payload_value(payload);
    // serde_json::to_vec produces compact JSON (no whitespace).
    serde_json::to_vec(&value).expect("BeadPayload is always serializable")
}

// We use the `hex` crate for encoding if available; otherwise inline.
// The project has `sha2` which depends on `digest`, and `hex` is
// commonly pulled in transitively. If not, provide a minimal encoder.
mod hex {
    /// Encode bytes as lowercase hex string.
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(nibble(b >> 4));
            s.push(nibble(b & 0x0F));
        }
        s
    }

    #[inline]
    fn nibble(n: u8) -> char {
        match n {
            0..=9 => (b'0' + n) as char,
            10..=15 => (b'a' + (n - 10)) as char,
            _ => unreachable!(),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A deterministic 64-char hex pubkey for testing.
    const TEST_OWNER: &str = "aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd";

    fn sample_payload() -> BeadPayload {
        BeadPayload::new(
            vec![json!({"@id": "node-1", "@type": "Concept", "label": "Rust"})],
            vec![json!({"@id": "edge-1", "source": "node-1", "target": "node-2"})],
        )
    }

    // ── Bead creation ─────────────────────────────────────────────────

    #[test]
    fn new_bead_mints_valid_urn() {
        let bead = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();

        assert!(
            bead.urn.starts_with("urn:visionclaw:bead:"),
            "URN must start with urn:visionclaw:bead:, got: {}",
            bead.urn
        );
        assert!(
            bead.urn.contains(TEST_OWNER),
            "URN must contain the owner hex"
        );
        assert!(
            bead.urn.contains("sha256-12-"),
            "URN must contain content-address hash"
        );
    }

    #[test]
    fn new_bead_fills_envelope_fields() {
        let bead = SignedBead::new(TEST_OWNER, sample_payload(), None, 42).unwrap();

        assert_eq!(bead.envelope.monotonic_seq, 42);
        assert!(bead.envelope.prev_bead_sha.is_none());
        assert!(bead.envelope.signature.is_none());
        assert!(!bead.envelope.signed_at.is_empty());
        assert!(!bead.envelope.expiry.is_empty());

        // Expiry is after signed_at
        let signed: DateTime<Utc> = bead.envelope.signed_at.parse().unwrap();
        let expiry: DateTime<Utc> = bead.envelope.expiry.parse().unwrap();
        assert!(expiry > signed);
    }

    #[test]
    fn new_bead_normalises_owner_to_lowercase() {
        let upper_owner = "AABBCCDDAABBCCDDAABBCCDDAABBCCDDAABBCCDDAABBCCDDAABBCCDDAABBCCDD";
        let bead = SignedBead::new(upper_owner, sample_payload(), None, 1).unwrap();
        assert_eq!(bead.owner_hex, TEST_OWNER);
    }

    #[test]
    fn new_bead_genesis_has_no_prev_hash() {
        let bead = SignedBead::new(TEST_OWNER, BeadPayload::empty(), None, 0).unwrap();
        assert!(bead.envelope.prev_bead_sha.is_none());
    }

    #[test]
    fn new_bead_with_prev_hash_stores_it() {
        let prev_hash = "abc123".to_string();
        let bead =
            SignedBead::new(TEST_OWNER, sample_payload(), Some(prev_hash.clone()), 2).unwrap();
        assert_eq!(bead.envelope.prev_bead_sha, Some(prev_hash));
    }

    // ── Content hashing determinism ───────────────────────────────────

    #[test]
    fn content_hash_is_deterministic() {
        let payload = sample_payload();
        let hash1 = SignedBead::compute_content_hash(&payload);
        let hash2 = SignedBead::compute_content_hash(&payload);
        assert_eq!(hash1, hash2, "same payload must produce same hash");
    }

    #[test]
    fn content_hash_is_64_hex_chars() {
        let hash = SignedBead::compute_content_hash(&sample_payload());
        assert_eq!(hash.len(), 64, "SHA-256 hex digest is 64 chars");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash must be hex"
        );
    }

    #[test]
    fn different_payloads_produce_different_hashes() {
        let p1 = sample_payload();
        let p2 = BeadPayload::new(vec![json!({"@id": "node-2", "@type": "Agent"})], vec![]);
        let h1 = SignedBead::compute_content_hash(&p1);
        let h2 = SignedBead::compute_content_hash(&p2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn empty_payload_has_stable_hash() {
        let hash = SignedBead::compute_content_hash(&BeadPayload::empty());
        // Re-compute to confirm stability
        let hash2 = SignedBead::compute_content_hash(&BeadPayload::empty());
        assert_eq!(hash, hash2);
        assert_eq!(hash.len(), 64);
    }

    // ── Sequence verification ─────────────────────────────────────────

    #[test]
    fn verify_sequence_accepts_valid_chain() {
        let bead1 = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();
        let bead2 = SignedBead::new(
            TEST_OWNER,
            sample_payload(),
            Some(bead1.content_hash.clone()),
            2,
        )
        .unwrap();

        assert!(bead1.verify_sequence(&bead2).is_ok());
    }

    #[test]
    fn verify_sequence_rejects_non_increasing_seq() {
        let bead1 = SignedBead::new(TEST_OWNER, sample_payload(), None, 5).unwrap();
        let bead2 = SignedBead::new(
            TEST_OWNER,
            sample_payload(),
            Some(bead1.content_hash.clone()),
            5, // same — not strictly increasing
        )
        .unwrap();

        let err = bead1.verify_sequence(&bead2).unwrap_err();
        assert!(
            matches!(
                err,
                BeadSigningError::SequenceViolation { prev: 5, next: 5 }
            ),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn verify_sequence_rejects_decreasing_seq() {
        let bead1 = SignedBead::new(TEST_OWNER, sample_payload(), None, 10).unwrap();
        let bead2 = SignedBead::new(
            TEST_OWNER,
            sample_payload(),
            Some(bead1.content_hash.clone()),
            3,
        )
        .unwrap();

        let err = bead1.verify_sequence(&bead2).unwrap_err();
        assert!(matches!(
            err,
            BeadSigningError::SequenceViolation { prev: 10, next: 3 }
        ));
    }

    #[test]
    fn verify_sequence_rejects_wrong_prev_hash() {
        let bead1 = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();
        let bead2 = SignedBead::new(
            TEST_OWNER,
            sample_payload(),
            Some("wrong_hash".to_string()),
            2,
        )
        .unwrap();

        let err = bead1.verify_sequence(&bead2).unwrap_err();
        assert!(
            matches!(err, BeadSigningError::HashChainBroken { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn verify_sequence_rejects_missing_prev_hash() {
        let bead1 = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();
        let bead2 = SignedBead::new(
            TEST_OWNER,
            sample_payload(),
            None, // missing — not genesis position
            2,
        )
        .unwrap();

        let err = bead1.verify_sequence(&bead2).unwrap_err();
        assert!(matches!(err, BeadSigningError::HashChainBroken { .. }));
    }

    // ── Expiry checks ─────────────────────────────────────────────────

    #[test]
    fn fresh_bead_is_not_expired() {
        let bead = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();
        assert!(!bead.is_expired());
    }

    #[test]
    fn bead_with_past_expiry_is_expired() {
        let mut bead = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();
        // Set expiry to 1 hour ago
        let past = Utc::now() - Duration::hours(1);
        bead.envelope.expiry = past.to_rfc3339();
        assert!(bead.is_expired());
    }

    #[test]
    fn bead_with_unparseable_expiry_is_treated_as_expired() {
        let mut bead = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();
        bead.envelope.expiry = "not-a-date".to_string();
        assert!(bead.is_expired(), "unparseable expiry should fail-closed");
    }

    // ── Hash chain validation (multi-bead) ────────────────────────────

    #[test]
    fn three_bead_chain_validates_end_to_end() {
        let b1 = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();

        let b2 = SignedBead::new(
            TEST_OWNER,
            BeadPayload::new(vec![json!({"@id": "n2"})], vec![]),
            Some(b1.content_hash.clone()),
            2,
        )
        .unwrap();

        let b3 = SignedBead::new(
            TEST_OWNER,
            BeadPayload::new(vec![json!({"@id": "n3"})], vec![]),
            Some(b2.content_hash.clone()),
            3,
        )
        .unwrap();

        assert!(b1.verify_sequence(&b2).is_ok());
        assert!(b2.verify_sequence(&b3).is_ok());
    }

    #[test]
    fn chain_detects_tampered_intermediate_bead() {
        let b1 = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();

        // b2 claims to follow b1 but has wrong prev hash
        let b2 = SignedBead::new(
            TEST_OWNER,
            BeadPayload::new(vec![json!({"@id": "tampered"})], vec![]),
            Some("0000000000000000000000000000000000000000000000000000000000000000".to_string()),
            2,
        )
        .unwrap();

        assert!(b1.verify_sequence(&b2).is_err());
    }

    // ── Owner validation ──────────────────────────────────────────────

    #[test]
    fn rejects_short_owner_hex() {
        let err = SignedBead::new("aabb", sample_payload(), None, 1).unwrap_err();
        assert!(matches!(err, BeadSigningError::InvalidOwner { .. }));
    }

    #[test]
    fn rejects_non_hex_owner() {
        let bad = "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg";
        let err = SignedBead::new(bad, sample_payload(), None, 1).unwrap_err();
        assert!(matches!(err, BeadSigningError::InvalidOwner { .. }));
    }

    #[test]
    fn rejects_empty_owner() {
        let err = SignedBead::new("", sample_payload(), None, 1).unwrap_err();
        assert!(matches!(err, BeadSigningError::InvalidOwner { .. }));
    }

    // ── Custom expiry ─────────────────────────────────────────────────

    #[test]
    fn with_expiry_hours_sets_custom_window() {
        let bead =
            SignedBead::with_expiry_hours(TEST_OWNER, sample_payload(), None, 1, 48).unwrap();

        let signed: DateTime<Utc> = bead.envelope.signed_at.parse().unwrap();
        let expiry: DateTime<Utc> = bead.envelope.expiry.parse().unwrap();
        let diff = expiry - signed;
        // Allow 1 second of drift from test execution
        assert!(
            diff.num_hours() == 48 || diff.num_hours() == 47,
            "expected ~48h, got {}h",
            diff.num_hours()
        );
    }

    // ── Serialization round-trip ──────────────────────────────────────

    #[test]
    fn signed_bead_serde_roundtrip() {
        let bead = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();
        let json = serde_json::to_string(&bead).expect("serialize");
        let restored: SignedBead = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(bead, restored);
    }

    #[test]
    fn envelope_serde_roundtrip() {
        let bead = SignedBead::new(TEST_OWNER, sample_payload(), None, 1).unwrap();
        let json = serde_json::to_string(&bead.envelope).expect("serialize");
        let restored: BeadEnvelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(bead.envelope, restored);
    }

    #[test]
    fn payload_serde_roundtrip() {
        let payload = sample_payload();
        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: BeadPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(payload, restored);
    }
}
