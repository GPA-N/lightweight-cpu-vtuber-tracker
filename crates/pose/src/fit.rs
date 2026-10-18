use nalgebra::{Matrix3, Rotation3, UnitQuaternion, Vector3};

const CANONICAL_OBJ: &str = include_str!("../../../data/canonical_face_model.obj");
const N_LANDMARKS: usize = 478;
// Weighted RMS fit error divided by the eye distance. MediaPipe has no residual gate, so this only
// has to reject garbage. Measured on 5 real photos (docs/notes/accuracy-baseline.md): 0.040 to 0.069
// with the 33-point basis, so 0.12 leaves room for harder faces.
const RESIDUAL_LIMIT: f64 = 0.12;
// (index, pbtxt weight) for the 11-point basis from docs/notes/canonical-face.md.
const SUBSET: [(usize, f64); 11] = [
    (129, 0.120625),
    (358, 0.120625),
    (4, 0.070910),
    (136, 0.066890),
    (365, 0.066890),
    (33, 0.058724),
    (263, 0.058724),
    (133, 0.053286),
    (362, 0.053286),
    (205, 0.047252),
    (425, 0.047252),
];

// The full 33-point procrustes basis from MediaPipe (geometry_pipeline_metadata_landmarks.pbtxt).
const BASIS_33: [(usize, f64); 33] = [
    (4, 0.070909939706326), (6, 0.032100144773722), (10, 0.008446550928056), (33, 0.058724168688059),
    (54, 0.007667080033571), (67, 0.009078059345484), (117, 0.009791937656701), (119, 0.014565368182957),
    (121, 0.018591361120343), (127, 0.005197994410992), (129, 0.120625205338001), (132, 0.005560018587857),
    (133, 0.05328618362546), (136, 0.066890455782413), (143, 0.014816547743976), (147, 0.014262833632529),
    (198, 0.025462191551924), (205, 0.047252278774977), (263, 0.058724168688059), (284, 0.007667080033571),
    (297, 0.009078059345484), (346, 0.009791937656701), (348, 0.014565368182957), (350, 0.018591361120343),
    (356, 0.005197994410992), (358, 0.120625205338001), (361, 0.005560018587857), (362, 0.05328618362546),
    (365, 0.066890455782413), (372, 0.014816547743976), (376, 0.014262833632529), (420, 0.025462191551924),
    (425, 0.047252278774977),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadFit {
    pub rotation: UnitQuaternion<f32>,
    pub scale: f32,
    pub residual: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FitError {
    TooFewPoints,
    ZSignAssumptionFailed,
    FitResidualHigh,
}

fn canonical_points() -> Vec<[f64; 3]> {
    CANONICAL_OBJ
        .lines()
        .filter_map(|line| {
            let mut tok = line.split_whitespace();
            if tok.next()? != "v" {
                return None;
            }
            let v: Vec<f64> = tok.map(|t| t.parse().unwrap()).collect();
            Some([v[0], v[1], v[2]])
        })
        .collect()
}

pub fn fit_head_rotation(landmarks: &[[f32; 3]]) -> Result<HeadFit, FitError> {
    fit_with(landmarks, true, true)
}

fn fit_with(landmarks: &[[f32; 3]], flip: bool, check_sign: bool) -> Result<HeadFit, FitError> {
    if landmarks.len() < N_LANDMARKS {
        return Err(FitError::TooFewPoints);
    }
    // Step 1: frame change p = F q with F = diag(1, -1, -1) (identity when flip is off).
    let sign_yz = if flip { -1.0 } else { 1.0 };
    let to_frame = |p: &[f32; 3]| Vector3::new(p[0] as f64, sign_yz * p[1] as f64, sign_yz * p[2] as f64);

    // Step 2: assumption A2, nose tip z must be greater than the mean eye-corner z.
    if check_sign {
        let z_nose = to_frame(&landmarks[4]).z;
        let z_eyes = (to_frame(&landmarks[33]).z + to_frame(&landmarks[263]).z) / 2.0;
        if z_nose <= z_eyes {
            return Err(FitError::ZSignAssumptionFailed);
        }
    }

    let sol = solve(landmarks, &BASIS_33, &to_frame);
    let residual = sol.residual_eye_xyz;
    if residual.is_nan() || residual > RESIDUAL_LIMIT {
        return Err(FitError::FitResidualHigh);
    }
    let (r, s) = (sol.r, sol.s);

    let rotation = UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(r.cast::<f32>()));
    Ok(HeadFit {
        rotation,
        scale: s as f32,
        residual: residual as f32,
    })
}

/// Result of one weighted similarity fit.
struct Solved {
    r: Matrix3<f64>,
    s: f64,
    /// Weighted RMS error over the weighted RMS radius of the destination points (the shipped residual).
    residual_rms_over_size: f64,
    /// Weighted RMS error over the eye distance, all three axes.
    residual_eye_xyz: f64,
    /// Weighted RMS error over the eye distance, x and y only.
    residual_eye_xy: f64,
}

fn solve(
    landmarks: &[[f32; 3]],
    basis: &[(usize, f64)],
    to_frame: &dyn Fn(&[f32; 3]) -> Vector3<f64>,
) -> Solved {
    // Step 3: weighted centroids and centered points.
    let canon = canonical_points();
    let pts: Vec<(f64, Vector3<f64>, Vector3<f64>)> = basis
        .iter()
        .map(|&(i, w)| (w, Vector3::from(canon[i]), to_frame(&landmarks[i])))
        .collect();
    let w_sum: f64 = pts.iter().map(|p| p.0).sum();
    let c_src = pts.iter().fold(Vector3::zeros(), |acc, p| acc + p.1 * p.0) / w_sum;
    let c_dst = pts.iter().fold(Vector3::zeros(), |acc, p| acc + p.2 * p.0) / w_sum;
    let centered: Vec<(f64, Vector3<f64>, Vector3<f64>)> =
        pts.iter().map(|p| (p.0, p.1 - c_src, p.2 - c_dst)).collect();

    // Step 4: H = sum w src dst^T, SVD, determinant correction, rotation and scale.
    let h = centered
        .iter()
        .fold(Matrix3::zeros(), |acc, (w, src, dst)| acc + (src * dst.transpose()) * *w);
    let svd = h.svd(true, true);
    let u = svd.u.expect("svd u requested");
    let v = svd.v_t.expect("svd v_t requested").transpose();
    let d = (v * u.transpose()).determinant().signum();
    let r = v * Matrix3::from_diagonal(&Vector3::new(1.0, 1.0, d)) * u.transpose();
    let sv = svd.singular_values;
    let src_energy: f64 = centered.iter().map(|(w, src, _)| w * src.norm_squared()).sum();
    let s = (sv[0] + sv[1] + d * sv[2]) / src_energy;

    // Fit-residual measures.
    let (mut err2, mut err2_xy, mut size2) = (0.0, 0.0, 0.0);
    for (w, src, dst) in &centered {
        let err = dst - r * src * s;
        err2 += w * err.norm_squared();
        err2_xy += w * (err.x * err.x + err.y * err.y);
        size2 += w * dst.norm_squared();
    }
    let eye = (to_frame(&landmarks[33]) - to_frame(&landmarks[263])).norm();
    Solved {
        r,
        s,
        residual_rms_over_size: (err2 / w_sum).sqrt() / (size2 / w_sum).sqrt(),
        residual_eye_xyz: (err2 / w_sum).sqrt() / eye,
        residual_eye_xy: (err2_xy / w_sum).sqrt() / eye,
    }
}

/// Residuals of several fit variants for one set of landmarks, for choosing the accept limit.
/// Each entry is (variant name, residual). Variants differ in basis size and normalisation.
pub fn residual_variants(landmarks: &[[f32; 3]]) -> Vec<(&'static str, f64)> {
    if landmarks.len() < N_LANDMARKS {
        return Vec::new();
    }
    let to_frame = |p: &[f32; 3]| Vector3::new(p[0] as f64, -(p[1] as f64), -(p[2] as f64));
    let a = solve(landmarks, &SUBSET, &to_frame);
    let b = solve(landmarks, &BASIS_33, &to_frame);
    vec![
        ("11pt shipped (rms radius)", a.residual_rms_over_size),
        ("11pt eye-normalised xyz", a.residual_eye_xyz),
        ("11pt eye-normalised xy", a.residual_eye_xy),
        ("33pt rms radius", b.residual_rms_over_size),
        ("33pt eye-normalised xyz", b.residual_eye_xyz),
        ("33pt eye-normalised xy", b.residual_eye_xy),
    ]
}

pub fn yaw_pitch_roll(q: &UnitQuaternion<f32>) -> (f32, f32, f32) {
    // R = R_y(yaw) R_x(pitch) R_z(roll).
    let m = q.to_rotation_matrix();
    let m = m.matrix();
    let yaw = m[(0, 2)].atan2(m[(2, 2)]);
    let pitch = (-m[(1, 2)]).clamp(-1.0, 1.0).asin();
    let roll = m[(1, 0)].atan2(m[(1, 1)]);
    (yaw.to_degrees(), pitch.to_degrees(), roll.to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rot_x(deg: f64) -> Matrix3<f64> {
        let (s, c) = deg.to_radians().sin_cos();
        Matrix3::new(1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c)
    }

    fn rot_y(deg: f64) -> Matrix3<f64> {
        let (s, c) = deg.to_radians().sin_cos();
        Matrix3::new(c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c)
    }

    fn rot_z(deg: f64) -> Matrix3<f64> {
        let (s, c) = deg.to_radians().sin_cos();
        Matrix3::new(c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0)
    }

    // Image-convention input: Q = 0.5 * r * P, x = Qx+128, y = -Qy+128, z = -Qz.
    // The 468 canonical points fill slots 0..468; slots 468..478 (iris) are zero
    // because the fit only reads the 11 SUBSET indices (all < 468).
    fn synth(r: &Matrix3<f64>) -> Vec<[f32; 3]> {
        let mut out = vec![[0.0f32; 3]; N_LANDMARKS];
        for (i, p) in canonical_points().iter().enumerate() {
            let q = r * Vector3::from(*p) * 0.5;
            out[i] = [(q.x + 128.0) as f32, (-q.y + 128.0) as f32, (-q.z) as f32];
        }
        out
    }

    fn assert_axis(fit: &HeadFit, want: (f32, f32, f32)) {
        let (y, p, r) = yaw_pitch_roll(&fit.rotation);
        assert!((y - want.0).abs() < 1.0, "yaw {y} want {}", want.0);
        assert!((p - want.1).abs() < 1.0, "pitch {p} want {}", want.1);
        assert!((r - want.2).abs() < 1.0, "roll {r} want {}", want.2);
        assert!((fit.scale - 0.5).abs() / 0.5 < 1e-3, "scale {}", fit.scale);
    }

    #[test]
    fn fit_recovers_each_axis_20_deg() {
        let cases = [
            (rot_y(20.0), (20.0, 0.0, 0.0)),
            (rot_x(20.0), (0.0, 20.0, 0.0)),
            (rot_z(20.0), (0.0, 0.0, 20.0)),
        ];
        for (r, want) in cases {
            let fit = fit_head_rotation(&synth(&r)).expect("fit");
            assert_axis(&fit, want);
        }
    }

    #[test]
    fn fit_without_flip_is_wrong() {
        let lm = synth(&rot_y(20.0));
        match fit_with(&lm, false, false) {
            Ok(f) => {
                let (y, p, r) = yaw_pitch_roll(&f.rotation);
                assert!(!((y - 20.0).abs() < 1.0 && p.abs() < 1.0 && r.abs() < 1.0), "got {y} {p} {r}");
            }
            Err(e) => assert_eq!(e, FitError::FitResidualHigh),
        }
    }

    #[test]
    fn translated_and_scaled_copy_same_rotation() {
        let lm = synth(&rot_y(20.0));
        let moved: Vec<[f32; 3]> = lm
            .iter()
            .map(|p| [2.0 * p[0] + 30.0, 2.0 * p[1] - 10.0, 2.0 * p[2] + 5.0])
            .collect();
        let a = fit_head_rotation(&lm).expect("fit");
        let b = fit_head_rotation(&moved).expect("fit moved");
        assert!(a.rotation.angle_to(&b.rotation) < 1e-3);
    }

    #[test]
    fn x_mirrored_input_is_err() {
        let mirrored: Vec<[f32; 3]> = synth(&rot_y(20.0)).iter().map(|p| [-p[0], p[1], p[2]]).collect();
        assert!(matches!(
            fit_head_rotation(&mirrored),
            Err(FitError::ZSignAssumptionFailed) | Err(FitError::FitResidualHigh)
        ));
    }

    #[test]
    fn too_few_points_is_err() {
        assert_eq!(fit_head_rotation(&[[0.0; 3]; 477]), Err(FitError::TooFewPoints));
    }

    #[test]
    fn compressed_z_still_fits() {
        // Real photo test7 had a z-to-x ratio of 1.16 against the canonical 2.01.
        let squashed: Vec<[f32; 3]> = synth(&rot_y(20.0)).iter().map(|p| [p[0], p[1], p[2] * 0.58]).collect();
        let fit = fit_head_rotation(&squashed).expect("compressed z must still fit");
        let (yaw, _, _) = yaw_pitch_roll(&fit.rotation);
        assert!((yaw - 20.0).abs() < 10.0, "yaw {yaw}");
    }

    #[test]
    fn random_landmarks_are_rejected() {
        // Deterministic pseudo-random points in a 256 box, with a valid z sign so only the residual can reject.
        let mut state = 12345u32;
        let mut next = || {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state >> 8) as f32 / (1u32 << 24) as f32 * 256.0
        };
        let mut lm: Vec<[f32; 3]> = (0..N_LANDMARKS).map(|_| [next(), next(), next() - 128.0]).collect();
        lm[4][2] = -100.0;
        lm[33][2] = 50.0;
        lm[263][2] = 50.0;
        assert_eq!(fit_head_rotation(&lm), Err(FitError::FitResidualHigh));
    }
}
