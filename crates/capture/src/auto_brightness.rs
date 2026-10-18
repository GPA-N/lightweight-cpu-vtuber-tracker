//! Automatic brightness for any camera. It works on the RGB frames, not through the camera driver, so it
//! behaves the same on every webcam and every operating system, whether or not the driver offers an
//! exposure control (many laptop cameras do not, or do it badly in a dark room or against a window).
//!
//! Each frame is metered on a sparse grid (about 16,000 samples, whatever the resolution), centre-weighted
//! because the person sits in the middle. The mean brightness picks a gamma that would move it to the
//! target. The gamma follows that wish with a time constant of about a second and is applied through a
//! 256-entry table, so lights switching on and off give a smooth fade, not flicker. Gamma lifts shadows
//! without clipping highlights, which is what the face models need. A frame that is almost black (a covered
//! lens) is left alone, and a gamma close to 1 is not applied at all.

use std::time::Instant;

/// Mean brightness (0 to 1) that the correction aims for.
const TARGET: f32 = 0.45;
/// Seconds the gamma takes to cover about two thirds of the way to a new wish.
const SMOOTH_S: f32 = 0.8;
/// A gamma this close to 1 is not worth a pass over the pixels.
const DEADBAND: f32 = 0.04;
/// Strongest brightening (below 1) and darkening (above 1).
const GAMMA_MIN: f32 = 0.4;
const GAMMA_MAX: f32 = 1.8;
/// Below this mean brightness the frame counts as a covered lens and is not amplified.
const BLACK: f32 = 0.02;
/// Samples taken per frame, about.
const SAMPLES: usize = 16_384;

#[derive(Debug)]
pub struct AutoBrightness {
    gamma: f32,
    last: Option<Instant>,
}

impl Default for AutoBrightness {
    fn default() -> Self {
        Self { gamma: 1.0, last: None }
    }
}

impl AutoBrightness {
    /// The gamma now in use (1 = frames pass unchanged).
    pub fn gamma(&self) -> f32 {
        self.gamma
    }

    /// Meters `rgb` (`width * height * 3` bytes) and corrects it in place.
    pub fn process(&mut self, rgb: &mut [u8], width: u32, height: u32) {
        self.process_at(rgb, width, height, Instant::now());
    }

    pub fn process_at(&mut self, rgb: &mut [u8], width: u32, height: u32, now: Instant) {
        let (w, h) = (width as usize, height as usize);
        if w == 0 || h == 0 || rgb.len() != w * h * 3 {
            return;
        }
        let wish = wish_gamma(mean_brightness(rgb, w, h));
        let dt = self.last.map(|t| (now - t).as_secs_f32()).unwrap_or(f32::INFINITY);
        self.last = Some(now);
        // The first frame jumps to the wish; later ones follow it smoothly.
        let alpha = if dt.is_finite() { 1.0 - (-dt.min(1.0) / SMOOTH_S).exp() } else { 1.0 };
        self.gamma += (wish - self.gamma) * alpha;
        if (self.gamma - 1.0).abs() < DEADBAND {
            return;
        }
        let lut = table(self.gamma);
        for b in rgb.iter_mut() {
            *b = lut[*b as usize];
        }
    }
}

/// Gamma that would bring `mean` to the target, or 1 when the frame is black.
fn wish_gamma(mean: f32) -> f32 {
    if mean < BLACK {
        return 1.0;
    }
    // mean^gamma = TARGET; both logarithms are negative, so gamma is positive.
    (TARGET.ln() / mean.min(0.98).ln()).clamp(GAMMA_MIN, GAMMA_MAX)
}

fn table(gamma: f32) -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        *v = ((i as f32 / 255.0).powf(gamma) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }
    lut
}

/// Centre-weighted mean brightness (0 to 1) on a sparse grid. The middle 60 percent of the frame counts double.
fn mean_brightness(rgb: &[u8], w: usize, h: usize) -> f32 {
    let step = ((w * h / SAMPLES) as f32).sqrt().ceil().max(1.0) as usize;
    let (x0, x1, y0, y1) = (w / 5, w * 4 / 5, h / 5, h * 4 / 5);
    let (mut sum, mut weight) = (0.0f64, 0.0f64);
    let mut y = step / 2;
    while y < h {
        let mut x = step / 2;
        while x < w {
            let i = (y * w + x) * 3;
            let luma = (299.0 * rgb[i] as f32 + 587.0 * rgb[i + 1] as f32 + 114.0 * rgb[i + 2] as f32) / 1000.0 / 255.0;
            let k = if x >= x0 && x < x1 && y >= y0 && y < y1 { 2.0 } else { 1.0 };
            sum += luma as f64 * k;
            weight += k;
            x += step;
        }
        y += step;
    }
    if weight == 0.0 {
        0.0
    } else {
        (sum / weight) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn flat(level: u8, w: usize, h: usize) -> Vec<u8> {
        vec![level; w * h * 3]
    }

    fn mean(rgb: &[u8], w: usize, h: usize) -> f32 {
        mean_brightness(rgb, w, h)
    }

    /// Feeds the same scene for `seconds` at 10 fps and returns the last corrected frame.
    fn settle(auto: &mut AutoBrightness, scene: &[u8], w: usize, h: usize, seconds: f32) -> Vec<u8> {
        let t0 = Instant::now();
        let mut last = scene.to_vec();
        for i in 0..(seconds * 10.0) as u32 {
            last = scene.to_vec();
            auto.process_at(&mut last, w as u32, h as u32, t0 + Duration::from_millis(100 * i as u64));
        }
        last
    }

    #[test]
    fn a_dark_scene_is_lifted_towards_the_target() {
        let (w, h) = (64, 48);
        let mut auto = AutoBrightness::default();
        let out = settle(&mut auto, &flat(45, w, h), w, h, 6.0);
        let m = mean(&out, w, h);
        assert!((m - TARGET).abs() < 0.05, "mean after settling {m}");
        assert!(auto.gamma() < 1.0);
        // Far too dark for the gamma limit to reach the target: lifted as far as the limit allows.
        let mut auto = AutoBrightness::default();
        let out = settle(&mut auto, &flat(10, w, h), w, h, 6.0);
        assert!(mean(&out, w, h) > 3.0 * mean(&flat(10, w, h), w, h) && (auto.gamma() - GAMMA_MIN).abs() < 0.02);
    }

    #[test]
    fn a_bright_scene_is_pulled_down() {
        let (w, h) = (64, 48);
        let mut auto = AutoBrightness::default();
        let out = settle(&mut auto, &flat(240, w, h), w, h, 6.0);
        let m = mean(&out, w, h);
        assert!(m < mean(&flat(240, w, h), w, h) - 0.03 && auto.gamma() > 1.5, "mean {m}, gamma {}", auto.gamma());
    }

    #[test]
    fn a_well_exposed_scene_is_left_alone() {
        let (w, h) = (64, 48);
        let scene = flat((TARGET * 255.0) as u8, w, h);
        let mut auto = AutoBrightness::default();
        let out = settle(&mut auto, &scene, w, h, 3.0);
        assert_eq!(out, scene, "a gamma inside the dead band changes nothing");
    }

    #[test]
    fn the_correction_fades_in_instead_of_jumping() {
        let (w, h) = (64, 48);
        let mut auto = AutoBrightness::default();
        let t0 = Instant::now();
        let dark = flat(25, w, h);
        let mut first = dark.clone();
        auto.process_at(&mut first, w as u32, h as u32, t0);
        let g_first = auto.gamma();
        // The scene gets bright; one frame later the gamma has moved only part of the way.
        let mut bright = flat(220, w, h);
        auto.process_at(&mut bright, w as u32, h as u32, t0 + Duration::from_millis(100));
        let wish = wish_gamma(mean(&flat(220, w, h), w, h));
        assert!(auto.gamma() > g_first && auto.gamma() < wish, "{g_first} -> {} (wish {wish})", auto.gamma());
    }

    #[test]
    fn a_covered_lens_is_not_amplified() {
        let (w, h) = (64, 48);
        let mut auto = AutoBrightness::default();
        let black = flat(3, w, h);
        let out = settle(&mut auto, &black, w, h, 3.0);
        assert_eq!(out, black);
        assert_eq!(auto.gamma(), 1.0);
    }

    #[test]
    fn the_middle_of_the_frame_counts_more_than_the_edges() {
        // A dark middle in a bright frame (a person against a window) must brighten, which a plain mean would hide.
        let (w, h) = (100, 100);
        let mut scene = flat(230, w, h);
        for y in 25..75 {
            for x in 25..75 {
                for c in 0..3 {
                    scene[(y * w + x) * 3 + c] = 30;
                }
            }
        }
        let weighted = mean(&scene, w, h);
        let plain = scene.iter().map(|&b| b as f32).sum::<f32>() / scene.len() as f32 / 255.0;
        assert!(weighted < plain, "weighted {weighted} against plain {plain}");
    }

    #[test]
    fn a_wrong_sized_buffer_is_left_alone() {
        let mut auto = AutoBrightness::default();
        let mut buf = vec![10u8; 10];
        auto.process(&mut buf, 64, 48);
        assert_eq!(buf, vec![10u8; 10]);
    }

    #[test]
    fn the_table_is_monotonic_and_keeps_black_and_white() {
        for g in [0.4, 0.7, 1.4, 1.8] {
            let lut = table(g);
            assert_eq!((lut[0], lut[255]), (0, 255));
            assert!(lut.windows(2).all(|p| p[0] <= p[1]), "gamma {g}");
        }
    }
}
