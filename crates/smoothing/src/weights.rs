use crate::one_euro::OneEuro;

/// One One Euro filter per blendshape weight; inputs and outputs clamped to 0.0..=1.0.
pub struct WeightsFilter {
    filters: [OneEuro; 52],
}

impl WeightsFilter {
    pub fn new(min_cutoff: f32, beta: f32) -> Self {
        Self { filters: std::array::from_fn(|_| OneEuro::new(min_cutoff, beta)) }
    }

    pub fn filter(&mut self, w: &[f32; 52], dt: f32) -> [f32; 52] {
        std::array::from_fn(|i| self.filters[i].filter(w[i].clamp(0.0, 1.0), dt).clamp(0.0, 1.0))
    }

    pub fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WeightsFilter;

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

    #[test]
    fn weights_output_stays_in_unit_range() {
        let mut f = WeightsFilter::new(1.0, 0.007);
        for frame in 0..200 {
            let mut w = [0.0f32; 52];
            for (i, v) in w.iter_mut().enumerate() {
                *v = if (frame + i) % 2 == 0 { -3.0 } else { 7.0 };
            }
            let out = f.filter(&w, DT);
            for (i, v) in out.iter().enumerate() {
                assert!((0.0..=1.0).contains(v), "frame {frame} weight {i}: {v}");
            }
        }
    }

    #[test]
    fn weights_still_noisy_input_changes_slowly() {
        let mut f = WeightsFilter::new(1.0, 0.007);
        let mut rng = Lcg(42);
        let noisy = |rng: &mut Lcg| {
            let mut w = [0.0f32; 52];
            for v in w.iter_mut() {
                *v = 0.5 + 0.01 * rng.next();
            }
            w
        };
        let mut prev = f.filter(&noisy(&mut rng), DT);
        for frame in 0..300 {
            let out = f.filter(&noisy(&mut rng), DT);
            for i in 0..52 {
                let jump = (out[i] - prev[i]).abs();
                assert!(jump < 0.02, "frame {frame} weight {i}: jump {jump}");
            }
            prev = out;
        }
    }
}
