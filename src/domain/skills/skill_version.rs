//! `SkillVersion` — immutable-after-benchmark version entity under `SkillPackage`.
//!
//! BC19 invariant 1: a `SkillVersion` is immutable once a benchmark has been
//! recorded against it. Prior to that, `Draft` / `Personal` versions may be
//! mutated by the maintainer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::lifecycle::SkillLifecycleState;

/// Content hash of the `SkillVersion` for signature verification and dedup.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkillFingerprint(pub String);

impl SkillFingerprint {
    pub fn new<S: Into<String>>(hex: S) -> Self {
        Self(hex.into())
    }
}

/// Ordered tool-call sequence with arg templates and expected shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolSequenceSpec {
    pub steps: Vec<ToolStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolStep {
    pub tool: String,
    /// Free-form JSON template encoded as a string for portability.
    pub args_template: String,
}

#[derive(Debug, Clone, Error)]
pub enum SkillVersionError {
    #[error("version is immutable after a benchmark has been attached")]
    ImmutableAfterBenchmark,

    #[error("cannot benchmark a version in lifecycle state {0:?}")]
    WrongLifecycleForBenchmark(SkillLifecycleState),

    #[error("fingerprint mismatch on signed install")]
    FingerprintMismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    pub version_id: String,
    /// Semver string. We keep the crate dependency-free; format is validated at MCP ingress.
    pub version: String,
    pub changelog: String,
    pub tool_sequence: ToolSequenceSpec,
    pub prerequisites: Vec<String>,
    pub pod_uri: String,
    pub fingerprint: SkillFingerprint,
    pub signature: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Once this is Some, the version is frozen. Invariant 1.
    pub benchmarked_at: Option<DateTime<Utc>>,
}

impl SkillVersion {
    pub fn new(
        version_id: String,
        version: String,
        tool_sequence: ToolSequenceSpec,
        pod_uri: String,
        fingerprint: SkillFingerprint,
    ) -> Self {
        Self {
            version_id,
            version,
            changelog: String::new(),
            tool_sequence,
            prerequisites: Vec::new(),
            pod_uri,
            fingerprint,
            signature: None,
            created_at: Utc::now(),
            benchmarked_at: None,
        }
    }

    /// Returns true if the version has been benchmarked (and is therefore immutable).
    pub fn is_frozen(&self) -> bool {
        self.benchmarked_at.is_some()
    }

    /// Mutating operations must check this guard. Callers bubble the error up.
    pub fn ensure_mutable(&self) -> Result<(), SkillVersionError> {
        if self.is_frozen() {
            Err(SkillVersionError::ImmutableAfterBenchmark)
        } else {
            Ok(())
        }
    }

    /// Idempotent freeze operation. Subsequent benchmark events touch
    /// benchmark history, not the version record.
    pub fn mark_benchmarked(&mut self, when: DateTime<Utc>) {
        if self.benchmarked_at.is_none() {
            self.benchmarked_at = Some(when);
        }
    }

    /// Update the changelog — allowed only while mutable.
    pub fn set_changelog(&mut self, text: String) -> Result<(), SkillVersionError> {
        self.ensure_mutable()?;
        self.changelog = text;
        Ok(())
    }

    /// Attach a Nostr signature — allowed only while mutable.
    pub fn attach_signature(&mut self, sig: String) -> Result<(), SkillVersionError> {
        self.ensure_mutable()?;
        self.signature = Some(sig);
        Ok(())
    }
}
