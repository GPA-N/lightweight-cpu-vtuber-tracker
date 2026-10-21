use std::f32::consts::PI;

/// Scalar One Euro filter (Casiez et al. 2012).
pub struct OneEuro {
    pub min_cutoff: f32,
    pub beta: f32,
    pub d_cutoff: f32,
    prev: Option<(f32, f32)>, // (last raw x, last filtered dx)
    last: f32,
}

impl OneEuro {
    /// `d_cutoff` defaults to 1.0; change the pub field to override.
    pub fn new(min_cutoff: f32, beta: f32) -> Self {
        Self { min_cutoff, beta, d_cutoff: 1.0, prev: None, last: 0.0 }
    }

    pub fn reset(&mut self) {
        self.prev = None;
    }

    /// Filter one sample taken `dt` seconds after the previous one.
    pub fn filter(&mut self, x: f32, dt: f32) -> f32 {
        let (x_prev, dx_prev) = match self.prev {
            None => {
                self.prev = Some((x, 0.0));
                self.last = x;
                return x;
            }
            Some(p) => p,
        };
        let dx = (x - x_prev) / dt;
        let a_d = alpha(self.d_cutoff, dt);
        let dx_hat = a_d * dx + (1.0 - a_d) * dx_prev;
        let cutoff = self.min_cutoff + self.beta * dx_hat.abs();
        let a = alpha(cutoff, dt);
        let x_hat = a * x + (1.0 - a) * self.last;
        self.prev = Some((x, dx_hat));
        self.last = x_hat;
        x_hat
    }
}

fn alpha(cutoff: f32, dt: f32) -> f32 {
    let tau = 1.0 / (2.0 * PI * cutoff);
    1.0 / (1.0 + tau / dt)
}

#[cfg(test)]
mod tests {
    use super::OneEuro;
    use crate::Lcg;

    const DT: f32 = 1.0 / 30.0;

    #[test]
    fn one_euro_constant_input_stays_put() {
        let mut f = OneEuro::new(1.0, 0.0);
        for _ in 0..100 {
            let y = f.filter(0.5, DT);
            assert!((y - 0.5).abs() <= 1e-6, "output {y}");
        }
    }

    #[test]
    fn one_euro_first_call_returns_input() {
        let mut f = OneEuro::new(1.0, 0.0);
        assert_eq!(f.filter(0.3, DT), 0.3);
    }

    #[test]
    fn one_euro_step_moves_monotonically_and_settles() {
        let mut f = OneEuro::new(1.0, 0.0);
        f.filter(0.0, DT);
        let mut prev = 0.0;
        let mut y = 0.0;
        for i in 0..30 {
            y = f.filter(1.0, DT);
            assert!(y >= prev, "not monotonic at sample {i}: {y} < {prev}");
            assert!(y <= 1.0, "overshoot at sample {i}: {y}");
            prev = y;
        }
        assert!((1.0f32 - y).abs() < 0.05, "after 30 samples: {y}");
    }

    #[test]
    fn one_euro_noise_changes_slowly() {
        let mut f = OneEuro::new(1.0, 0.0);
        let mut rng = Lcg(42);
        let mut prev = f.filter(0.5, DT);
        for _ in 0..300 {
            let y = f.filter(0.5 + 0.01 * rng.next(), DT);
            assert!((y - prev).abs() < 0.02, "jump {} at output {y}", (y - prev).abs());
            prev = y;
        }
    }
}
