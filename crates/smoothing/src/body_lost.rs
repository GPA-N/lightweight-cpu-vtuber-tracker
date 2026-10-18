use std::time::Instant;

use nalgebra::UnitQuaternion;

/// Body parts held independently: left arm, right arm, left hand, right hand, torso.
pub const PARTS: usize = 5;

/// One rotation per body part, in the order listed above.
pub type BodyParts = [UnitQuaternion<f32>; PARTS];

#[derive(Debug, Clone)]
struct Held {
    rotation: UnitQuaternion<f32>,
    seen: Instant,
}

/// Holds each body part's last tracked rotation after it is lost, then eases it to rest.
#[derive(Debug, Default)]
pub struct BodyLost {
    last: [Option<Held>; PARTS],
}

impl BodyLost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, tracked: [Option<UnitQuaternion<f32>>; PARTS], now: Instant) -> BodyParts {
        std::array::from_fn(|i| match tracked[i] {
            Some(rotation) => {
                self.last[i] = Some(Held { rotation, seen: now });
                rotation
            }
            None => match &self.last[i] {
                Some(held) => {
                    let elapsed = now.saturating_duration_since(held.seen).as_secs_f32();
                    let ease = ((elapsed - 0.5) / 1.0).clamp(0.0, 1.0);
                    held.rotation.slerp(&UnitQuaternion::identity(), ease)
                }
                None => UnitQuaternion::identity(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Vector3;
    use std::time::Duration;

    fn rot(angle: f32) -> UnitQuaternion<f32> {
        UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle)
    }

    fn all(angle: f32) -> [Option<UnitQuaternion<f32>>; PARTS] {
        [Some(rot(angle)); PARTS]
    }

    fn none() -> [Option<UnitQuaternion<f32>>; PARTS] {
        [None; PARTS]
    }

    #[test]
    fn body_lost_follows_tracked_while_seen() {
        let base = Instant::now();
        let out = BodyLost::new().update(all(0.6), base);
        for part in out {
            assert_eq!(part, rot(0.6));
        }
    }

    #[test]
    fn body_lost_holds_last_pose_at_0_4s() {
        let base = Instant::now();
        let mut lost = BodyLost::new();
        lost.update(all(0.6), base);
        let out = lost.update(none(), base + Duration::from_millis(400));
        for part in out {
            assert!((part.angle() - 0.6).abs() < 1e-4);
        }
    }

    #[test]
    fn body_lost_eases_partway_at_1_0s() {
        let base = Instant::now();
        let mut lost = BodyLost::new();
        lost.update(all(0.6), base);
        let out = lost.update(none(), base + Duration::from_millis(1000));
        for part in out {
            assert!((part.angle() - 0.3).abs() < 1e-3, "angle {}", part.angle());
        }
    }

    #[test]
    fn body_lost_is_neutral_at_1_5s() {
        let base = Instant::now();
        let mut lost = BodyLost::new();
        lost.update(all(0.6), base);
        let out = lost.update(none(), base + Duration::from_millis(1500));
        for part in out {
            assert!(part.angle().abs() < 1e-5);
        }
    }

    #[test]
    fn body_lost_returns_to_tracked_when_seen_again() {
        let base = Instant::now();
        let mut lost = BodyLost::new();
        lost.update(all(0.6), base);
        lost.update(none(), base);
        let out = lost.update(all(-0.4), base + Duration::from_millis(1000));
        for part in out {
            assert_eq!(part, rot(-0.4));
        }
    }

    #[test]
    fn body_lost_parts_are_independent() {
        let base = Instant::now();
        let mut lost = BodyLost::new();
        lost.update(all(0.6), base);
        let later = base + Duration::from_millis(1000);
        let out = lost.update(
            [Some(rot(-0.4)), None, Some(rot(0.6)), None, None],
            later,
        );
        assert_eq!(out[0], rot(-0.4));
        assert_eq!(out[2], rot(0.6));
        for i in [1, 3, 4] {
            assert!((out[i].angle() - 0.3).abs() < 1e-3, "part {i} angle {}", out[i].angle());
        }
    }

    #[test]
    fn body_lost_without_history_is_neutral() {
        let out = BodyLost::new().update(none(), Instant::now());
        for part in out {
            assert_eq!(part, UnitQuaternion::identity());
        }
    }
}
