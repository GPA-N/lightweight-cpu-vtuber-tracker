pub mod one_euro;
pub mod quat;
pub mod weights;
pub mod face_lost;
pub mod body_lost;

/// Eased share (0 to 1) of the way back to neutral, `elapsed` seconds after the loss:
/// holds for 0.5 s, then ramps over 1.0 s.
fn ease_after_hold(elapsed: f32) -> f32 {
    (elapsed - 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
pub(crate) struct Lcg(pub u64);

#[cfg(test)]
impl Lcg {
    // Uniform in [-1, 1).
    pub(crate) fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}
