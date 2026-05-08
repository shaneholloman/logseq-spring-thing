//! Provenance Commit Encoder — PRD-013 G3.
//!
//! Every machine-generated commit carries structured provenance in git trailer
//! format (parseable via `git log --format='%(trailers)'`). The trailers form
//! a tamper-evident audit chain linking the enriched node URN, the proposing
//! agent DID, the approving broker DID, the broker case id, and a SHA-256 hash
//! of the reasoning text.
//!
//! Example output:
//!
//! ```text
//! feat(ontology): promote vc:bc/smart-contract to OWL class
//!
//! Enrichment applied by VisionClaw ingest pipeline.
//!
//! Urn: urn:visionclaw:concept:bc:smart-contract
//! Proposed-by: did:nostr:4d5e6f...
//! Approved-by: did:nostr:7a8b9c...
//! Broker-case: case-2026-05-08-001
//! Decision: approve
//! Reasoning-hash: sha256:abc123...
//! Timestamp: 2026-05-08T14:32:00Z
//! Signed-off-by: did:nostr:1a2b3c...
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// All fields that appear as trailers in a provenance commit message.
///
/// Field names follow git trailer conventions (Title-Case, colon-space
/// separated). The `Signed-off-by` trailer is the standard DCO trailer
/// repurposed with a `did:nostr:` identity instead of an email.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceTrailer {
    /// `urn:visionclaw:*` URI of the enriched node / class / edge.
    pub urn: String,

    /// `did:nostr:<hex>` of the agent (or system) that proposed the enrichment.
    pub proposed_by: String,

    /// `did:nostr:<hex>` of the broker that approved the enrichment.
    pub approved_by: String,

    /// Broker case id (from `BrokerCase.id`).
    pub broker_case: String,

    /// Decision outcome label (e.g. "approve", "promote").
    pub decision: String,

    /// SHA-256 hash of the full reasoning text, hex-encoded.
    /// Provides tamper-evidence without leaking the full reasoning.
    pub reasoning_hash: String,

    /// Timestamp of the broker decision.
    pub timestamp: DateTime<Utc>,

    /// `did:nostr:<hex>` of the identity signing the `git push` transport
    /// (typically the VisionClaw server identity).
    pub signed_off_by: String,
}

impl ProvenanceTrailer {
    /// Construct a `ProvenanceTrailer` from its components, computing the
    /// reasoning hash automatically.
    pub fn new(
        urn: impl Into<String>,
        proposed_by: impl Into<String>,
        approved_by: impl Into<String>,
        broker_case: impl Into<String>,
        decision: impl Into<String>,
        reasoning_text: &str,
        timestamp: DateTime<Utc>,
        signed_off_by: impl Into<String>,
    ) -> Self {
        Self {
            urn: urn.into(),
            proposed_by: proposed_by.into(),
            approved_by: approved_by.into(),
            broker_case: broker_case.into(),
            decision: decision.into(),
            reasoning_hash: hash_reasoning(reasoning_text),
            timestamp,
            signed_off_by: signed_off_by.into(),
        }
    }

    /// Format the trailer block as lines suitable for appending after a blank
    /// line in a git commit message.
    pub fn format_trailers(&self) -> String {
        // M7: strip newlines from all trailer values to prevent injection.
        fn safe(v: &str) -> String {
            v.replace('\n', " ").replace('\r', " ")
        }
        let mut lines = Vec::with_capacity(8);
        lines.push(format!("Urn: {}", safe(&self.urn)));
        lines.push(format!("Proposed-by: {}", safe(&self.proposed_by)));
        lines.push(format!("Approved-by: {}", safe(&self.approved_by)));
        lines.push(format!("Broker-case: {}", safe(&self.broker_case)));
        lines.push(format!("Decision: {}", safe(&self.decision)));
        lines.push(format!(
            "Reasoning-hash: sha256:{}",
            safe(&self.reasoning_hash)
        ));
        lines.push(format!("Timestamp: {}", self.timestamp.to_rfc3339()));
        lines.push(format!("Signed-off-by: {}", safe(&self.signed_off_by)));
        lines.join("\n")
    }
}

/// Encode a full commit message with a subject line, body, and provenance
/// trailers in standard git format.
///
/// Layout:
/// ```text
/// <subject>
///
/// <body>
///
/// <trailers>
/// ```
pub fn encode_commit_message(subject: &str, body: &str, trailer: &ProvenanceTrailer) -> String {
    let mut msg = String::with_capacity(512);

    // Subject line (first line, no trailing period per convention).
    msg.push_str(subject);
    msg.push('\n');

    // Body paragraph.
    if !body.is_empty() {
        msg.push('\n');
        msg.push_str(body);
        msg.push('\n');
    }

    // Trailer block (separated from body by a blank line).
    msg.push('\n');
    msg.push_str(&trailer.format_trailers());
    msg.push('\n');

    msg
}

/// Compute SHA-256 of reasoning text and return lowercase hex string.
fn hash_reasoning(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_trailer() -> ProvenanceTrailer {
        let ts = Utc.with_ymd_and_hms(2026, 5, 8, 14, 32, 0).unwrap();
        ProvenanceTrailer::new(
            "urn:visionclaw:concept:bc:smart-contract",
            "did:nostr:4d5e6f0000000000000000000000000000000000000000000000000000000000",
            "did:nostr:7a8b9c0000000000000000000000000000000000000000000000000000000000",
            "case-2026-05-08-001",
            "approve",
            "The smart-contract concept has sufficient community consensus.",
            ts,
            "did:nostr:1a2b3c0000000000000000000000000000000000000000000000000000000000",
        )
    }

    #[test]
    fn trailer_format_produces_valid_git_trailers() {
        let trailer = sample_trailer();
        let text = trailer.format_trailers();

        // Each line must be "Key: Value" format.
        for line in text.lines() {
            assert!(
                line.contains(": "),
                "trailer line missing ': ' separator: {}",
                line
            );
        }

        assert!(text.contains("Urn: urn:visionclaw:concept:bc:smart-contract"));
        assert!(text.contains("Proposed-by: did:nostr:4d5e6f"));
        assert!(text.contains("Approved-by: did:nostr:7a8b9c"));
        assert!(text.contains("Broker-case: case-2026-05-08-001"));
        assert!(text.contains("Decision: approve"));
        assert!(text.contains("Reasoning-hash: sha256:"));
        assert!(text.contains("Timestamp: 2026-05-08T14:32:00+00:00"));
        assert!(text.contains("Signed-off-by: did:nostr:1a2b3c"));
    }

    #[test]
    fn encode_commit_message_layout() {
        let trailer = sample_trailer();
        let msg = encode_commit_message(
            "feat(ontology): promote vc:bc/smart-contract to OWL class",
            "Enrichment applied by VisionClaw ingest pipeline.",
            &trailer,
        );

        let lines: Vec<&str> = msg.lines().collect();

        // First line is subject.
        assert_eq!(
            lines[0],
            "feat(ontology): promote vc:bc/smart-contract to OWL class"
        );
        // Second line is blank (separator).
        assert_eq!(lines[1], "");
        // Third line is body.
        assert_eq!(
            lines[2],
            "Enrichment applied by VisionClaw ingest pipeline."
        );
        // Fourth line is blank (separator before trailers).
        assert_eq!(lines[3], "");
        // Fifth line onwards are trailers.
        assert!(lines[4].starts_with("Urn:"));
    }

    #[test]
    fn reasoning_hash_is_deterministic() {
        let h1 = hash_reasoning("same text");
        let h2 = hash_reasoning("same text");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn reasoning_hash_differs_for_different_input() {
        let h1 = hash_reasoning("text A");
        let h2 = hash_reasoning("text B");
        assert_ne!(h1, h2);
    }

    #[test]
    fn empty_body_produces_valid_message() {
        let trailer = sample_trailer();
        let msg = encode_commit_message("fix: correct embedding dimensions", "", &trailer);

        let lines: Vec<&str> = msg.lines().collect();
        assert_eq!(lines[0], "fix: correct embedding dimensions");
        // No body paragraph — goes straight to blank + trailers.
        assert_eq!(lines[1], "");
        assert!(lines[2].starts_with("Urn:"));
    }
}
