use visionclaw_xr_gdext::interaction::{
    find_target, is_grab_active, HandRay, TargetCandidate, ACTIVATION_THRESHOLD,
    MAX_RAY_DISTANCE_M, TARGET_RADIUS_M,
};

fn forward(pinch: f32) -> HandRay {
    HandRay {
        origin: [0.0, 0.0, 0.0],
        direction: [0.0, 0.0, -1.0],
        pinch_strength: pinch,
        is_tracking: true,
    }
}

#[test]
fn boundary_at_max_distance_inclusive() {
    let candidates = vec![TargetCandidate {
        node_id: 1,
        position: [0.0, 0.0, -MAX_RAY_DISTANCE_M],
    }];
    assert!(find_target(&forward(0.0), &candidates).is_some());
}

#[test]
fn just_beyond_max_distance_misses() {
    let candidates = vec![TargetCandidate {
        node_id: 1,
        position: [0.0, 0.0, -(MAX_RAY_DISTANCE_M + 0.01)],
    }];
    assert!(find_target(&forward(0.0), &candidates).is_none());
}

#[test]
fn radius_boundary_inclusive() {
    let candidates = vec![TargetCandidate {
        node_id: 1,
        position: [TARGET_RADIUS_M, 0.0, -5.0],
    }];
    assert!(find_target(&forward(0.0), &candidates).is_some());
}

#[test]
fn just_outside_radius_misses() {
    let candidates = vec![TargetCandidate {
        node_id: 1,
        position: [TARGET_RADIUS_M + 0.01, 0.0, -5.0],
    }];
    assert!(find_target(&forward(0.0), &candidates).is_none());
}

#[test]
fn untracked_ray_returns_none_even_with_target() {
    let mut ray = forward(0.0);
    ray.is_tracking = false;
    let candidates = vec![TargetCandidate {
        node_id: 1,
        position: [0.0, 0.0, -5.0],
    }];
    assert!(find_target(&ray, &candidates).is_none());
}

#[test]
fn pinch_at_threshold_triggers_grab() {
    let ray = forward(ACTIVATION_THRESHOLD);
    assert!(is_grab_active(&ray));
}

#[test]
fn pinch_just_under_threshold_does_not_grab() {
    let ray = forward(ACTIVATION_THRESHOLD - 0.001);
    assert!(!is_grab_active(&ray));
}

#[test]
fn nearest_node_wins_when_two_along_ray() {
    let candidates = vec![
        TargetCandidate {
            node_id: 9,
            position: [0.0, 0.0, -10.0],
        },
        TargetCandidate {
            node_id: 4,
            position: [0.0, 0.0, -2.0],
        },
        TargetCandidate {
            node_id: 7,
            position: [0.0, 0.0, -25.0],
        },
    ];
    let hit = find_target(&forward(0.0), &candidates).unwrap();
    assert_eq!(hit.node_id, 4);
}
