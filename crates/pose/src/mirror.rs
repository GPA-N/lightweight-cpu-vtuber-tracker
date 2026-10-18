use core::names::mirror_partner;
use nalgebra::{Quaternion, UnitQuaternion};

/// Mirror setting: when on, swap each `*Left`/`*Right` weight pair and mirror the
/// head rotation as `M R M` with `M = diag(-1, 1, 1)` (negates yaw and roll).
/// When off, both inputs are returned unchanged.
pub fn apply_mirror(
    mirror: bool,
    weights: &[f32; 52],
    rotation: UnitQuaternion<f32>,
) -> ([f32; 52], UnitQuaternion<f32>) {
    if !mirror {
        return (*weights, rotation);
    }
    let swapped = std::array::from_fn(|i| weights[mirror_partner(i)]);
    // M R M on a quaternion is negating the y and z components of the vector part.
    let mirrored = UnitQuaternion::from_quaternion(Quaternion::new(
        rotation.w, rotation.i, -rotation.j, -rotation.k,
    ));
    (swapped, mirrored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::names::{index_of, BLENDSHAPE_NAMES};
    use nalgebra::{Matrix3, Vector3};

    fn sample_weights() -> [f32; 52] {
        let mut w = [0.0f32; 52];
        for (i, v) in w.iter_mut().enumerate() {
            *v = (i as f32) / 100.0 + 0.001;
        }
        w
    }

    fn deg(d: f32) -> f32 {
        d.to_radians()
    }

    fn euler(yaw: f32, pitch: f32, roll: f32) -> UnitQuaternion<f32> {
        UnitQuaternion::from_axis_angle(&Vector3::y_axis(), deg(yaw))
            * UnitQuaternion::from_axis_angle(&Vector3::x_axis(), deg(pitch))
            * UnitQuaternion::from_axis_angle(&Vector3::z_axis(), deg(roll))
    }

    fn assert_rot_close(a: UnitQuaternion<f32>, b: UnitQuaternion<f32>) {
        let (ma, mb): (Matrix3<f32>, Matrix3<f32>) = (a.to_rotation_matrix().into(), b.to_rotation_matrix().into());
        assert!((ma - mb).amax() < 1e-5, "rotations differ:\n{ma}\n{mb}");
    }

    /// Yaw, pitch, roll in degrees of `R = R_y(yaw) R_x(pitch) R_z(roll)`.
    fn decompose(q: UnitQuaternion<f32>) -> (f32, f32, f32) {
        let r = q.to_rotation_matrix();
        let r = r.matrix();
        let yaw = r[(0, 2)].atan2(r[(2, 2)]);
        let pitch = (-r[(1, 2)]).asin();
        let roll = r[(1, 0)].atan2(r[(1, 1)]);
        (yaw.to_degrees(), pitch.to_degrees(), roll.to_degrees())
    }

    #[test]
    fn eye_blink_pair_is_swapped_when_on() {
        let mut w = [0.0f32; 52];
        w[index_of("eyeBlinkLeft").unwrap()] = 1.0;
        w[index_of("eyeBlinkRight").unwrap()] = 0.0;
        let (out, _) = apply_mirror(true, &w, UnitQuaternion::identity());
        assert_eq!(out[index_of("eyeBlinkRight").unwrap()], 1.0);
        assert_eq!(out[index_of("eyeBlinkLeft").unwrap()], 0.0);
    }

    #[test]
    fn every_pair_is_swapped_when_on() {
        let w = sample_weights();
        let (out, _) = apply_mirror(true, &w, UnitQuaternion::identity());
        for i in 0..52 {
            assert_eq!(out[i], w[mirror_partner(i)], "index {i} ({})", BLENDSHAPE_NAMES[i]);
        }
    }

    #[test]
    fn nothing_changes_when_off() {
        let w = sample_weights();
        let r = euler(20.0, 15.0, 10.0);
        let (out, rot) = apply_mirror(false, &w, r);
        assert_eq!(out, w);
        assert_eq!(rot, r);
    }

    #[test]
    fn yaw_flips_pitch_kept_roll_flips() {
        let r = euler(20.0, 15.0, 10.0);
        let (_, out) = apply_mirror(true, &[0.0; 52], r);
        let (yaw, pitch, roll) = decompose(out);
        assert!((yaw + 20.0).abs() < 1e-3, "yaw {yaw}");
        assert!((pitch - 15.0).abs() < 1e-3, "pitch {pitch}");
        assert!((roll + 10.0).abs() < 1e-3, "roll {roll}");
    }

    #[test]
    fn mirror_matches_matrix_form_m_r_m() {
        let r = euler(33.0, -12.0, 7.0);
        let m = Matrix3::new(-1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
        let rm: Matrix3<f32> = r.to_rotation_matrix().into();
        let expected = UnitQuaternion::from_matrix(&(m * rm * m));
        let (_, out) = apply_mirror(true, &[0.0; 52], r);
        assert_rot_close(out, expected);
        assert_rot_close(out, euler(-33.0, -12.0, -7.0));
    }

    #[test]
    fn mirror_twice_returns_original() {
        let w = sample_weights();
        let r = euler(20.0, 15.0, 10.0);
        let (w1, r1) = apply_mirror(true, &w, r);
        let (w2, r2) = apply_mirror(true, &w1, r1);
        assert_eq!(w2, w);
        assert_rot_close(r2, r);
    }
}
