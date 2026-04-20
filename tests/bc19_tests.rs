//! BC19 Skill Lifecycle tests.
//!
//! Covers:
//! * Lifecycle state transitions + guards (immutable-after-benchmark,
//!   TeamShared baseline, MeshCandidate 3-benchmark-30-day + broker).
//! * Evaluation gate enforcement per spec §8.3.
//! * [`SkillCompatibilityScanner`] semaphore fan-out cap (PRD-003 §14 R12).

use async_trait::async_trait;
use chrono::{Duration, Utc};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration as TokioDuration};

use webxr::actors::{
    BenchmarkDispatcher, ScanAllInstalled, SkillCompatibilityScanner,
    SkillCompatibilityScannerConfig,
};
use webxr::domain::skills::{
    lifecycle::{
        transition, BenchmarkSummary, LifecycleTransitionError, SkillLifecycleState,
        TransitionContext,
    },
    skill_benchmark::{EvalVerdict, SkillBenchmark},
    skill_distribution::{DistributionScope, SkillDistribution},
    skill_eval_suite::{EvalCase, GraderKind, SkillEvalSuite},
    skill_package::SkillPackage,
    skill_version::{SkillFingerprint, SkillVersion, ToolSequenceSpec},
};

// ───────────── helpers ─────────────

fn base_pkg() -> SkillPackage {
    SkillPackage::new(
        "skill-001".to_string(),
        "market-analysis-brief".to_string(),
        "test skill".to_string(),
        "research.finance".to_string(),
        "https://alice.pods.visionclaw.org/profile/card#me".to_string(),
        "https://alice.pods.visionclaw.org/public/skills/market-analysis-brief/".to_string(),
    )
}

fn draft_version(id: &str) -> SkillVersion {
    SkillVersion::new(
        id.to_string(),
        "1.0.0".to_string(),
        ToolSequenceSpec::default(),
        "pod://v1".to_string(),
        SkillFingerprint::new("sha256:abcdef"),
    )
}

fn passing_benchmark(id: &str, version_ref: &str, days_ago: i64) -> SkillBenchmark {
    SkillBenchmark {
        benchmark_id: id.to_string(),
        suite_id: "suite-1".to_string(),
        version_ref: version_ref.to_string(),
        pass_rate: 0.95,
        verdict: EvalVerdict::Pass,
        baseline: None,
        drift_score: 0.0,
        run_at: Utc::now() - Duration::days(days_ago),
        model_id: "claude-sonnet-4-7".to_string(),
        tier: 3,
    }
}

// ───────────── lifecycle transitions ─────────────

#[test]
fn lifecycle_draft_to_personal_ok() {
    let ctx = TransitionContext {
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let out = transition(SkillLifecycleState::Draft, SkillLifecycleState::Personal, &ctx).unwrap();
    assert_eq!(out, SkillLifecycleState::Personal);
}

#[test]
fn lifecycle_personal_to_team_requires_passing_baseline() {
    let ctx = TransitionContext {
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let err = transition(
        SkillLifecycleState::Personal,
        SkillLifecycleState::TeamShared,
        &ctx,
    )
    .unwrap_err();
    assert!(matches!(err, LifecycleTransitionError::TeamSharedNeedsPassingBaseline));
}

#[test]
fn lifecycle_personal_to_team_ok_with_recent_pass() {
    let ctx = TransitionContext {
        passing_benchmarks: vec![BenchmarkSummary {
            benchmark_id: "b1".to_string(),
            run_at: Utc::now() - Duration::days(5),
            passed: true,
        }],
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let out = transition(
        SkillLifecycleState::Personal,
        SkillLifecycleState::TeamShared,
        &ctx,
    )
    .unwrap();
    assert_eq!(out, SkillLifecycleState::TeamShared);
}

#[test]
fn lifecycle_stale_benchmark_fails_team_gate() {
    let ctx = TransitionContext {
        passing_benchmarks: vec![BenchmarkSummary {
            benchmark_id: "b1".to_string(),
            run_at: Utc::now() - Duration::days(40),
            passed: true,
        }],
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let err = transition(
        SkillLifecycleState::Personal,
        SkillLifecycleState::TeamShared,
        &ctx,
    )
    .unwrap_err();
    assert!(matches!(err, LifecycleTransitionError::TeamSharedNeedsPassingBaseline));
}

#[test]
fn lifecycle_benchmarked_to_mesh_candidate_needs_three_benchmarks_and_broker() {
    // Only one passing benchmark, no broker review.
    let one = TransitionContext {
        passing_benchmarks: vec![BenchmarkSummary {
            benchmark_id: "b1".to_string(),
            run_at: Utc::now() - Duration::days(1),
            passed: true,
        }],
        broker_review_ok: false,
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let err = transition(
        SkillLifecycleState::Benchmarked,
        SkillLifecycleState::MeshCandidate,
        &one,
    )
    .unwrap_err();
    assert!(matches!(err, LifecycleTransitionError::MeshCandidateNeedsBenchmarks));

    // Three fresh benchmarks, no broker.
    let three_no_broker = TransitionContext {
        passing_benchmarks: (0..3)
            .map(|i| BenchmarkSummary {
                benchmark_id: format!("b{i}"),
                run_at: Utc::now() - Duration::days(i as i64),
                passed: true,
            })
            .collect(),
        broker_review_ok: false,
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let err = transition(
        SkillLifecycleState::Benchmarked,
        SkillLifecycleState::MeshCandidate,
        &three_no_broker,
    )
    .unwrap_err();
    assert!(matches!(err, LifecycleTransitionError::MeshCandidateNeedsBrokerReview));

    // Three fresh benchmarks + broker — pass.
    let three_with_broker = TransitionContext {
        broker_review_ok: true,
        ..three_no_broker
    };
    let ok = transition(
        SkillLifecycleState::Benchmarked,
        SkillLifecycleState::MeshCandidate,
        &three_with_broker,
    )
    .unwrap();
    assert_eq!(ok, SkillLifecycleState::MeshCandidate);
}

#[test]
fn lifecycle_retirement_needs_successor_or_absorbed() {
    let bare = TransitionContext {
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let err = transition(SkillLifecycleState::Promoted, SkillLifecycleState::Retired, &bare)
        .unwrap_err();
    assert!(matches!(err, LifecycleTransitionError::RetirementNeedsJustification));

    let absorbed = TransitionContext {
        base_model_absorbed: true,
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let ok = transition(
        SkillLifecycleState::Promoted,
        SkillLifecycleState::Retired,
        &absorbed,
    )
    .unwrap();
    assert_eq!(ok, SkillLifecycleState::Retired);
}

#[test]
fn lifecycle_retired_is_terminal() {
    let ctx = TransitionContext {
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let err = transition(
        SkillLifecycleState::Retired,
        SkillLifecycleState::Promoted,
        &ctx,
    )
    .unwrap_err();
    assert!(matches!(err, LifecycleTransitionError::TerminalState));
}

#[test]
fn lifecycle_cannot_skip_states() {
    let ctx = TransitionContext {
        now: Utc::now(),
        ..TransitionContext::default()
    };
    let err = transition(
        SkillLifecycleState::Draft,
        SkillLifecycleState::TeamShared,
        &ctx,
    )
    .unwrap_err();
    assert!(matches!(err, LifecycleTransitionError::IllegalTransition { .. }));
}

// ───────────── SkillVersion immutability ─────────────

#[test]
fn skill_version_is_mutable_before_benchmark_and_frozen_after() {
    let mut v = draft_version("v1");
    v.set_changelog("initial draft".to_string()).unwrap();
    assert!(!v.is_frozen());

    v.mark_benchmarked(Utc::now());
    assert!(v.is_frozen());

    let err = v.set_changelog("edit".to_string()).unwrap_err();
    assert!(matches!(
        err,
        webxr::domain::skills::skill_version::SkillVersionError::ImmutableAfterBenchmark
    ));
}

// ───────────── SkillPackage aggregate invariants ─────────────

#[test]
fn package_attach_benchmark_freezes_version() {
    let mut pkg = base_pkg();
    pkg.upsert_version(draft_version("v1")).unwrap();
    pkg.attach_benchmark(passing_benchmark("bm1", "v1", 1)).unwrap();
    assert!(pkg.versions.get("v1").unwrap().is_frozen());
}

#[test]
fn package_promote_to_mesh_candidate_requires_share_intent() {
    let mut pkg = base_pkg();
    pkg.upsert_version(draft_version("v1")).unwrap();
    pkg.attach_benchmark(passing_benchmark("b1", "v1", 1)).unwrap();
    pkg.attach_benchmark(passing_benchmark("b2", "v1", 2)).unwrap();
    pkg.attach_benchmark(passing_benchmark("b3", "v1", 3)).unwrap();

    // Walk: Draft → Personal → TeamShared → Benchmarked.
    pkg.transition(SkillLifecycleState::Personal, false).unwrap();
    pkg.transition(SkillLifecycleState::TeamShared, false).unwrap();
    pkg.transition(SkillLifecycleState::Benchmarked, false).unwrap();

    // No share intent — rejected.
    let err = pkg
        .transition(SkillLifecycleState::MeshCandidate, false)
        .unwrap_err();
    assert!(matches!(
        err,
        webxr::domain::skills::skill_package::SkillPackageError::PromotionRequiresShareIntent
    ));

    // With share intent — accepted.
    let next = pkg
        .transition(SkillLifecycleState::MeshCandidate, true)
        .unwrap();
    assert_eq!(next, SkillLifecycleState::MeshCandidate);
}

#[test]
fn package_distribution_widening_requires_policy_approval() {
    let mut pkg = base_pkg();
    let widened = SkillDistribution {
        scope: DistributionScope::Public,
        allow_list: Vec::new(),
        group_ref: None,
        wac_refs: Vec::new(),
    };
    let err = pkg.set_distribution(widened.clone(), false).unwrap_err();
    assert!(matches!(
        err,
        webxr::domain::skills::skill_package::SkillPackageError::DistributionWideningRequiresPolicy
    ));
    pkg.set_distribution(widened, true).unwrap();
    assert_eq!(pkg.distribution.scope, DistributionScope::Public);
}

#[test]
fn package_fingerprint_mismatch_is_detected() {
    let mut pkg = base_pkg();
    pkg.upsert_version(draft_version("v1")).unwrap();
    let wrong = SkillFingerprint::new("sha256:deadbeef");
    let err = pkg.verify_fingerprint("v1", &wrong).unwrap_err();
    assert!(matches!(
        err,
        webxr::domain::skills::skill_package::SkillPackageError::FingerprintMismatch
    ));
}

// ───────────── eval suite ─────────────

#[test]
fn eval_suite_cases_count() {
    let mut s = SkillEvalSuite::new("suite-1".to_string(), 3, GraderKind::Hybrid);
    assert_eq!(s.case_count(), 0);
    s.add_case(EvalCase {
        id: "t1".to_string(),
        prompt: "brief on VOD.L".to_string(),
        assertions_json: "[]".to_string(),
        grader: GraderKind::Executor,
        tier_budget: 3,
    });
    assert_eq!(s.case_count(), 1);
}

// ───────────── benchmark invariants ─────────────

#[test]
fn benchmark_non_initial_without_baseline_fails() {
    let b = passing_benchmark("b2", "v2", 1);
    // has_prior_version = true triggers BC19 inv. 7
    assert!(b.validate(true).is_err());
    // first benchmark is ok
    assert!(b.validate(false).is_ok());
}

// ───────────── scanner concurrency ─────────────

/// Counting dispatcher: records concurrent invocations and sleeps long enough
/// to observe the semaphore's back-pressure.
struct CountingDispatcher {
    in_flight: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    hold_ms: u64,
}

#[async_trait]
impl BenchmarkDispatcher for CountingDispatcher {
    async fn dispatch(&self, _skill_id: &str) -> Result<(), String> {
        let cur = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        let mut prev = self.peak.load(Ordering::SeqCst);
        while cur > prev {
            match self.peak.compare_exchange(prev, cur, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(observed) => prev = observed,
            }
        }
        sleep(TokioDuration::from_millis(self.hold_ms)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

#[actix_rt::test]
async fn scanner_caps_parallel_dispatch_at_semaphore_limit() {
    let permits = 8usize;
    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let dispatcher = Arc::new(CountingDispatcher {
        in_flight: in_flight.clone(),
        peak: peak.clone(),
        hold_ms: 30,
    });

    let scanner = SkillCompatibilityScanner::new(SkillCompatibilityScannerConfig {
        max_parallel: permits,
        dispatch_jitter_ms: 0,
    })
    .with_dispatcher(dispatcher)
    .start();

    let ids: Vec<String> = (0..64).map(|i| format!("skill-{i}")).collect();
    let count = scanner
        .send(ScanAllInstalled {
            skill_ids: ids,
            affected_tiers: vec![2, 3],
        })
        .await
        .unwrap();

    assert_eq!(count, 64, "every skill must be dispatched");
    let observed_peak = peak.load(Ordering::SeqCst);
    assert!(
        observed_peak <= permits,
        "dispatcher peak {observed_peak} exceeded semaphore cap {permits}"
    );
    assert!(
        observed_peak >= 2,
        "dispatcher peak was suspiciously low ({observed_peak}); concurrency is not engaging"
    );
}

#[actix_rt::test]
async fn scanner_default_concurrency_is_eight() {
    let cfg = SkillCompatibilityScannerConfig::default();
    assert_eq!(cfg.max_parallel, 8, "PRD-003 §14 R12 default");
}
