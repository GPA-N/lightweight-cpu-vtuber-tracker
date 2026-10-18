use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotatedRect {
    pub cx: f32,
    pub cy: f32,
    pub width: f32,
    pub height: f32,
    pub angle: f32,
}

/// NormalizeRadians: wrap to [-pi, pi).
fn normalize_radians(a: f32) -> f32 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}

/// Rotation from start to end point (pixels): 0 - atan2(-(y1-y0), x1-x0).
fn rotation(p0: (f32, f32), p1: (f32, f32)) -> f32 {
    normalize_radians(0.0 - (-(p1.1 - p0.1)).atan2(p1.0 - p0.0))
}

pub fn rect_from_detection(
    box_norm: (f32, f32, f32, f32),
    k0: (f32, f32),
    k1: (f32, f32),
    w: u32,
    h: u32,
) -> RotatedRect {
    let (wf, hf) = (w as f32, h as f32);
    let (xmin, ymin, bw, bh) = box_norm;
    RotatedRect {
        cx: (xmin + bw / 2.0) * wf,
        cy: (ymin + bh / 2.0) * hf,
        width: 1.5 * bw * wf,
        height: 1.5 * bh * hf,
        angle: rotation((k0.0 * wf, k0.1 * hf), (k1.0 * wf, k1.1 * hf)),
    }
}

pub fn rect_from_landmarks(
    landmarks_xy_norm: &[(f32, f32)],
    idx_a: usize,
    idx_b: usize,
    w: u32,
    h: u32,
) -> RotatedRect {
    let (wf, hf) = (w as f32, h as f32);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in landmarks_xy_norm {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    // square_long: side L = max(w*W, h*H), then scale 1.5.
    let side = ((x1 - x0) * wf).max((y1 - y0) * hf);
    let a = landmarks_xy_norm[idx_a];
    let b = landmarks_xy_norm[idx_b];
    RotatedRect {
        cx: (x0 + x1) / 2.0 * wf,
        cy: (y0 + y1) / 2.0 * hf,
        width: 1.5 * side,
        height: 1.5 * side,
        angle: rotation((a.0 * wf, a.1 * hf), (b.0 * wf, b.1 * hf)),
    }
}

/// Bilinear sample with replicate border. Returns RGB in 0..255.
fn bilinear(frame: &[u8], w: usize, h: usize, x: f32, y: f32) -> [f32; 3] {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let clamp = |v: f32, n: usize| (v.max(0.0) as usize).min(n - 1);
    let (xa, xb) = (clamp(x0, w), clamp(x0 + 1.0, w));
    let (ya, yb) = (clamp(y0, h), clamp(y0 + 1.0, h));
    let at = |xi: usize, yi: usize| {
        let i = (yi * w + xi) * 3;
        [frame[i] as f32, frame[i + 1] as f32, frame[i + 2] as f32]
    };
    let (p00, p10, p01, p11) = (at(xa, ya), at(xb, ya), at(xa, yb), at(xb, yb));
    let mut o = [0.0f32; 3];
    for c in 0..3 {
        let top = p00[c] * (1.0 - fx) + p10[c] * fx;
        let bot = p01[c] * (1.0 - fx) + p11[c] * fx;
        o[c] = top * (1.0 - fy) + bot * fy;
    }
    o
}

pub fn warp_to_tensor(
    frame_rgb: &[u8],
    w: u32,
    h: u32,
    rect: &RotatedRect,
    out_size: usize,
) -> Vec<f32> {
    let (wi, hi) = (w as usize, h as usize);
    assert_eq!(frame_rgb.len(), wi * hi * 3, "frame length");
    let mut out = Vec::with_capacity(out_size * out_size * 3);
    for v in 0..out_size {
        for u in 0..out_size {
            let (x, y) = crop_to_frame(u as f32, v as f32, rect, out_size);
            out.extend(bilinear(frame_rgb, wi, hi, x, y).map(|c| c / 255.0));
        }
    }
    out
}

pub fn crop_to_frame(u: f32, v: f32, rect: &RotatedRect, out_size: usize) -> (f32, f32) {
    let n = out_size as f32;
    let lx = (u / n - 0.5) * rect.width;
    let ly = (v / n - 0.5) * rect.height;
    let (s, c) = rect.angle.sin_cos();
    (rect.cx + c * lx - s * ly, rect.cy + s * lx + c * ly)
}

pub fn frame_to_crop(x: f32, y: f32, rect: &RotatedRect, out_size: usize) -> (f32, f32) {
    let n = out_size as f32;
    let (dx, dy) = (x - rect.cx, y - rect.cy);
    let (s, c) = rect.angle.sin_cos();
    (
        ((c * dx + s * dy) / rect.width + 0.5) * n,
        ((-s * dx + c * dy) / rect.height + 0.5) * n,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 640;
    const H: u32 = 480;

    fn example_rect() -> RotatedRect {
        rect_from_detection(
            (0.40, 0.30, 0.20, 0.25),
            (0.45, 0.40),
            (0.55, 0.42),
            W,
            H,
        )
    }

    #[test]
    fn worked_example_rect_and_angle() {
        let r = example_rect();
        assert!((r.angle - 0.148890).abs() < 1e-5, "angle {}", r.angle);
        assert!((r.cx - 320.0).abs() < 1e-3, "cx {}", r.cx);
        assert!((r.cy - 204.0).abs() < 1e-3, "cy {}", r.cy);
        assert!((r.width - 192.0).abs() < 1e-3, "width {}", r.width);
        assert!((r.height - 180.0).abs() < 1e-3, "height {}", r.height);
    }

    #[test]
    fn landmarks_rect_squares_then_scales() {
        // Bounds in px: x 256..384 (128 wide), y 144..240 (96 high).
        let lm = [(0.4, 0.3), (0.6, 0.5)];
        let r = rect_from_landmarks(&lm, 0, 1, W, H);
        assert!((r.cx - 320.0).abs() < 1e-3, "cx {}", r.cx);
        assert!((r.cy - 192.0).abs() < 1e-3, "cy {}", r.cy);
        assert!((r.width - 192.0).abs() < 1e-3, "width {}", r.width);
        assert!((r.height - 192.0).abs() < 1e-3, "height {}", r.height);
        assert!((r.angle - 0.643501).abs() < 1e-5, "angle {}", r.angle);
    }

    #[test]
    fn point_maps_to_crop_and_back() {
        let r = example_rect();
        let (u, v) = frame_to_crop(300.0, 200.0, &r, 256);
        assert!((u - 100.837).abs() < 0.01, "u {u}");
        assert!((v - 126.594).abs() < 0.01, "v {v}");
        let (x, y) = crop_to_frame(u, v, &r, 256);
        assert!((x - 300.0).abs() < 0.01, "x {x}");
        assert!((y - 200.0).abs() < 0.01, "y {y}");
    }

    #[test]
    fn crop_origin_lands_on_expected_frame_point() {
        let r = example_rect();
        let (x, y) = crop_to_frame(0.0, 0.0, &r, 256);
        assert!((x - 238.413).abs() < 0.01, "x {x}");
        assert!((y - 100.755).abs() < 0.01, "y {y}");
    }

    #[test]
    fn round_trip_fifty_pseudo_random_points_under_half_pixel() {
        let r = example_rect();
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 40) as f32 / (1u64 << 24) as f32 * 256.0
        };
        for _ in 0..50 {
            let (u, v) = (next(), next());
            let (x, y) = crop_to_frame(u, v, &r, 256);
            let (u2, v2) = frame_to_crop(x, y, &r, 256);
            let err = ((u2 - u).powi(2) + (v2 - v).powi(2)).sqrt();
            assert!(err < 0.5, "error {err} at ({u}, {v})");
        }
    }

    #[test]
    fn warp_constant_image_gives_constant_tensor() {
        let frame = vec![128u8; (W * H * 3) as usize];
        let out = warp_to_tensor(&frame, W, H, &example_rect(), 256);
        assert_eq!(out.len(), 256 * 256 * 3);
        for &x in &out {
            assert!((x - 128.0 / 255.0).abs() < 1e-5, "value {x}");
        }
    }

    #[test]
    fn warp_gradient_axis_aligned_matches_corners() {
        // 256x256 frame, R = x, G = y, B = 0.
        let n = 256usize;
        let mut frame = vec![0u8; n * n * 3];
        for y in 0..n {
            for x in 0..n {
                let i = (y * n + x) * 3;
                frame[i] = x as u8;
                frame[i + 1] = y as u8;
            }
        }
        let rect = RotatedRect {
            cx: 128.0,
            cy: 128.0,
            width: 256.0,
            height: 256.0,
            angle: 0.0,
        };
        let out = warp_to_tensor(&frame, 256, 256, &rect, 256);
        let px = |u: usize, v: usize| {
            let i = (v * 256 + u) * 3;
            [out[i], out[i + 1], out[i + 2]]
        };
        let close = |a: [f32; 3], b: [f32; 3]| (0..3).all(|c| (a[c] - b[c]).abs() < 1e-4);
        assert!(close(px(0, 0), [0.0, 0.0, 0.0]), "tl {:?}", px(0, 0));
        assert!(close(px(255, 0), [1.0, 0.0, 0.0]), "tr {:?}", px(255, 0));
        assert!(close(px(0, 255), [0.0, 1.0, 0.0]), "bl {:?}", px(0, 255));
        assert!(close(px(255, 255), [1.0, 1.0, 0.0]), "br {:?}", px(255, 255));
    }
}
