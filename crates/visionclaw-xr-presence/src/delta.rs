use serde::{Deserialize, Serialize};

use crate::types::{PoseFrame, Transform};

/// Bit mask of which transforms changed from the previous frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformMask(pub u8);

impl TransformMask {
    pub const HEAD: u8 = 0b001;
    pub const LEFT_HAND: u8 = 0b010;
    pub const RIGHT_HAND: u8 = 0b100;

    pub fn has_head(self) -> bool {
        self.0 & Self::HEAD != 0
    }
    pub fn has_left(self) -> bool {
        self.0 & Self::LEFT_HAND != 0
    }
    pub fn has_right(self) -> bool {
        self.0 & Self::RIGHT_HAND != 0
    }
    pub fn empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoseDelta {
    pub timestamp_us: u64,
    pub mask: TransformMask,
    pub head: Option<Transform>,
    pub left_hand: Option<Transform>,
    pub right_hand: Option<Transform>,
}

impl PoseDelta {
    /// Compute the delta from `prev` to `next`. Slots whose transform is
    /// bit-equal in both frames are dropped from the wire payload.
    pub fn between(prev: &PoseFrame, next: &PoseFrame) -> Self {
        let mut mask = 0u8;

        let head = if transform_eq(&prev.head, &next.head) {
            None
        } else {
            mask |= TransformMask::HEAD;
            Some(next.head)
        };

        let left_hand = match (prev.left_hand, next.left_hand) {
            (Some(a), Some(b)) if transform_eq(&a, &b) => None,
            (_, Some(b)) => {
                mask |= TransformMask::LEFT_HAND;
                Some(b)
            }
            _ => None,
        };

        let right_hand = match (prev.right_hand, next.right_hand) {
            (Some(a), Some(b)) if transform_eq(&a, &b) => None,
            (_, Some(b)) => {
                mask |= TransformMask::RIGHT_HAND;
                Some(b)
            }
            _ => None,
        };

        Self {
            timestamp_us: next.timestamp_us,
            mask: TransformMask(mask),
            head,
            left_hand,
            right_hand,
        }
    }

    /// Apply this delta on top of the last-known frame.
    pub fn apply(&self, base: &PoseFrame) -> PoseFrame {
        PoseFrame {
            timestamp_us: self.timestamp_us,
            head: self.head.unwrap_or(base.head),
            left_hand: if self.mask.has_left() {
                self.left_hand
            } else {
                base.left_hand
            },
            right_hand: if self.mask.has_right() {
                self.right_hand
            } else {
                base.right_hand
            },
        }
    }
}

fn transform_eq(a: &Transform, b: &Transform) -> bool {
    a.position == b.position && a.rotation == b.rotation
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_with(ts: u64, hp: [f32; 3]) -> PoseFrame {
        PoseFrame {
            timestamp_us: ts,
            head: Transform {
                position: hp,
                rotation: [0.0, 0.0, 0.0, 1.0],
            },
            left_hand: None,
            right_hand: None,
        }
    }

    #[test]
    fn no_change_produces_empty_mask() {
        let prev = frame_with(0, [1.0, 2.0, 3.0]);
        let next = frame_with(10_000, [1.0, 2.0, 3.0]);
        let delta = PoseDelta::between(&prev, &next);
        assert!(delta.mask.empty());
        assert!(delta.head.is_none());
    }

    #[test]
    fn head_change_sets_mask() {
        let prev = frame_with(0, [1.0, 2.0, 3.0]);
        let next = frame_with(10_000, [1.1, 2.0, 3.0]);
        let delta = PoseDelta::between(&prev, &next);
        assert!(delta.mask.has_head());
        assert!(!delta.mask.has_left());
    }

    #[test]
    fn apply_round_trip() {
        let prev = frame_with(0, [1.0, 2.0, 3.0]);
        let next = frame_with(10_000, [4.0, 5.0, 6.0]);
        let delta = PoseDelta::between(&prev, &next);
        let restored = delta.apply(&prev);
        assert_eq!(restored, next);
    }
}
