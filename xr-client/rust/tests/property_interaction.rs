//! Property tests for hand-tracking ray cast and pinch detection
//! (PRD-QE-002 §4.5; replaces parts of the deleted
//! `client/src/immersive/hooks/__tests__/useVRHandTracking.test.ts`).
//!
//! Thresholds (from `xr-client/rust/src/interaction.rs`):
//! - `MAX_RAY_DISTANCE_M = 30.0` — anything beyond is never targeted
//! - `TARGET_RADIUS_M = 1.0` — corridor radius around the ray
//! - `ACTIVATION_THRESHOLD = 0.7` — pinch strength to trigger grab
//!
//! Boundary semantics (from existing deterministic tests):
//! - `pinch_strength == ACTIVATION_THRESHOLD` triggers grab (inclusive)
//! - `position == TARGET_RADIUS_M` is in the corridor (inclusive)
//! - `along == MAX_RAY_DISTANCE_M` still hits (inclusive)

use proptest::prelude::*;
use visionclaw_xr_gdext::interaction::{
    find_target, is_grab_active, HandRay, TargetCandidate, ACTIVATION_THRESHOLD,
    MAX_RAY_DISTANCE_M, TARGET_RADIUS_M,
};

fn forward_ray(pinch: f32, tracking: bool) -> HandRay {
    HandRay {
        origin: [0.0, 0.0, 0.0],
        direction: [0.0, 0.0, -1.0],
        pinch_strength: pinch,
        is_tracking: tracking,
    }
}

proptest! {
    /// PROP-INT-1: any returned hit has distance ≤ MAX_RAY_DISTANCE_M.
    #[test]
    fn hit_distance_within_max(
        z in -50.0f32..0.0,
        x in -2.0f32..2.0,
    ) {
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [x, 0.0, z],
        }];
        if let Some(hit) = find_target(&forward_ray(0.0, true), &candidates) {
            prop_assert!(
                hit.distance <= MAX_RAY_DISTANCE_M,
                "hit distance {} exceeds MAX {}", hit.distance, MAX_RAY_DISTANCE_M
            );
        }
    }

    /// PROP-INT-2: pinch strength below threshold never triggers a grab,
    /// regardless of which candidate is targeted.
    #[test]
    fn below_threshold_never_grabs(pinch in 0.0f32..ACTIVATION_THRESHOLD - 0.001) {
        let ray = forward_ray(pinch, true);
        prop_assert!(!is_grab_active(&ray), "pinch {pinch} grabbed below threshold");
    }

    /// PROP-INT-3: pinch strength at-or-above threshold always triggers a grab
    /// while tracking.
    #[test]
    fn at_or_above_threshold_grabs(pinch in ACTIVATION_THRESHOLD..1.0f32) {
        let ray = forward_ray(pinch, true);
        prop_assert!(is_grab_active(&ray), "pinch {pinch} did not grab at/above threshold");
    }

    /// PROP-INT-4: untracked hand never grabs even at full pinch.
    #[test]
    fn untracked_never_grabs(pinch in 0.0f32..1.0) {
        let ray = forward_ray(pinch, false);
        prop_assert!(!is_grab_active(&ray), "untracked ray grabbed at pinch={pinch}");
    }

    /// PROP-INT-5: ray cast is deterministic — identical inputs produce
    /// identical outputs (referential transparency).
    #[test]
    fn raycast_deterministic(
        z in -25.0f32..-1.0,
        x in -0.5f32..0.5,
    ) {
        let candidates = vec![TargetCandidate {
            node_id: 7,
            position: [x, 0.0, z],
        }];
        let ray = forward_ray(0.5, true);
        let h1 = find_target(&ray, &candidates);
        let h2 = find_target(&ray, &candidates);
        prop_assert_eq!(h1, h2);
    }

    /// PROP-INT-6: the nearest candidate inside the corridor wins.
    #[test]
    fn nearest_candidate_wins(
        d_near in 1.0f32..15.0,
        gap in 1.0f32..10.0,
    ) {
        let d_far = d_near + gap;
        let candidates = vec![
            TargetCandidate { node_id: 1, position: [0.0, 0.0, -d_far] },
            TargetCandidate { node_id: 2, position: [0.0, 0.0, -d_near] },
        ];
        let hit = find_target(&forward_ray(0.0, true), &candidates).expect("must hit");
        prop_assert_eq!(hit.node_id, 2, "expected nearest (id=2) to win");
    }

    /// PROP-INT-7: nodes behind the origin (along ≤ 0) never hit.
    #[test]
    fn nothing_behind_origin(z in 0.001f32..30.0) {
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [0.0, 0.0, z], // positive z is behind for forward ray
        }];
        prop_assert!(find_target(&forward_ray(0.0, true), &candidates).is_none());
    }

    /// PROP-INT-8: any candidate with perpendicular offset > TARGET_RADIUS_M
    /// is ignored.
    #[test]
    fn outside_corridor_ignored(
        offset in TARGET_RADIUS_M + 0.01..5.0f32,
        z in -25.0f32..-1.0,
    ) {
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [offset, 0.0, z],
        }];
        prop_assert!(find_target(&forward_ray(0.0, true), &candidates).is_none());
    }
}

// -- boundary tests ----------------------------------------------------------

#[test]
fn boundary_pinch_exactly_at_threshold_triggers() {
    // Documented behaviour: `pinch_strength >= ACTIVATION_THRESHOLD` triggers.
    let ray = forward_ray(ACTIVATION_THRESHOLD, true);
    assert!(is_grab_active(&ray));
}

#[test]
fn boundary_target_radius_exactly_inclusive() {
    // The implementation uses `perp_sq > r_sq` as the reject condition,
    // so equality is accepted (inclusive boundary).
    let candidates = vec![TargetCandidate {
        node_id: 1,
        position: [TARGET_RADIUS_M, 0.0, -5.0],
    }];
    assert!(find_target(&forward_ray(0.0, true), &candidates).is_some());
}

#[test]
fn boundary_max_distance_exactly_inclusive() {
    // The implementation uses `along > MAX_RAY_DISTANCE_M` as reject,
    // so equality hits.
    let candidates = vec![TargetCandidate {
        node_id: 1,
        position: [0.0, 0.0, -MAX_RAY_DISTANCE_M],
    }];
    assert!(find_target(&forward_ray(0.0, true), &candidates).is_some());
}
