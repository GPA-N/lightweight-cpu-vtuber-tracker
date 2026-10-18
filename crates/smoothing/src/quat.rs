use crate::one_euro::OneEuro;
use nalgebra::{Quaternion, UnitQuaternion, Vector4};

// Tuned against the two head-rotation spec scenarios; weights keep beta 0.007.
pub const QUAT_MIN_CUTOFF: f32 = 1.0;
pub const QUAT_BETA: f32 = 0.3;

/// One Euro filter on a unit quaternion: one scalar filter per component,
/// with a sign flip toward the previous output so q and -q filter the same.
pub struct QuatFilter {
    filters: [OneEuro; 4],
    prev: Option<Vector4<f32>>,
}

impl Default for QuatFilter {
    fn default() -> Self {
        Self::new(QUAT_MIN_CUTOFF, QUAT_BETA)
    }
}

impl QuatFilter {
    pub fn new(min_cutoff: f32, beta: f32) -> Self {
        Self {
            filters: std::array::from_fn(|_| OneEuro::new(min_cutoff, beta)),
            prev: None,
        }
    }

    pub fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
        self.prev = None;
    }

    /// Filter one sample taken `dt` seconds after the previous one.
    pub fn filter(&mut self, q: UnitQuaternion<f32>, dt: f32) -> UnitQuaternion<f32> {
        // Stored order is (i, j, k, w).
        let mut c = q.into_inner().coords;
        if self.prev.is_some_and(|p| c.dot(&p) < 0.0) {
            c = -c;
        }
        let mut out = Vector4::zeros();
        for (i, f) in self.filters.iter_mut().enumerate() {
            out[i] = f.filter(c[i], dt);
        }
        let normed = UnitQuaternion::new_normalize(Quaternion::new(out[3], out[0], out[1], out[2]));
        let coords = normed.into_inner().coords;
        self.prev = Some(coords);
        normed
    }
}

#[cfg(test)]
mod tests {
    use super::{QuatFilter, QUAT_BETA, QUAT_MIN_CUTOFF};
    use nalgebra::{Unit, UnitQuaternion, Vector3};

    const SEEDS: [u64; 5] = [7, 11, 23, 42, 99];

    const DT: f32 = 1.0 / 30.0;

    struct Lcg(u64);

    impl Lcg {
        // Uniform in [-1, 1).
        fn next(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        }
    }

    fn yaw(deg: f32) -> UnitQuaternion<f32> {
        UnitQuaternion::from_axis_angle(&Vector3::y_axis(), deg.to_radians())
    }

    fn deg_between(a: &UnitQuaternion<f32>, b: &UnitQuaternion<f32>) -> f32 {
        a.angle_to(b).to_degrees()
    }

    // Worst per-frame change of the output for a still face with 0.2 deg noise.
    fn still_worst_step(beta: f32, seed: u64) -> f32 {
        let mut f = QuatFilter::new(QUAT_MIN_CUTOFF, beta);
        let mut rng = Lcg(seed);
        let axis = Unit::new_normalize(Vector3::new(1.0, 2.0, -0.5));
        let base = yaw(30.0);
        let noisy = |rng: &mut Lcg| {
            let n = UnitQuaternion::from_axis_angle(&axis, (0.2 * rng.next()).to_radians());
            base * n
        };
        let mut prev = f.filter(noisy(&mut rng), DT);
        let mut worst = 0.0f32;
        for _ in 0..300 {
            let y = f.filter(noisy(&mut rng), DT);
            worst = worst.max(deg_between(&prev, &y));
            prev = y;
        }
        worst
    }

    #[test]
    fn quat_still_rotation_with_noise_changes_slowly() {
        for seed in SEEDS {
            let step = still_worst_step(QUAT_BETA, seed);
            assert!(step < 0.5, "seed {seed}: step {step} deg");
        }
    }

    #[test]
    fn quat_yaw_turn_settles_within_10_deg_three_frames_after_end() {
        let mut f = QuatFilter::new(QUAT_MIN_CUTOFF, QUAT_BETA);
        f.filter(yaw(0.0), DT);
        // 45 deg over 9 frames (0.3 s at 30 Hz), then held at 45 deg.
        let mut out = yaw(0.0);
        for frame in 1..=12 {
            let angle = 45.0 * (frame.min(9) as f32) / 9.0;
            out = f.filter(yaw(angle), DT);
        }
        let err = deg_between(&out, &yaw(45.0));
        assert!(err <= 10.0, "error {err} deg at 3 frames after turn end");
    }

    #[test]
    fn quat_negated_input_gives_same_rotation() {
        let mut a = QuatFilter::new(1.0, 0.007);
        let mut b = QuatFilter::new(1.0, 0.007);
        for frame in 0..=12 {
            let q = yaw(45.0 * (frame.min(9) as f32) / 9.0);
            let neg = UnitQuaternion::new_normalize(-q.into_inner());
            // Alternate the sign on every other frame for the second filter.
            let input_b = if frame % 2 == 1 { neg } else { q };
            let ya = a.filter(q, DT);
            let yb = b.filter(input_b, DT);
            let diff = deg_between(&ya, &yb);
            assert!(diff < 1e-3, "frame {frame}: outputs differ by {diff} deg");
        }
    }
}
