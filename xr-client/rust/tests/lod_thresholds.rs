use visionclaw_xr_gdext::lod::{
    classify, classify_squared, distance_squared, LodLevel, LodPolicyState, HIGH_DISTANCE_M,
    LOW_DISTANCE_M, MEDIUM_DISTANCE_M, RECOMPUTE_INTERVAL_FRAMES,
};

#[test]
fn boundary_values_match_browser_port() {
    assert_eq!(classify(HIGH_DISTANCE_M - 0.001), LodLevel::High);
    assert_eq!(classify(HIGH_DISTANCE_M), LodLevel::Medium);
    assert_eq!(classify(MEDIUM_DISTANCE_M - 0.001), LodLevel::Medium);
    assert_eq!(classify(MEDIUM_DISTANCE_M), LodLevel::Low);
    assert_eq!(classify(LOW_DISTANCE_M - 0.001), LodLevel::Low);
    assert_eq!(classify(LOW_DISTANCE_M), LodLevel::Culled);
}

#[test]
fn squared_classify_round_trips() {
    for d in [0.1f32, 4.99, 5.0, 14.99, 15.0, 29.99, 30.0, 100.0] {
        let sq = d * d;
        assert_eq!(classify(d), classify_squared(sq), "mismatch at {d}");
    }
}

#[test]
fn distance_squared_is_symmetric() {
    let a = [1.0, 2.0, 3.0];
    let b = [4.0, 5.0, 6.0];
    let d_ab = distance_squared(a, b);
    let d_ba = distance_squared(b, a);
    assert!((d_ab - d_ba).abs() < f32::EPSILON);
    assert!((d_ab - 27.0).abs() < 1e-3);
}

#[test]
fn recompute_cadence_matches_arch_doc() {
    let mut s = LodPolicyState::new();
    let mut recomputes = 0u32;
    for _ in 0..(RECOMPUTE_INTERVAL_FRAMES * 5) {
        if s.tick() {
            recomputes += 1;
        }
    }
    assert_eq!(recomputes, 5);
}

#[test]
fn classify_avatars_returns_one_level_per_input() {
    let mut s = LodPolicyState::new();
    let avatars = vec![[0.0, 0.0, 1.0]; 7];
    let levels = s.classify_avatars([0.0, 0.0, 0.0], &avatars).to_vec();
    assert_eq!(levels.len(), 7);
}
