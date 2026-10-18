//! Hand landmarks from InstantHMR's MHR70 joints. Each of the 21 MediaPipe hand points is read from
//! one MHR70 joint of that hand. The x and y are frame pixels, and z is the depth from the wrist in
//! the same pixels, so `HandLandmarks` reads the same way as it does from the 2023 hand model.

use core::body::{HandLandmarks, SubjectSide, HAND_POINTS};
use infer::instant_hmr::{joint, Joints};

/// MHR70 joint for each MediaPipe hand point (index 0 is the wrist), person's right hand.
const RIGHT: [usize; HAND_POINTS] = [41, 24, 23, 22, 21, 28, 27, 26, 25, 32, 31, 30, 29, 36, 35, 34, 33, 40, 39, 38, 37];
/// The same for the person's left hand.
const LEFT: [usize; HAND_POINTS] = [62, 45, 44, 43, 42, 49, 48, 47, 46, 53, 52, 51, 50, 57, 56, 55, 54, 61, 60, 59, 58];

/// Maps the InstantHMR joints of one hand to the 21 hand landmarks. The depth is scaled to pixels with
/// the shoulder pair: pixels between the shoulders over metres between them.
pub fn hand_landmarks(joints: &Joints, side: SubjectSide) -> HandLandmarks {
    let map = match side {
        SubjectSide::Left => &LEFT,
        SubjectSide::Right => &RIGHT,
    };
    let (l, r) = (joint::L_SHOULDER, joint::R_SHOULDER);
    let px = (joints.xy[l][0] - joints.xy[r][0]).hypot(joints.xy[l][1] - joints.xy[r][1]);
    let [dx, dy, dz] = [joints.xyz[l][0] - joints.xyz[r][0], joints.xyz[l][1] - joints.xyz[r][1], joints.xyz[l][2] - joints.xyz[r][2]];
    let m = (dx * dx + dy * dy + dz * dz).sqrt();
    let px_per_m = if m >= 0.05 { px / m } else { 0.0 };
    let wrist_z = joints.xyz[map[0]][2];
    let mut points = [[0.0; 3]; HAND_POINTS];
    for (slot, &j) in map.iter().enumerate() {
        points[slot] = [joints.xy[j][0], joints.xy[j][1], (joints.xyz[j][2] - wrist_z) * px_per_m];
    }
    // The hand model's handedness has no source here, so it is fixed at 1.0.
    HandLandmarks { points, score: 1.0, handedness: 1.0, side }
}

#[cfg(test)]
mod tests {
    use super::*;
    use infer::instant_hmr::JOINTS;

    /// Joints where the 2D x of every joint is its index, so a point's x names the joint it came from.
    fn synthetic() -> Joints {
        let mut j = Joints { xy: [[0.0; 2]; JOINTS], xyz: [[0.0; 3]; JOINTS] };
        for i in 0..JOINTS {
            j.xy[i] = [i as f32, 0.0];
            j.xyz[i] = [i as f32, 0.0, 0.0];
        }
        j
    }

    fn table(side: SubjectSide) -> &'static [usize; HAND_POINTS] {
        match side {
            SubjectSide::Left => &LEFT,
            SubjectSide::Right => &RIGHT,
        }
    }

    #[test]
    fn every_table_entry_lands_in_its_slot_for_both_hands() {
        for side in [SubjectSide::Left, SubjectSide::Right] {
            let hand = hand_landmarks(&synthetic(), side);
            for (slot, p) in hand.points.iter().enumerate() {
                assert_eq!(p[0], table(side)[slot] as f32, "{side:?} slot {slot}");
                assert_eq!(p[1], 0.0);
            }
            assert_eq!(hand.side, side);
            assert_eq!(hand.score, 1.0);
        }
    }

    #[test]
    fn left_and_right_hands_differ() {
        let left = hand_landmarks(&synthetic(), SubjectSide::Left);
        let right = hand_landmarks(&synthetic(), SubjectSide::Right);
        assert_ne!(left.points, right.points);
        assert_eq!(left.points[1][0], 45.0);
        assert_eq!(right.points[1][0], 24.0);
    }

    #[test]
    fn wrist_is_slot_zero() {
        assert_eq!(hand_landmarks(&synthetic(), SubjectSide::Right).points[0][0], 41.0);
        assert_eq!(hand_landmarks(&synthetic(), SubjectSide::Left).points[0][0], 62.0);
    }

    #[test]
    fn depth_is_relative_to_the_wrist_in_pixels() {
        // Shoulders 40 px apart and 0.4 m apart: 100 px per metre.
        let mut j = synthetic();
        j.xy[joint::L_SHOULDER] = [0.0, 0.0];
        j.xy[joint::R_SHOULDER] = [40.0, 0.0];
        j.xyz[joint::L_SHOULDER] = [0.0, 0.0, 0.0];
        j.xyz[joint::R_SHOULDER] = [0.4, 0.0, 0.0];
        j.xyz[41] = [0.0, 0.0, 1.0];
        j.xyz[24] = [0.0, 0.0, 1.1];
        let hand = hand_landmarks(&j, SubjectSide::Right);
        assert_eq!(hand.points[0][2], 0.0);
        assert!((hand.points[1][2] - 10.0).abs() < 1e-3);
    }
}
