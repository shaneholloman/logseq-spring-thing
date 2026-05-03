//! Distance-bucket LOD policy. Thresholds are a verbatim port of
//! `client/src/immersive/hooks/useVRConnectionsLOD.ts` so visual fidelity
//! matches the deprecated browser path. Recompute cadence is every 2 frames
//! per `xr-godot-system-architecture.md` §4.

#[cfg(not(test))]
use godot::prelude::*;

pub const HIGH_DISTANCE_M: f32 = 5.0;
pub const MEDIUM_DISTANCE_M: f32 = 15.0;
pub const LOW_DISTANCE_M: f32 = 30.0;
pub const RECOMPUTE_INTERVAL_FRAMES: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodLevel {
    High,
    Medium,
    Low,
    Culled,
}

impl LodLevel {
    pub fn as_i32(self) -> i32 {
        match self {
            LodLevel::High => 0,
            LodLevel::Medium => 1,
            LodLevel::Low => 2,
            LodLevel::Culled => 3,
        }
    }
}

pub fn classify(distance_m: f32) -> LodLevel {
    if distance_m < HIGH_DISTANCE_M {
        LodLevel::High
    } else if distance_m < MEDIUM_DISTANCE_M {
        LodLevel::Medium
    } else if distance_m < LOW_DISTANCE_M {
        LodLevel::Low
    } else {
        LodLevel::Culled
    }
}

pub fn classify_squared(distance_sq_m2: f32) -> LodLevel {
    let high_sq = HIGH_DISTANCE_M * HIGH_DISTANCE_M;
    let med_sq = MEDIUM_DISTANCE_M * MEDIUM_DISTANCE_M;
    let low_sq = LOW_DISTANCE_M * LOW_DISTANCE_M;
    if distance_sq_m2 < high_sq {
        LodLevel::High
    } else if distance_sq_m2 < med_sq {
        LodLevel::Medium
    } else if distance_sq_m2 < low_sq {
        LodLevel::Low
    } else {
        LodLevel::Culled
    }
}

pub fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

pub struct LodPolicyState {
    frame_counter: u32,
    last_levels: Vec<LodLevel>,
}

impl LodPolicyState {
    pub fn new() -> Self {
        Self {
            frame_counter: 0,
            last_levels: Vec::new(),
        }
    }

    pub fn tick(&mut self) -> bool {
        self.frame_counter = self.frame_counter.wrapping_add(1);
        self.frame_counter % RECOMPUTE_INTERVAL_FRAMES == 0
    }

    pub fn classify_avatars(
        &mut self,
        camera: [f32; 3],
        avatars: &[[f32; 3]],
    ) -> &[LodLevel] {
        self.last_levels.clear();
        for pos in avatars {
            let d_sq = distance_squared(camera, *pos);
            self.last_levels.push(classify_squared(d_sq));
        }
        &self.last_levels
    }
}

impl Default for LodPolicyState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(test))]
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub struct LodPolicy {
    state: LodPolicyState,
    base: Base<RefCounted>,
}

#[cfg(not(test))]
#[godot_api]
impl LodPolicy {
    #[func]
    fn create() -> Gd<Self> {
        Gd::from_init_fn(|base| Self {
            state: LodPolicyState::new(),
            base,
        })
    }

    #[func]
    fn should_recompute(&mut self) -> bool {
        self.state.tick()
    }

    #[func]
    fn classify_distance(&self, distance_m: f32) -> i32 {
        classify(distance_m).as_i32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_under_5m() {
        assert_eq!(classify(0.0), LodLevel::High);
        assert_eq!(classify(4.99), LodLevel::High);
    }

    #[test]
    fn medium_5_to_15m() {
        assert_eq!(classify(5.0), LodLevel::Medium);
        assert_eq!(classify(14.99), LodLevel::Medium);
    }

    #[test]
    fn low_15_to_30m() {
        assert_eq!(classify(15.0), LodLevel::Low);
        assert_eq!(classify(29.99), LodLevel::Low);
    }

    #[test]
    fn culled_above_30m() {
        assert_eq!(classify(30.0), LodLevel::Culled);
        assert_eq!(classify(1000.0), LodLevel::Culled);
    }

    #[test]
    fn squared_classify_matches_linear() {
        for d in [0.5_f32, 4.9, 5.1, 14.5, 15.5, 29.5, 30.5, 100.0] {
            assert_eq!(classify(d), classify_squared(d * d), "mismatch at {d}");
        }
    }

    #[test]
    fn tick_returns_true_every_two_frames() {
        let mut s = LodPolicyState::new();
        assert!(!s.tick(), "frame 1 should not recompute");
        assert!(s.tick(), "frame 2 should recompute");
        assert!(!s.tick(), "frame 3 should not recompute");
        assert!(s.tick(), "frame 4 should recompute");
    }

    #[test]
    fn classify_avatars_respects_camera_position() {
        let mut s = LodPolicyState::new();
        let cam = [0.0, 0.0, 0.0];
        let avatars = vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 25.0],
            [0.0, 0.0, 50.0],
        ];
        let levels = s.classify_avatars(cam, &avatars).to_vec();
        assert_eq!(
            levels,
            vec![
                LodLevel::High,
                LodLevel::Medium,
                LodLevel::Low,
                LodLevel::Culled
            ]
        );
    }
}
