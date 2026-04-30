//! URN/CURIE data model for the central URI library (PRD-006 §5.1).
//!
//! `ParsedUri` is the post-parse algebraic type — every variant carries
//! exactly the components needed to round-trip back to the canonical string
//! form. The `Kind` enum is the discriminator used by callers that only need
//! to know which family a URI belongs to.

use serde::{Deserialize, Serialize};

/// Coarse categorisation of every URI the library mints/parses.
/// Keep this in lockstep with `ParsedUri` and the `KINDS` lookup in
/// `agentbox/management-api/lib/uris.js`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// `urn:visionclaw:concept:<domain>:<slug>` — R3 (stable on identity).
    Concept,
    /// `urn:visionclaw:group:<team>#members` — R3.
    Group,
    /// `urn:visionclaw:kg:<hex-pubkey>:<sha256-12-hex>` — R1 + R2.
    /// 12-hex form is the API alias; the canonical_iri column carries the
    /// legacy `visionclaw:owner:<npub>/kg/<sha256-64>` form (see ADR-054 +
    /// `legacy::canonical_iri_npub`).
    OwnedKg,
    /// `urn:visionclaw:bead:<hex-pubkey>:<sha256-12-hex>` — R1 + R2.
    Bead,
    /// `urn:visionclaw:execution:<sha256-12-hex>` — R1.
    AgentExecution,
    /// `did:nostr:<64-hex-pubkey>` — R3.
    Did,
}

/// The structured form of a parsed URI. Reconstructable to the canonical
/// string via `mint::format(parsed)` or by re-minting with the kind's
/// `mint_*` constructor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParsedUri {
    Concept {
        domain: String,
        slug: String,
    },
    Group {
        team: String,
    },
    OwnedKg {
        /// 64-char lowercase hex (BIP-340 x-only pubkey).
        pubkey_hex: String,
        /// 12-char lowercase hex (the API alias hash).
        hash12: String,
    },
    Bead {
        pubkey_hex: String,
        hash12: String,
    },
    AgentExecution {
        hash12: String,
    },
    Did {
        /// 64-char lowercase hex.
        pubkey_hex: String,
    },
}

impl ParsedUri {
    /// Coarse kind discriminator.
    pub fn kind(&self) -> Kind {
        match self {
            ParsedUri::Concept { .. } => Kind::Concept,
            ParsedUri::Group { .. } => Kind::Group,
            ParsedUri::OwnedKg { .. } => Kind::OwnedKg,
            ParsedUri::Bead { .. } => Kind::Bead,
            ParsedUri::AgentExecution { .. } => Kind::AgentExecution,
            ParsedUri::Did { .. } => Kind::Did,
        }
    }

    /// True if this kind is owner-scoped (R2). Used by call sites to decide
    /// whether they need a pubkey before minting.
    pub fn is_owner_scoped(&self) -> bool {
        matches!(self, ParsedUri::OwnedKg { .. } | ParsedUri::Bead { .. })
    }
}
