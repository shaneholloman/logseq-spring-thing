use crate::error::ValidationError;
use crate::types::{Aabb, HandPose, PoseFrame, Transform};

pub const DEFAULT_MAX_VELOCITY_MPS: f32 = 20.0;
pub const DEFAULT_HAND_REACH_M: f32 = 1.2;
pub const DEFAULT_MIN_FRAME_INTERVAL_US: u64 = 8_000;
const QUAT_UNIT_LO: f32 = 0.99;
const QUAT_UNIT_HI: f32 = 1.01;

pub fn velocity_gate(
    prev: &PoseFrame,
    next: &PoseFrame,
    max_mps: f32,
) -> Result<(), ValidationError> {
    if next.timestamp_us <= prev.timestamp_us {
        return Err(ValidationError::NonMonotonicTimestamp {
            prev_us: prev.timestamp_us,
            next_us: next.timestamp_us,
        });
    }
    let dt_us = next.timestamp_us - prev.timestamp_us;
    let dt_s = (dt_us as f64) / 1_000_000.0;
    let dx = next.head.position[0] - prev.head.position[0];
    let dy = next.head.position[1] - prev.head.position[1];
    let dz = next.head.position[2] - prev.head.position[2];
    let dist = ((dx * dx + dy * dy + dz * dz) as f64).sqrt();
    let observed_mps = (dist / dt_s) as f32;
    if observed_mps > max_mps {
        return Err(ValidationError::VelocityExceeded {
            observed_mps,
            limit_mps: max_mps,
        });
    }
    Ok(())
}

pub fn world_bounds(transform: &Transform, bounds: &Aabb) -> Result<(), ValidationError> {
    if !bounds.contains(&transform.position) {
        return Err(ValidationError::OutOfBounds {
            x: transform.position[0],
            y: transform.position[1],
            z: transform.position[2],
        });
    }
    let mag = transform.quaternion_magnitude();
    if !(QUAT_UNIT_LO..=QUAT_UNIT_HI).contains(&mag) {
        return Err(ValidationError::NonUnitQuaternion {
            mag,
            lo: QUAT_UNIT_LO,
            hi: QUAT_UNIT_HI,
        });
    }
    Ok(())
}

pub fn monotonic_timestamp(prev_us: u64, next_us: u64) -> Result<(), ValidationError> {
    match next_us.cmp(&prev_us) {
        std::cmp::Ordering::Greater => {
            let dt = next_us - prev_us;
            if dt < DEFAULT_MIN_FRAME_INTERVAL_US {
                return Err(ValidationError::IntervalTooShort {
                    dt_us: dt,
                    min_us: DEFAULT_MIN_FRAME_INTERVAL_US,
                });
            }
            Ok(())
        }
        std::cmp::Ordering::Equal => Err(ValidationError::DuplicateTimestamp { ts_us: next_us }),
        std::cmp::Ordering::Less => Err(ValidationError::NonMonotonicTimestamp { prev_us, next_us }),
    }
}

/// Anatomical sanity check on hand poses. v1 enforces only the wrist quaternion
/// unit-norm; full MANO joint flexion gates per `xr-godot-threat-model.md`
/// T-HAND-1 land once the gdext hand tracker exposes joint angles.
pub fn joint_anatomy(
    left_hand: &HandPose,
    right_hand: &HandPose,
) -> Result<(), ValidationError> {
    for hand in [left_hand, right_hand] {
        let mag = hand.wrist.quaternion_magnitude();
        if !(QUAT_UNIT_LO..=QUAT_UNIT_HI).contains(&mag) {
            return Err(ValidationError::NonUnitQuaternion {
                mag,
                lo: QUAT_UNIT_LO,
                hi: QUAT_UNIT_HI,
            });
        }
        // TODO(PRD-008-followup): MANO per-joint flexion ranges once joints are populated.
        for joint in &hand.joints {
            let m = (joint[0] * joint[0]
                + joint[1] * joint[1]
                + joint[2] * joint[2]
                + joint[3] * joint[3])
                .sqrt();
            if !(QUAT_UNIT_LO..=QUAT_UNIT_HI).contains(&m) {
                return Err(ValidationError::NonUnitQuaternion {
                    mag: m,
                    lo: QUAT_UNIT_LO,
                    hi: QUAT_UNIT_HI,
                });
            }
        }
    }
    Ok(())
}

pub fn hand_reach(head: &Transform, hand: &Transform, limit_m: f32) -> Result<(), ValidationError> {
    let dx = hand.position[0] - head.position[0];
    let dy = hand.position[1] - head.position[1];
    let dz = hand.position[2] - head.position[2];
    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    if dist > limit_m {
        return Err(ValidationError::HandReachExceeded {
            observed_m: dist,
            limit_m,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pf(ts_us: u64, x: f32) -> PoseFrame {
        PoseFrame {
            timestamp_us: ts_us,
            head: Transform {
                position: [x, 0.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
            left_hand: None,
            right_hand: None,
        }
    }

    #[test]
    fn velocity_within_gate_passes() {
        let prev = pf(0, 0.0);
        let next = pf(100_000, 0.5);
        velocity_gate(&prev, &next, 20.0).unwrap();
    }

    #[test]
    fn velocity_above_gate_rejected() {
        let prev = pf(0, 0.0);
        let next = pf(10_000, 5.0);
        let err = velocity_gate(&prev, &next, 20.0).unwrap_err();
        assert!(matches!(err, ValidationError::VelocityExceeded { .. }));
    }

    #[test]
    fn out_of_bounds_rejected() {
        let bounds = Aabb::symmetric(50.0);
        let t = Transform {
            position: [100.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
        };
        assert!(matches!(
            world_bounds(&t, &bounds),
            Err(ValidationError::OutOfBounds { .. })
        ));
    }

    #[test]
    fn non_unit_quaternion_rejected() {
        let bounds = Aabb::symmetric(50.0);
        let t = Transform {
            position: [0.0, 0.0, 0.0],
            rotation: [2.0, 0.0, 0.0, 0.0],
        };
        assert!(matches!(
            world_bounds(&t, &bounds),
            Err(ValidationError::NonUnitQuaternion { .. })
        ));
    }

    #[test]
    fn monotonic_duplicate_rejected() {
        assert!(matches!(
            monotonic_timestamp(100, 100),
            Err(ValidationError::DuplicateTimestamp { .. })
        ));
    }

    #[test]
    fn monotonic_backwards_rejected() {
        assert!(matches!(
            monotonic_timestamp(200, 100),
            Err(ValidationError::NonMonotonicTimestamp { .. })
        ));
    }

    #[test]
    fn monotonic_interval_too_short_rejected() {
        assert!(matches!(
            monotonic_timestamp(0, 100),
            Err(ValidationError::IntervalTooShort { .. })
        ));
    }
}
