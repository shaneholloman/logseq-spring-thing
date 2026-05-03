//! Hand-tracking ray cast and pinch detection. Thresholds ported verbatim
//! from `client/src/immersive/hooks/useVRHandTracking.ts` so behaviour matches
//! the deprecated browser path until QE re-grounds them.

use tracing::trace;

#[cfg(not(test))]
use godot::prelude::*;

pub const MAX_RAY_DISTANCE_M: f32 = 30.0;
pub const TARGET_RADIUS_M: f32 = 1.0;
pub const ACTIVATION_THRESHOLD: f32 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandRay {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub pinch_strength: f32,
    pub is_tracking: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetCandidate {
    pub node_id: u32,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaycastHit {
    pub node_id: u32,
    pub distance: f32,
}

pub fn find_target(ray: &HandRay, candidates: &[TargetCandidate]) -> Option<RaycastHit> {
    if !ray.is_tracking {
        return None;
    }
    let dir = normalise(ray.direction);
    let mut best: Option<RaycastHit> = None;
    for c in candidates {
        let to = [
            c.position[0] - ray.origin[0],
            c.position[1] - ray.origin[1],
            c.position[2] - ray.origin[2],
        ];
        let along = dot(&to, &dir);
        if along <= 0.0 || along > MAX_RAY_DISTANCE_M {
            continue;
        }
        let perp_sq = sq_len(&to) - along * along;
        let r_sq = TARGET_RADIUS_M * TARGET_RADIUS_M;
        if perp_sq > r_sq {
            continue;
        }
        let candidate_hit = RaycastHit {
            node_id: c.node_id,
            distance: along,
        };
        match best {
            None => best = Some(candidate_hit),
            Some(prev) if along < prev.distance => best = Some(candidate_hit),
            _ => {}
        }
    }
    trace!(?best, "find_target result");
    best
}

pub fn is_grab_active(ray: &HandRay) -> bool {
    ray.is_tracking && ray.pinch_strength >= ACTIVATION_THRESHOLD
}

fn dot(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sq_len(a: &[f32; 3]) -> f32 {
    dot(a, a)
}

fn normalise(v: [f32; 3]) -> [f32; 3] {
    let len = sq_len(&v).sqrt();
    if len < f32::EPSILON {
        return [0.0, 0.0, -1.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

#[cfg(not(test))]
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub struct XrInteraction {
    base: Base<RefCounted>,
}

#[cfg(not(test))]
#[godot_api]
impl XrInteraction {
    #[signal]
    fn node_targeted(node_id: u32, distance: f32);

    #[signal]
    fn node_grabbed(node_id: u32, position: Vector3);

    #[signal]
    fn haptic_pulse(controller: i32, intensity: f32);

    #[func]
    fn create() -> Gd<Self> {
        Gd::from_init_fn(|base| Self { base })
    }

    #[func]
    fn evaluate_ray(
        &mut self,
        origin: Vector3,
        direction: Vector3,
        pinch_strength: f32,
        candidate_ids: PackedInt32Array,
        candidate_positions: PackedVector3Array,
    ) {
        let ray = HandRay {
            origin: [origin.x, origin.y, origin.z],
            direction: [direction.x, direction.y, direction.z],
            pinch_strength,
            is_tracking: true,
        };
        let candidates: Vec<TargetCandidate> = candidate_ids
            .as_slice()
            .iter()
            .zip(candidate_positions.as_slice().iter())
            .map(|(id, p)| TargetCandidate {
                node_id: (*id) as u32,
                position: [p.x, p.y, p.z],
            })
            .collect();
        if let Some(hit) = find_target(&ray, &candidates) {
            self.base_mut().emit_signal(
                "node_targeted",
                &[Variant::from(hit.node_id), Variant::from(hit.distance)],
            );
            if is_grab_active(&ray) {
                let pos = candidate_positions
                    .as_slice()
                    .iter()
                    .zip(candidate_ids.as_slice().iter())
                    .find(|(_, id)| (**id) as u32 == hit.node_id)
                    .map(|(p, _)| *p)
                    .unwrap_or(Vector3::new(0.0, 0.0, 0.0));
                self.base_mut().emit_signal(
                    "node_grabbed",
                    &[Variant::from(hit.node_id), Variant::from(pos)],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ray_forward() -> HandRay {
        HandRay {
            origin: [0.0, 0.0, 0.0],
            direction: [0.0, 0.0, -1.0],
            pinch_strength: 0.0,
            is_tracking: true,
        }
    }

    #[test]
    fn finds_node_directly_in_front() {
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [0.0, 0.0, -5.0],
        }];
        let hit = find_target(&ray_forward(), &candidates).unwrap();
        assert_eq!(hit.node_id, 1);
        assert!((hit.distance - 5.0).abs() < 1e-3);
    }

    #[test]
    fn ignores_node_behind_origin() {
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [0.0, 0.0, 5.0],
        }];
        assert!(find_target(&ray_forward(), &candidates).is_none());
    }

    #[test]
    fn ignores_node_outside_max_distance() {
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [0.0, 0.0, -(MAX_RAY_DISTANCE_M + 1.0)],
        }];
        assert!(find_target(&ray_forward(), &candidates).is_none());
    }

    #[test]
    fn ignores_node_outside_radius() {
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [TARGET_RADIUS_M + 0.5, 0.0, -5.0],
        }];
        assert!(find_target(&ray_forward(), &candidates).is_none());
    }

    #[test]
    fn picks_nearest_when_two_in_corridor() {
        let candidates = vec![
            TargetCandidate {
                node_id: 1,
                position: [0.0, 0.0, -10.0],
            },
            TargetCandidate {
                node_id: 2,
                position: [0.0, 0.0, -3.0],
            },
        ];
        let hit = find_target(&ray_forward(), &candidates).unwrap();
        assert_eq!(hit.node_id, 2);
    }

    #[test]
    fn untracked_hand_returns_none() {
        let mut ray = ray_forward();
        ray.is_tracking = false;
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [0.0, 0.0, -5.0],
        }];
        assert!(find_target(&ray, &candidates).is_none());
    }

    #[test]
    fn pinch_below_threshold_not_grab() {
        let mut ray = ray_forward();
        ray.pinch_strength = ACTIVATION_THRESHOLD - 0.01;
        assert!(!is_grab_active(&ray));
    }

    #[test]
    fn pinch_at_threshold_is_grab() {
        let mut ray = ray_forward();
        ray.pinch_strength = ACTIVATION_THRESHOLD;
        assert!(is_grab_active(&ray));
    }

    #[test]
    fn radius_boundary_inclusive() {
        let candidates = vec![TargetCandidate {
            node_id: 1,
            position: [TARGET_RADIUS_M, 0.0, -5.0],
        }];
        let hit = find_target(&ray_forward(), &candidates);
        assert!(hit.is_some(), "node at exact target radius should hit");
    }
}
