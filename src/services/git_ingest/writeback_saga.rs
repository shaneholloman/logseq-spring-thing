//! Write-Back Saga — PRD-013 G4 (scaffold).
//!
//! Orchestrates the reverse flow of the ingest pipeline: enrichments that have
//! been approved by the Judgment Broker are committed back to the source pod
//! or repository. The saga fetches the latest state, applies the enrichment,
//! commits with full provenance trailers, and pushes.
//!
//! **This module is a scaffold.** The full implementation depends on Phase 3
//! deliverables (NIP-98 signed push, enrichment file format writers, conflict
//! detection). The public API is stabilised here so that the broker's
//! `DecisionOrchestrator` can reference it today.
//!
//! # Saga phases
//!
//! 1. **Fetch** — `git fetch` latest from the remote to ensure no conflicts.
//! 2. **Apply** — Write the enrichment to the local worktree in the
//!    appropriate file format (`.ttl`, `.embeddings.json`, `.proposals.md`).
//! 3. **Commit** — Create a commit with provenance trailers (G3).
//! 4. **Push** — `git push` to the remote with NIP-98 signed transport.
//! 5. **Record** — Persist the push result in Neo4j (audit trail).
//!
//! # Feature gate
//!
//! Write-back is disabled by default (`WRITEBACK_ENABLED=false`). It must be
//! opted in per-remote (`GitRemote.writeback_enabled`) AND globally.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::provenance::ProvenanceTrailer;

// ---------------------------------------------------------------------------
// Feature flag
// ---------------------------------------------------------------------------

/// Global kill-switch for write-back (default: `false`).
pub const WRITEBACK_ENABLED_ENV: &str = "WRITEBACK_ENABLED";

/// Returns `true` if write-back is globally enabled.
pub fn writeback_enabled() -> bool {
    std::env::var(WRITEBACK_ENABLED_ENV)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Payload describing the enrichment to write back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentPayload {
    /// Type of enrichment being applied.
    pub enrichment_type: EnrichmentType,
    /// Relative file path within the repository where the enrichment lands.
    pub target_path: String,
    /// Serialised content to write (format depends on `enrichment_type`).
    pub content: String,
    /// Human-readable subject line for the commit message.
    pub commit_subject: String,
    /// Optional body paragraph for the commit message.
    pub commit_body: String,
}

/// Discriminator for the kind of enrichment being applied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnrichmentType {
    /// Ontology promotion: write OWL fragment as `.ttl` alongside `.md`.
    OntologyPromotion,
    /// Embedding update: write vector to `.embeddings.json` sidecar.
    EmbeddingUpdate,
    /// Gap detection: write proposed edge as `.proposals.md`.
    GapDetection,
    /// Agent reasoning: write structured annotation.
    AgentAnnotation,
}

/// Report from the broker's `DecisionOrchestrator` authorising the write-back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionReport {
    /// Broker case id.
    pub case_id: String,
    /// Decision outcome label (e.g. "approve", "promote").
    pub decision: String,
    /// `did:nostr:<hex>` of the proposing agent.
    pub proposed_by: String,
    /// `did:nostr:<hex>` of the approving broker.
    pub approved_by: String,
    /// Full reasoning text (hashed for the commit trailer).
    pub reasoning: String,
    /// `did:nostr:<hex>` of the server identity (signs the push).
    pub server_did: String,
    /// URN of the enriched entity.
    pub entity_urn: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum WriteBackError {
    #[error("write-back is globally disabled (set WRITEBACK_ENABLED=true)")]
    GloballyDisabled,

    #[error("write-back is disabled for remote {0}")]
    RemoteDisabled(String),

    #[error("git operation failed: {0}")]
    Git(String),

    #[error("enrichment apply failed: {0}")]
    ApplyFailed(String),

    #[error("push failed: {0}")]
    PushFailed(String),

    #[error("audit recording failed: {0}")]
    AuditFailed(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

// ---------------------------------------------------------------------------
// WriteBackSaga
// ---------------------------------------------------------------------------

/// Orchestrates writing an approved enrichment back to a source remote.
///
/// Phase 3 deliverable — this is currently a scaffold exposing the stable
/// public API. The five saga phases are documented but not yet implemented.
#[derive(Debug, Clone)]
pub struct WriteBackSaga;

impl WriteBackSaga {
    pub fn new() -> Self {
        Self
    }

    /// Execute the full write-back saga for an approved enrichment.
    ///
    /// # Arguments
    ///
    /// * `remote_id` — id of the `GitRemote` to push to.
    /// * `enrichment` — the enrichment content and metadata.
    /// * `decision` — the broker's decision report authorising the push.
    ///
    /// # Errors
    ///
    /// Returns `WriteBackError::GloballyDisabled` if `WRITEBACK_ENABLED` is
    /// not set, or `WriteBackError::NotImplemented` for the remaining phases
    /// pending Phase 3 completion.
    pub async fn execute(
        &self,
        remote_id: &str,
        enrichment: &EnrichmentPayload,
        decision: &DecisionReport,
    ) -> Result<WriteBackResult, WriteBackError> {
        // ---------------------------------------------------------------
        // Gate: global kill-switch
        // ---------------------------------------------------------------
        if !writeback_enabled() {
            return Err(WriteBackError::GloballyDisabled);
        }

        // ---------------------------------------------------------------
        // Phase 1: Fetch latest from remote (ensure no conflicts)
        // ---------------------------------------------------------------
        // TODO(phase-3): git fetch + conflict detection.
        // The fetch must verify that our local HEAD is an ancestor of the
        // remote HEAD. If not, the saga aborts with a conflict error and
        // the broker is notified for manual resolution.
        log::debug!(
            "write-back: phase 1 — fetch latest for remote {} (not yet implemented)",
            remote_id
        );

        // ---------------------------------------------------------------
        // Phase 2: Apply enrichment to local worktree
        // ---------------------------------------------------------------
        // TODO(phase-3): Write enrichment content to the appropriate file.
        //   - OntologyPromotion → .ttl sidecar
        //   - EmbeddingUpdate   → .embeddings.json
        //   - GapDetection      → .proposals.md
        //   - AgentAnnotation   → structured annotation file
        log::debug!(
            "write-back: phase 2 — apply {:?} to {} (not yet implemented)",
            enrichment.enrichment_type,
            enrichment.target_path
        );

        // ---------------------------------------------------------------
        // Phase 3: Commit with provenance trailers (G3)
        // ---------------------------------------------------------------
        // TODO(phase-3): Use provenance::encode_commit_message() to create
        // the commit. The ProvenanceTrailer is built from the DecisionReport.
        let _trailer = ProvenanceTrailer::new(
            &decision.entity_urn,
            &decision.proposed_by,
            &decision.approved_by,
            &decision.case_id,
            &decision.decision,
            &decision.reasoning,
            chrono::Utc::now(),
            &decision.server_did,
        );
        log::debug!("write-back: phase 3 — commit prepared (not yet implemented)");

        // ---------------------------------------------------------------
        // Phase 4: Push to remote (NIP-98 signed)
        // ---------------------------------------------------------------
        // TODO(phase-3): git push with NIP-98 auth headers.
        log::debug!("write-back: phase 4 — push (not yet implemented)");

        // ---------------------------------------------------------------
        // Phase 5: Record push result in Neo4j (audit trail)
        // ---------------------------------------------------------------
        // TODO(phase-3): Persist a WriteBackAuditEntry linking the broker
        // case, the commit SHA, the remote id, and the push timestamp.
        log::debug!("write-back: phase 5 — audit record (not yet implemented)");

        Err(WriteBackError::NotImplemented(
            "WriteBackSaga phases 1-5 require Phase 3 implementation".to_string(),
        ))
    }
}

/// Result of a successful write-back execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteBackResult {
    /// Commit SHA that was pushed.
    pub commit_sha: String,
    /// Remote id the commit was pushed to.
    pub remote_id: String,
    /// Broker case id that authorised the push.
    pub case_id: String,
    /// Timestamp of the push.
    pub pushed_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writeback_disabled_by_default() {
        std::env::remove_var(WRITEBACK_ENABLED_ENV);
        assert!(!writeback_enabled());
    }

    #[test]
    fn enrichment_type_serialises() {
        let json = serde_json::to_string(&EnrichmentType::OntologyPromotion).unwrap();
        assert_eq!(json, "\"ontology_promotion\"");
    }

    #[test]
    fn enrichment_payload_round_trips() {
        let payload = EnrichmentPayload {
            enrichment_type: EnrichmentType::EmbeddingUpdate,
            target_path: "concepts/smart-contract.embeddings.json".to_string(),
            content: r#"{"vector": [0.1, 0.2]}"#.to_string(),
            commit_subject: "feat(embedding): update smart-contract vector".to_string(),
            commit_body: String::new(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let rt: EnrichmentPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(rt.target_path, payload.target_path);
    }

    #[tokio::test]
    async fn execute_returns_not_implemented() {
        std::env::set_var(WRITEBACK_ENABLED_ENV, "true");
        let saga = WriteBackSaga::new();
        let payload = EnrichmentPayload {
            enrichment_type: EnrichmentType::OntologyPromotion,
            target_path: "ontology/test.ttl".to_string(),
            content: String::new(),
            commit_subject: "test".to_string(),
            commit_body: String::new(),
        };
        let decision = DecisionReport {
            case_id: "case-001".to_string(),
            decision: "approve".to_string(),
            proposed_by: "did:nostr:aaa".to_string(),
            approved_by: "did:nostr:bbb".to_string(),
            reasoning: "test reasoning".to_string(),
            server_did: "did:nostr:ccc".to_string(),
            entity_urn: "urn:visionclaw:concept:test:node".to_string(),
        };
        let result = saga.execute("remote-1", &payload, &decision).await;
        assert!(matches!(result, Err(WriteBackError::NotImplemented(_))));
        std::env::remove_var(WRITEBACK_ENABLED_ENV);
    }

    #[tokio::test]
    async fn execute_rejects_when_globally_disabled() {
        std::env::remove_var(WRITEBACK_ENABLED_ENV);
        let saga = WriteBackSaga::new();
        let payload = EnrichmentPayload {
            enrichment_type: EnrichmentType::GapDetection,
            target_path: "proposals/test.md".to_string(),
            content: String::new(),
            commit_subject: "test".to_string(),
            commit_body: String::new(),
        };
        let decision = DecisionReport {
            case_id: "case-002".to_string(),
            decision: "promote".to_string(),
            proposed_by: "did:nostr:aaa".to_string(),
            approved_by: "did:nostr:bbb".to_string(),
            reasoning: "test".to_string(),
            server_did: "did:nostr:ccc".to_string(),
            entity_urn: "urn:visionclaw:concept:test:edge".to_string(),
        };
        let result = saga.execute("remote-2", &payload, &decision).await;
        assert!(matches!(result, Err(WriteBackError::GloballyDisabled)));
    }
}
