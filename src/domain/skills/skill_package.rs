//! `SkillPackage` — aggregate root for BC19.
//!
//! Enforces all BC19 invariants and owns the lifecycle transitions for its
//! versions, benchmarks, and distribution.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use super::lifecycle::{
    self, BenchmarkSummary, LifecycleTransitionError, SkillLifecycleState, TransitionContext,
};
use super::skill_benchmark::SkillBenchmark;
use super::skill_distribution::{DistributionScope, SkillDistribution};
use super::skill_version::{SkillFingerprint, SkillVersion, SkillVersionError};

#[derive(Debug, Clone, Error)]
pub enum SkillPackageError {
    #[error("unknown version_id: {0}")]
    UnknownVersion(String),

    #[error("lifecycle transition failed: {0}")]
    Lifecycle(#[from] LifecycleTransitionError),

    #[error("version mutation failed: {0}")]
    Version(#[from] SkillVersionError),

    #[error("distribution widening requires policy evaluation (BC19 inv. 5)")]
    DistributionWideningRequiresPolicy,

    #[error("cannot promote to MeshCandidate without an external ShareIntent (BC19 inv. 4)")]
    PromotionRequiresShareIntent,

    #[error("SkillFingerprint mismatch on benchmark attach")]
    FingerprintMismatch,
}

/// Aggregate root. All mutations to entities under this package happen
/// through methods here, so the invariants remain centralised.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPackage {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub maintainer_webid: String,
    pub distribution: SkillDistribution,
    pub lifecycle_state: SkillLifecycleState,
    pub pod_uri: String,
    pub current_version_id: Option<String>,
    pub versions: HashMap<String, SkillVersion>,
    /// Indexed by `version_id` for guard queries.
    pub benchmarks: HashMap<String, Vec<SkillBenchmark>>,
}

impl SkillPackage {
    pub fn new(
        skill_id: String,
        name: String,
        description: String,
        category: String,
        maintainer_webid: String,
        pod_uri: String,
    ) -> Self {
        Self {
            skill_id,
            name,
            description,
            category,
            maintainer_webid,
            distribution: SkillDistribution::personal(),
            lifecycle_state: SkillLifecycleState::Draft,
            pod_uri,
            current_version_id: None,
            versions: HashMap::new(),
            benchmarks: HashMap::new(),
        }
    }

    /// Add a new version; only permitted while the package is mutable.
    /// A `Draft` or `Personal` package accepts new or replaced versions.
    pub fn upsert_version(&mut self, version: SkillVersion) -> Result<(), SkillPackageError> {
        if let Some(existing) = self.versions.get(&version.version_id) {
            existing.ensure_mutable()?;
        }
        let vid = version.version_id.clone();
        self.versions.insert(vid.clone(), version);
        self.current_version_id.get_or_insert(vid);
        Ok(())
    }

    /// Attach a benchmark — freezes the referenced version per BC19 inv. 1.
    /// Returns the benchmark count for the version afterwards.
    pub fn attach_benchmark(
        &mut self,
        benchmark: SkillBenchmark,
    ) -> Result<usize, SkillPackageError> {
        let version = self
            .versions
            .get_mut(&benchmark.version_ref)
            .ok_or_else(|| SkillPackageError::UnknownVersion(benchmark.version_ref.clone()))?;
        version.mark_benchmarked(benchmark.run_at);
        let entry = self.benchmarks.entry(benchmark.version_ref.clone()).or_default();
        entry.push(benchmark);
        Ok(entry.len())
    }

    /// Change distribution scope. Widenings require `policy_approved=true`.
    pub fn set_distribution(
        &mut self,
        new_dist: SkillDistribution,
        policy_approved: bool,
    ) -> Result<(), SkillPackageError> {
        if self.distribution.scope.is_widening_to(new_dist.scope) && !policy_approved {
            return Err(SkillPackageError::DistributionWideningRequiresPolicy);
        }
        self.distribution = new_dist;
        Ok(())
    }

    /// Transition the lifecycle. Guards defined in [`super::lifecycle::transition`].
    ///
    /// Callers provide `share_intent_present` to honour BC19 inv. 4 (promotion
    /// to `MeshCandidate` requires a BC18 `ShareIntent`).
    pub fn transition(
        &mut self,
        target: SkillLifecycleState,
        share_intent_present: bool,
    ) -> Result<SkillLifecycleState, SkillPackageError> {
        if target == SkillLifecycleState::MeshCandidate && !share_intent_present {
            return Err(SkillPackageError::PromotionRequiresShareIntent);
        }

        let now = Utc::now();
        let passing: Vec<BenchmarkSummary> = self
            .benchmarks
            .values()
            .flatten()
            .filter(|b| b.passed())
            .map(|b| BenchmarkSummary {
                benchmark_id: b.benchmark_id.clone(),
                run_at: b.run_at,
                passed: true,
            })
            .collect();

        let ctx = TransitionContext {
            passing_benchmarks: passing,
            broker_review_ok: share_intent_present, // broker marker piggy-backs share intent in the scaffold
            successor_ref: None,
            base_model_absorbed: false,
            now,
        };

        let next = lifecycle::transition(self.lifecycle_state, target, &ctx)?;
        self.lifecycle_state = next;
        Ok(next)
    }

    /// Retire the package. Needs a successor or `BaseModelAbsorbed`.
    pub fn retire(
        &mut self,
        successor_ref: Option<String>,
        base_model_absorbed: bool,
    ) -> Result<SkillLifecycleState, SkillPackageError> {
        let ctx = TransitionContext {
            passing_benchmarks: Vec::new(),
            broker_review_ok: false,
            successor_ref,
            base_model_absorbed,
            now: Utc::now(),
        };
        let next = lifecycle::transition(self.lifecycle_state, SkillLifecycleState::Retired, &ctx)?;
        self.lifecycle_state = next;
        Ok(next)
    }

    /// Verify fingerprint on install. BC19 inv. 6.
    pub fn verify_fingerprint(
        &self,
        version_id: &str,
        expected: &SkillFingerprint,
    ) -> Result<(), SkillPackageError> {
        let v = self
            .versions
            .get(version_id)
            .ok_or_else(|| SkillPackageError::UnknownVersion(version_id.to_string()))?;
        if &v.fingerprint != expected {
            return Err(SkillPackageError::FingerprintMismatch);
        }
        Ok(())
    }
}

/// Default installers of BC19 may wish to know the canonical distribution scope
/// that corresponds to a lifecycle state for display; no invariant is enforced
/// here beyond a loose mapping.
pub fn default_scope_for(state: SkillLifecycleState) -> DistributionScope {
    match state {
        SkillLifecycleState::Draft | SkillLifecycleState::Personal => DistributionScope::Personal,
        SkillLifecycleState::TeamShared | SkillLifecycleState::Benchmarked => {
            DistributionScope::Team
        }
        SkillLifecycleState::MeshCandidate
        | SkillLifecycleState::Promoted
        | SkillLifecycleState::Retired => DistributionScope::Public,
    }
}
