//! CIGPose-m limb stage: 17 COCO keypoints in frame pixels from one person box.
//!
//! Ported exactly from the `cigpose-onnx` 1.0.1 wheel (`inference.py`: `preprocess_person`,
//! `decode_simcc`, `remap_to_frame`). Differences from the usual affine warp are deliberate: the
//! reference crops an integer-truncated rectangle clamped to the frame, resizes it with bilinear
//! interpolation (OpenCV `INTER_LINEAR`, half-pixel centres, no anti-aliasing) and does not pad.
//!
//! Model: input `[1,3,256,192]` (NCHW, RGB, ImageNet mean/std in 0..255 units), two SimCC outputs
//! x `[1,17,384]` and y `[1,17,512]`, split ratio 2.0. Output confidence is the minimum of the two
//! logit maxima (MMPose `get_simcc_maximum`), so it is a logit, not a probability.

use std::path::Path;

use crate::engine::{load_model, Engine, ModelRun};
use crate::upper_body::Image;

pub const INPUT_W: usize = 192;
pub const INPUT_H: usize = 256;
pub const SPLIT_RATIO: f32 = 2.0;
pub const KEYPOINTS: usize = 17;
const PAD: f64 = 1.25;
const MEAN: [f32; 3] = [123.675, 116.28, 103.53];
const STD: [f32; 3] = [58.395, 57.12, 57.375];

/// COCO-17 indices of the arm joints.
pub const L_SHOULDER: usize = 5;
pub const R_SHOULDER: usize = 6;
pub const L_ELBOW: usize = 7;
pub const R_ELBOW: usize = 8;
pub const L_WRIST: usize = 9;
pub const R_WRIST: usize = 10;

/// One keypoint in frame pixels; `score` is min(max logit x, max logit y).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keypoint {
    pub x: f32,
    pub y: f32,
    pub score: f32,
}

/// Person box in frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PersonBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A stage that turns a person box into 2D limb keypoints. BlazePose stays in `UpperBody`; this
/// is the box-driven alternative.
pub trait LimbStage {
    fn run(&mut self, img: &Image, person: PersonBox) -> Result<[Keypoint; KEYPOINTS], String>;
}

/// Integer crop rectangle `(x1, y1, x2, y2)` in frame pixels, exclusive end.
pub type CropRegion = (usize, usize, usize, usize);

/// Expands the box to the model aspect, pads by 1.25 and clamps to the frame
/// (`preprocess_person`, including its integer truncation). An empty crop falls back to the frame.
pub fn crop_region(frame_w: usize, frame_h: usize, b: PersonBox) -> CropRegion {
    let (x1, y1) = (b.x as f64, b.y as f64);
    let (x2, y2) = (x1 + b.w as f64, y1 + b.h as f64);
    let (cx, cy) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
    let (mut bw, mut bh) = (x2 - x1, y2 - y1);
    let aspect = INPUT_W as f64 / INPUT_H as f64;
    if bw / bh.max(1.0) > aspect {
        bh = bw / aspect;
    } else {
        bw = bh * aspect;
    }
    bw *= PAD;
    bh *= PAD;
    // Python int() truncates toward zero; the max/min clamps make every value non-negative.
    let sx1 = (cx - bw / 2.0).max(0.0) as usize;
    let sy1 = (cy - bh / 2.0).max(0.0) as usize;
    let sx2 = (cx + bw / 2.0).min(frame_w as f64) as usize;
    let sy2 = (cy + bh / 2.0).min(frame_h as f64) as usize;
    if sx2 <= sx1 || sy2 <= sy1 {
        (0, 0, frame_w, frame_h)
    } else {
        (sx1, sy1, sx2, sy2)
    }
}

/// Crop, bilinear resize to 192x256 (rounded to u8 like OpenCV), normalise, NCHW.
pub fn preprocess(img: &Image, region: CropRegion) -> Vec<f32> {
    let (sx1, sy1, sx2, sy2) = region;
    let (cw, ch) = (sx2 - sx1, sy2 - sy1);
    let (scale_x, scale_y) = (cw as f32 / INPUT_W as f32, ch as f32 / INPUT_H as f32);
    let plane = INPUT_W * INPUT_H;
    let mut out = vec![0.0f32; 3 * plane];
    let at = |x: usize, y: usize, c: usize| img.rgb[((sy1 + y) * img.w + sx1 + x) * 3 + c] as f32;
    for v in 0..INPUT_H {
        let fy = ((v as f32 + 0.5) * scale_y - 0.5).max(0.0);
        let y0 = (fy.floor() as usize).min(ch - 1);
        let y1 = (y0 + 1).min(ch - 1);
        let wy = fy - y0 as f32;
        for u in 0..INPUT_W {
            let fx = ((u as f32 + 0.5) * scale_x - 0.5).max(0.0);
            let x0 = (fx.floor() as usize).min(cw - 1);
            let x1 = (x0 + 1).min(cw - 1);
            let wx = fx - x0 as f32;
            for c in 0..3 {
                let top = at(x0, y0, c) * (1.0 - wx) + at(x1, y0, c) * wx;
                let bot = at(x0, y1, c) * (1.0 - wx) + at(x1, y1, c) * wx;
                let px = (top * (1.0 - wy) + bot * wy).round();
                out[c * plane + v * INPUT_W + u] = (px - MEAN[c]) / STD[c];
            }
        }
    }
    out
}

/// SimCC decode: argmax per keypoint on each axis divided by the split ratio, score = min of the
/// two maxima. `x` is `[k, nx]`, `y` is `[k, ny]`, row-major.
pub fn decode_simcc(x: &[f32], y: &[f32], k: usize) -> Result<Vec<Keypoint>, String> {
    if k == 0 || !x.len().is_multiple_of(k) || !y.len().is_multiple_of(k) || x.is_empty() || y.is_empty() {
        return Err(format!("simcc sizes {} / {} do not split into {k} rows", x.len(), y.len()));
    }
    let argmax = |row: &[f32]| {
        let mut best = (0usize, f32::NEG_INFINITY);
        for (i, &v) in row.iter().enumerate() {
            if v > best.1 {
                best = (i, v);
            }
        }
        best
    };
    let (nx, ny) = (x.len() / k, y.len() / k);
    Ok((0..k)
        .map(|i| {
            let (xi, xv) = argmax(&x[i * nx..(i + 1) * nx]);
            let (yi, yv) = argmax(&y[i * ny..(i + 1) * ny]);
            Keypoint { x: xi as f32 / SPLIT_RATIO, y: yi as f32 / SPLIT_RATIO, score: xv.min(yv) }
        })
        .collect())
}

/// Maps model-input coordinates back to frame pixels (`remap_to_frame`).
pub fn remap_to_frame(kps: &mut [Keypoint], region: CropRegion) {
    let (sx1, sy1, sx2, sy2) = region;
    let sx = (sx2 - sx1) as f32 / INPUT_W as f32;
    let sy = (sy2 - sy1) as f32 / INPUT_H as f32;
    for k in kps {
        k.x = k.x * sx + sx1 as f32;
        k.y = k.y * sy + sy1 as f32;
    }
}

/// CIGPose-m on either engine.
pub struct CigPose {
    model: Box<dyn ModelRun + Send>,
}

impl CigPose {
    pub fn load(engine: Engine, path: &Path, threads: usize) -> Result<Self, String> {
        Ok(Self { model: load_model(engine, path, threads).map_err(|e| e.to_string())? })
    }
}

impl LimbStage for CigPose {
    fn run(&mut self, img: &Image, person: PersonBox) -> Result<[Keypoint; KEYPOINTS], String> {
        if img.rgb.len() != img.w * img.h * 3 || img.w == 0 || img.h == 0 {
            return Err("image buffer does not match its size".into());
        }
        let region = crop_region(img.w, img.h, person);
        let input = preprocess(img, region);
        let outs = self.model.run(&input, &[1, 3, INPUT_H, INPUT_W]).map_err(|e| e.to_string())?;
        if outs.len() < 2 {
            return Err(format!("expected 2 SimCC outputs, got {}", outs.len()));
        }
        let mut kps = decode_simcc(&outs[0].data, &outs[1].data, KEYPOINTS)?;
        remap_to_frame(&mut kps, region);
        kps.try_into().map_err(|_| "keypoint count".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_wide_box_gets_height_from_aspect_then_padding() {
        // 400x100 box in a big frame: wider than 192/256, so height = 400/0.75, then both x1.25.
        let r = crop_region(2000, 2000, PersonBox { x: 800.0, y: 900.0, w: 400.0, h: 100.0 });
        // centre (1000, 950); bw = 500, bh = 666.67
        assert_eq!(r, (750, 616, 1250, 1283));
    }

    #[test]
    fn crop_clamps_to_frame_and_truncates() {
        let r = crop_region(200, 150, PersonBox { x: 10.0, y: 1.5, w: 167.0, h: 144.0 });
        assert!(r.0 == 0 && r.1 == 0 && r.2 <= 200 && r.3 == 150, "{r:?}");
    }

    #[test]
    fn crop_empty_falls_back_to_frame() {
        let r = crop_region(100, 80, PersonBox { x: 500.0, y: 500.0, w: 10.0, h: 10.0 });
        assert_eq!(r, (0, 0, 100, 80));
    }

    #[test]
    fn simcc_argmax_split_and_min_score() {
        let mut x = vec![0.0; 2 * 4];
        let mut y = vec![0.0; 2 * 6];
        x[2] = 5.0; // kp0 x argmax 2
        y[5] = 3.0; // kp0 y argmax 5
        x[4 + 1] = 1.0; // kp1 x argmax 1
        y[6 + 4] = 9.0; // kp1 y argmax 4
        let k = decode_simcc(&x, &y, 2).unwrap();
        assert_eq!(k[0], Keypoint { x: 1.0, y: 2.5, score: 3.0 });
        assert_eq!(k[1], Keypoint { x: 0.5, y: 2.0, score: 1.0 });
    }

    #[test]
    fn remap_scales_and_offsets() {
        let mut k = [Keypoint { x: 96.0, y: 128.0, score: 1.0 }];
        remap_to_frame(&mut k, (10, 20, 10 + 384, 20 + 512));
        assert_eq!((k[0].x, k[0].y), (202.0, 276.0));
    }

    #[test]
    fn preprocess_constant_image_is_normalised_nchw() {
        let rgb: Vec<u8> = (0..16 * 16).flat_map(|_| [123u8, 116, 104]).collect();
        let img = Image { rgb: &rgb, w: 16, h: 16 };
        let t = preprocess(&img, (0, 0, 16, 16));
        assert_eq!(t.len(), 3 * INPUT_W * INPUT_H);
        let plane = INPUT_W * INPUT_H;
        assert!((t[0] - (123.0 - MEAN[0]) / STD[0]).abs() < 1e-5);
        assert!((t[plane] - (116.0 - MEAN[1]) / STD[1]).abs() < 1e-5);
        assert!((t[2 * plane + plane - 1] - (104.0 - MEAN[2]) / STD[2]).abs() < 1e-5);
    }
}
