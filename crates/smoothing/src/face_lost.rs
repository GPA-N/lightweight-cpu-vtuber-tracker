use std::time::Instant;

use core::frame::TrackingOutput;
use nalgebra::UnitQuaternion;

/// Holds the last tracked output after the face is lost, then eases it to neutral.
#[derive(Debug, Default)]
pub struct FaceLost {
    last: Option<TrackingOutput>,
}

impl FaceLost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, tracked: Option<&TrackingOutput>, now: Instant) -> TrackingOutput {
        if let Some(found) = tracked {
            self.last = Some(found.clone());
            return found.clone();
        }
        let (weights, head_rotation) = match &self.last {
            Some(last) => {
                let elapsed = now.saturating_duration_since(last.ready).as_secs_f32();
                let ease = ((elapsed - 0.5) / 1.0).clamp(0.0, 1.0);
                let mut weights = last.weights;
                for w in &mut weights {
                    *w *= 1.0 - ease;
                }
                let rotation = last
                    .head_rotation
                    .slerp(&UnitQuaternion::identity(), ease);
                (weights, rotation)
            }
            None => ([0.0; 52], UnitQuaternion::identity()),
        };
        TrackingOutput {
            weights,
            head_rotation,
            face_found: false,
            ready: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{UnitQuaternion, Vector3};
    use std::time::Duration;

    fn tracked(weight: f32, angle: f32, ready: Instant) -> TrackingOutput {
        TrackingOutput {
            weights: [weight; 52],
            head_rotation: UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle),
            face_found: true,
            ready,
        }
    }

    #[test]
    fn face_lost_follows_tracked_output_while_found() {
        let base = Instant::now();
        let face = tracked(0.8, 0.6, base);
        let out = FaceLost::new().update(Some(&face), base);
        assert!(out.face_found);
        assert_eq!(out.weights, face.weights);
        assert_eq!(out.head_rotation, face.head_rotation);
    }

    #[test]
    fn face_lost_holds_last_output_at_0_4s() {
        let base = Instant::now();
        let face = tracked(0.8, 0.6, base);
        let mut lost = FaceLost::new();
        lost.update(Some(&face), base);
        let out = lost.update(None, base + Duration::from_millis(400));
        assert!(!out.face_found);
        assert_eq!(out.weights, face.weights);
        assert!((out.head_rotation.angle() - 0.6).abs() < 1e-4);
    }

    #[test]
    fn face_lost_eases_partway_at_1_0s() {
        let base = Instant::now();
        let face = tracked(0.8, 0.6, base);
        let mut lost = FaceLost::new();
        lost.update(Some(&face), base);
        let out = lost.update(None, base + Duration::from_millis(1000));
        assert!(!out.face_found);
        for w in out.weights {
            assert!(w > 0.0 && w < 0.8, "weight {w} not strictly between 0 and 0.8");
        }
        assert!((out.head_rotation.angle() - 0.3).abs() < 1e-3);
    }

    #[test]
    fn face_lost_is_neutral_at_1_6s() {
        let base = Instant::now();
        let face = tracked(0.8, 0.6, base);
        let mut lost = FaceLost::new();
        lost.update(Some(&face), base);
        let out = lost.update(None, base + Duration::from_millis(1600));
        assert!(!out.face_found);
        assert_eq!(out.weights, [0.0; 52]);
        assert!(out.head_rotation.angle().abs() < 1e-5);
    }

    #[test]
    fn face_lost_returns_to_tracked_output_at_1_0s() {
        let base = Instant::now();
        let old = tracked(0.8, 0.6, base);
        let new = tracked(0.2, -0.4, base);
        let mut lost = FaceLost::new();
        lost.update(Some(&old), base);
        lost.update(None, base);
        let out = lost.update(Some(&new), base + Duration::from_millis(1000));
        assert!(out.face_found);
        assert_eq!(out.weights, new.weights);
        assert_eq!(out.head_rotation, new.head_rotation);
    }

    #[test]
    fn face_lost_without_history_is_neutral() {
        let out = FaceLost::new().update(None, Instant::now());
        assert!(!out.face_found);
        assert_eq!(out.weights, [0.0; 52]);
        assert_eq!(out.head_rotation, UnitQuaternion::identity());
    }
}
