//! Bead Provenance Domain Types
//!
//! Core types for the bead lifecycle: state machine, outcomes, metadata,
//! and learning entries. Inspired by NEEDLE's exhaustive outcome classification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Bead lifecycle states — explicit FSM, no implicit transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeadState {
    Created,
    Publishing,
    Published,
    Neo4jPersisted,
    Bridged,
    Archived,
    Failed(BeadFailure),
}

/// Exhaustive outcome classification — every publish attempt produces one.
/// No wildcard arms permitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeadOutcome {
    Success,
    RelayTimeout { attempts: u8 },
    RelayRejected { reason: String },
    RelayUnreachable { error: String },
    SigningFailed { error: String },
    Neo4jWriteFailed { error: String },
    BridgeFailed { error: String },
}

/// Failure classification — determines retry eligibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeadFailure {
    Transient(String),
    Permanent(String),
}

/// Extended bead metadata for full lifecycle tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadMetadata {
    pub bead_id: String,
    pub brief_id: String,
    pub debrief_path: String,
    pub user_pubkey: Option<String>,
    pub state: BeadState,
    pub outcome: Option<BeadOutcome>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub persisted_at: Option<DateTime<Utc>>,
    pub bridged_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    pub retry_count: u8,
    pub nostr_event_id: Option<String>,
}

impl BeadMetadata {
    /// Create new bead metadata in Created state.
    pub fn new(
        bead_id: String,
        brief_id: String,
        debrief_path: String,
        user_pubkey: Option<String>,
    ) -> Self {
        Self {
            bead_id,
            brief_id,
            debrief_path,
            user_pubkey,
            state: BeadState::Created,
            outcome: None,
            created_at: Utc::now(),
            published_at: None,
            persisted_at: None,
            bridged_at: None,
            archived_at: None,
            retry_count: 0,
            nostr_event_id: None,
        }
    }
}

/// Post-bead learning entry — structured retrospective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadLearning {
    pub bead_id: String,
    pub what_worked: Option<String>,
    pub what_failed: Option<String>,
    pub reusable_pattern: Option<String>,
    pub confidence: f32,
    pub recorded_at: DateTime<Utc>,
}

/// Bead health status for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadHealthStatus {
    pub relay_connected: bool,
    pub bridge_connected: bool,
    pub neo4j_connected: bool,
    pub last_publish_at: Option<DateTime<Utc>>,
    pub last_publish_outcome: Option<String>,
    pub beads_by_state: HashMap<String, u64>,
    pub relay_latency_ms: Option<u64>,
}

/// Retry configuration — loaded from environment.
#[derive(Debug, Clone)]
pub struct BeadRetryConfig {
    pub max_attempts: u8,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for BeadRetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 10_000,
            backoff_multiplier: 2.0,
        }
    }
}

impl BeadRetryConfig {
    /// Load from environment with defaults.
    pub fn from_env() -> Self {
        Self {
            max_attempts: std::env::var("BEAD_RETRY_MAX_ATTEMPTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
            base_delay_ms: std::env::var("BEAD_RETRY_BASE_DELAY_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000),
            max_delay_ms: std::env::var("BEAD_RETRY_MAX_DELAY_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10_000),
            backoff_multiplier: std::env::var("BEAD_RETRY_BACKOFF_MULTIPLIER")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2.0),
        }
    }

    /// Calculate delay for a given attempt number (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u8) -> u64 {
        let delay = (self.base_delay_ms as f64) * self.backoff_multiplier.powi(attempt as i32);
        (delay as u64).min(self.max_delay_ms)
    }
}

impl BeadOutcome {
    /// Whether this outcome represents a success.
    pub fn is_success(&self) -> bool {
        matches!(self, BeadOutcome::Success)
    }

    /// Whether the failure is transient (retryable).
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            BeadOutcome::RelayTimeout { .. } | BeadOutcome::RelayUnreachable { .. }
        )
    }

    /// Human-readable label for monitoring/logging.
    pub fn label(&self) -> &'static str {
        match self {
            BeadOutcome::Success => "Success",
            BeadOutcome::RelayTimeout { .. } => "RelayTimeout",
            BeadOutcome::RelayRejected { .. } => "RelayRejected",
            BeadOutcome::RelayUnreachable { .. } => "RelayUnreachable",
            BeadOutcome::SigningFailed { .. } => "SigningFailed",
            BeadOutcome::Neo4jWriteFailed { .. } => "Neo4jWriteFailed",
            BeadOutcome::BridgeFailed { .. } => "BridgeFailed",
        }
    }
}

impl std::fmt::Display for BeadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BeadState::Created => write!(f, "Created"),
            BeadState::Publishing => write!(f, "Publishing"),
            BeadState::Published => write!(f, "Published"),
            BeadState::Neo4jPersisted => write!(f, "Neo4jPersisted"),
            BeadState::Bridged => write!(f, "Bridged"),
            BeadState::Archived => write!(f, "Archived"),
            BeadState::Failed(failure) => write!(f, "Failed({failure})"),
        }
    }
}

impl std::fmt::Display for BeadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BeadFailure::Transient(msg) => write!(f, "Transient: {msg}"),
            BeadFailure::Permanent(msg) => write!(f, "Permanent: {msg}"),
        }
    }
}

/// Errors from BeadStore operations.
#[derive(Debug, thiserror::Error)]
pub enum BeadStoreError {
    #[error("Neo4j query failed: {0}")]
    QueryFailed(String),
    #[error("Bead not found: {0}")]
    NotFound(String),
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── BeadMetadata::new ──────────────────────────────────────────────

    #[test]
    fn new_metadata_starts_in_created_state() {
        // GIVEN: valid bead parameters
        let meta = BeadMetadata::new(
            "bead-1".into(),
            "brief-1".into(),
            "/path/debrief".into(),
            Some("pk-abc".into()),
        );

        // THEN: state is Created with zero retries and no outcome
        assert_eq!(meta.state, BeadState::Created);
        assert_eq!(meta.retry_count, 0);
        assert!(meta.outcome.is_none());
        assert!(meta.published_at.is_none());
        assert!(meta.nostr_event_id.is_none());
        assert_eq!(meta.bead_id, "bead-1");
        assert_eq!(meta.brief_id, "brief-1");
        assert_eq!(meta.debrief_path, "/path/debrief");
        assert_eq!(meta.user_pubkey, Some("pk-abc".into()));
    }

    // ── BeadRetryConfig ────────────────────────────────────────────────

    #[test]
    fn default_retry_config_has_expected_values() {
        let cfg = BeadRetryConfig::default();

        assert_eq!(cfg.max_attempts, 3);
        assert_eq!(cfg.base_delay_ms, 1000);
        assert_eq!(cfg.max_delay_ms, 10_000);
        assert!((cfg.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn delay_for_attempt_calculates_exponential_backoff() {
        // GIVEN: default config (base=1000, multiplier=2.0)
        let cfg = BeadRetryConfig::default();

        // WHEN/THEN: attempt 0 → 1000ms, attempt 1 → 2000ms, attempt 2 → 4000ms
        assert_eq!(cfg.delay_for_attempt(0), 1000);
        assert_eq!(cfg.delay_for_attempt(1), 2000);
        assert_eq!(cfg.delay_for_attempt(2), 4000);
    }

    #[test]
    fn delay_for_attempt_clamps_to_max_delay() {
        // GIVEN: config with low max_delay
        let cfg = BeadRetryConfig {
            max_attempts: 5,
            base_delay_ms: 1000,
            max_delay_ms: 3000,
            backoff_multiplier: 2.0,
        };

        // WHEN: attempt 2 would yield 4000ms
        // THEN: clamped to 3000ms
        assert_eq!(cfg.delay_for_attempt(2), 3000);
        // attempt 10 also clamped
        assert_eq!(cfg.delay_for_attempt(10), 3000);
    }

    // ── BeadOutcome ────────────────────────────────────────────────────

    #[test]
    fn is_success_returns_true_only_for_success() {
        assert!(BeadOutcome::Success.is_success());
        assert!(!BeadOutcome::RelayTimeout { attempts: 3 }.is_success());
        assert!(!BeadOutcome::RelayRejected { reason: "x".into() }.is_success());
        assert!(!BeadOutcome::SigningFailed { error: "x".into() }.is_success());
    }

    #[test]
    fn is_transient_returns_true_for_retryable_errors() {
        // GIVEN: transient outcomes
        assert!(BeadOutcome::RelayTimeout { attempts: 1 }.is_transient());
        assert!(BeadOutcome::RelayUnreachable { error: "dns".into() }.is_transient());
    }

    #[test]
    fn is_transient_returns_false_for_permanent_errors() {
        // GIVEN: permanent outcomes
        assert!(!BeadOutcome::SigningFailed { error: "bad key".into() }.is_transient());
        assert!(!BeadOutcome::RelayRejected { reason: "blocked".into() }.is_transient());
        assert!(!BeadOutcome::Neo4jWriteFailed { error: "x".into() }.is_transient());
        assert!(!BeadOutcome::BridgeFailed { error: "x".into() }.is_transient());
        assert!(!BeadOutcome::Success.is_transient());
    }

    #[test]
    fn label_returns_correct_string_for_each_variant() {
        assert_eq!(BeadOutcome::Success.label(), "Success");
        assert_eq!(BeadOutcome::RelayTimeout { attempts: 1 }.label(), "RelayTimeout");
        assert_eq!(BeadOutcome::RelayRejected { reason: "x".into() }.label(), "RelayRejected");
        assert_eq!(BeadOutcome::RelayUnreachable { error: "x".into() }.label(), "RelayUnreachable");
        assert_eq!(BeadOutcome::SigningFailed { error: "x".into() }.label(), "SigningFailed");
        assert_eq!(BeadOutcome::Neo4jWriteFailed { error: "x".into() }.label(), "Neo4jWriteFailed");
        assert_eq!(BeadOutcome::BridgeFailed { error: "x".into() }.label(), "BridgeFailed");
    }

    // ── Display impls ──────────────────────────────────────────────────

    #[test]
    fn bead_state_display_formats_correctly() {
        assert_eq!(format!("{}", BeadState::Created), "Created");
        assert_eq!(format!("{}", BeadState::Publishing), "Publishing");
        assert_eq!(format!("{}", BeadState::Published), "Published");
        assert_eq!(format!("{}", BeadState::Archived), "Archived");
        assert_eq!(
            format!("{}", BeadState::Failed(BeadFailure::Transient("timeout".into()))),
            "Failed(Transient: timeout)"
        );
        assert_eq!(
            format!("{}", BeadState::Failed(BeadFailure::Permanent("bad sig".into()))),
            "Failed(Permanent: bad sig)"
        );
    }

    #[test]
    fn bead_failure_display_formats_correctly() {
        assert_eq!(
            format!("{}", BeadFailure::Transient("network".into())),
            "Transient: network"
        );
        assert_eq!(
            format!("{}", BeadFailure::Permanent("invalid key".into())),
            "Permanent: invalid key"
        );
    }

    // ── Serialization round-trip ───────────────────────────────────────

    #[test]
    fn bead_metadata_serde_roundtrip() {
        // GIVEN: a metadata instance
        let meta = BeadMetadata::new(
            "bead-rt".into(),
            "brief-rt".into(),
            "/debrief".into(),
            None,
        );

        // WHEN: serialized and deserialized
        let json = serde_json::to_string(&meta).expect("serialize");
        let restored: BeadMetadata = serde_json::from_str(&json).expect("deserialize");

        // THEN: fields match
        assert_eq!(restored.bead_id, "bead-rt");
        assert_eq!(restored.state, BeadState::Created);
        assert!(restored.outcome.is_none());
    }

    #[test]
    fn bead_state_failed_variant_serde_roundtrip() {
        // GIVEN: a Failed state with Transient failure
        let state = BeadState::Failed(BeadFailure::Transient("relay down".into()));

        // WHEN: serialized and deserialized
        let json = serde_json::to_string(&state).expect("serialize");
        let restored: BeadState = serde_json::from_str(&json).expect("deserialize");

        // THEN: variant is preserved
        assert_eq!(restored, state);
    }

    #[test]
    fn bead_outcome_all_variants_serde_roundtrip() {
        let outcomes = vec![
            BeadOutcome::Success,
            BeadOutcome::RelayTimeout { attempts: 3 },
            BeadOutcome::RelayRejected { reason: "spam".into() },
            BeadOutcome::RelayUnreachable { error: "dns".into() },
            BeadOutcome::SigningFailed { error: "bad hex".into() },
            BeadOutcome::Neo4jWriteFailed { error: "conn".into() },
            BeadOutcome::BridgeFailed { error: "forum".into() },
        ];

        for outcome in outcomes {
            let json = serde_json::to_string(&outcome).expect("serialize");
            let restored: BeadOutcome = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(restored, outcome);
        }
    }

    // ── BeadStoreError ─────────────────────────────────────────────────

    #[test]
    fn bead_store_error_display_messages() {
        let err = BeadStoreError::NotFound("bead-42".into());
        assert_eq!(format!("{err}"), "Bead not found: bead-42");

        let err = BeadStoreError::InvalidTransition {
            from: "Created".into(),
            to: "Archived".into(),
        };
        assert!(format!("{err}").contains("Created"));
        assert!(format!("{err}").contains("Archived"));
    }
}
