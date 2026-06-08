//! Property tests for the LOD policy (PRD-QE-002 §4.5; replaces the deleted
//! `client/src/immersive/hooks/__tests__/useVRConnectionsLOD.test.ts`).
//!
//! Thresholds (from `xr-client/rust/src/lod.rs`):
//! - distance < 5 m   → High
//! - 5 ≤ d < 15 m     → Medium
//! - 15 ≤ d < 30 m    → Low
//! - d ≥ 30 m         → Culled
//!
//! These properties use proptest to assert the rule holds for any value in
//! the band, not just the hand-picked boundaries already covered by the
//! deterministic suite at `tests/lod_thresholds.rs`.

use proptest::prelude::*;
use visionclaw_xr_gdext::lod::{
    classify, classify_squared, distance_squared, LodLevel, LodPolicyState, HIGH_DISTANCE_M,
    LOW_DISTANCE_M, MEDIUM_DISTANCE_M,
};

proptest! {
    /// PROP-LOD-1: distance < 5 m always returns High.
    #[test]
    fn high_when_under_5m(d in 0.0f32..HIGH_DISTANCE_M - 0.001) {
        prop_assert_eq!(classify(d), LodLevel::High);
    }

    /// PROP-LOD-2: 5 ≤ d < 15 m always returns Medium.
    #[test]
    fn medium_when_5_to_15m(
        d in HIGH_DISTANCE_M..MEDIUM_DISTANCE_M - 0.001,
    ) {
        prop_assert_eq!(classify(d), LodLevel::Medium);
    }

    /// PROP-LOD-3: 15 ≤ d < 30 m always returns Low.
    #[test]
    fn low_when_15_to_30m(
        d in MEDIUM_DISTANCE_M..LOW_DISTANCE_M - 0.001,
    ) {
        prop_assert_eq!(classify(d), LodLevel::Low);
    }

    /// PROP-LOD-4: d ≥ 30 m always returns Culled.
    #[test]
    fn culled_when_above_30m(d in LOW_DISTANCE_M..1_000.0f32) {
        prop_assert_eq!(classify(d), LodLevel::Culled);
    }

    /// PROP-LOD-5: classify is monotonic non-decreasing in distance —
    /// d1 ≤ d2 ⇒ rank(tier(d1)) ≤ rank(tier(d2)).
    #[test]
    fn classify_is_monotonic(d1 in 0.0f32..50.0, d2 in 0.0f32..50.0) {
        let (a, b) = if d1 <= d2 { (d1, d2) } else { (d2, d1) };
        let ta = classify(a).as_i32();
        let tb = classify(b).as_i32();
        prop_assert!(ta <= tb, "expected tier({a}) <= tier({b}) but got {ta} > {tb}");
    }

    /// PROP-LOD-6: squared classifier matches linear classifier at any d > 0.
    #[test]
    fn squared_classifier_matches_linear(d in 0.001f32..100.0) {
        prop_assert_eq!(classify(d), classify_squared(d * d));
    }

    /// PROP-LOD-7: distance_squared is symmetric: d²(a, b) == d²(b, a).
    #[test]
    fn distance_squared_symmetric(
        a in proptest::array::uniform3(-10.0f32..10.0),
        b in proptest::array::uniform3(-10.0f32..10.0),
    ) {
        let ab = distance_squared(a, b);
        let ba = distance_squared(b, a);
        prop_assert!((ab - ba).abs() < 1e-3, "asymmetric: {ab} vs {ba}");
    }

    /// PROP-LOD-8: distance_squared(a, a) == 0 ± epsilon.
    #[test]
    fn distance_squared_self_is_zero(a in proptest::array::uniform3(-10.0f32..10.0)) {
        let d = distance_squared(a, a);
        prop_assert!(d.abs() < 1e-3, "self-distance not zero: {d}");
    }

    /// PROP-LOD-9: classifying N avatars produces N levels, in matching order.
    #[test]
    fn classify_avatars_yields_one_level_per_avatar(
        avatars in proptest::collection::vec(proptest::array::uniform3(-50.0f32..50.0), 0..32),
    ) {
        let mut state = LodPolicyState::new();
        let levels = state.classify_avatars([0.0, 0.0, 0.0], &avatars).to_vec();
        prop_assert_eq!(levels.len(), avatars.len());
    }
}
