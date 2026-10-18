//! Face detector wrapper: BlazeFace-style SSD (`face_detector.onnx`), CPU only.
//! Layout, letterbox, anchors, decode and NMS follow `docs/notes/model-detector.md`.

use std::fmt;

use core::frame::Frame;
use ort::session::Session;
use ort::value::Tensor;

/// Square side the model expects (NHWC 1x128x128x3).
pub const INPUT_SIZE: usize = 128;
/// Total anchors (512 at stride 8 + 384 at stride 16).
pub const ANCHOR_COUNT: usize = 896;
/// Values per anchor in `regressors`: 4 box values + 6 keypoints x 2.
pub const REGRESSOR_LEN: usize = 16;
/// Keypoints per face: right_eye, left_eye, nose, mouth, right_ear, left_ear.
pub const KEYPOINT_COUNT: usize = 6;
/// Score threshold on the sigmoid output used by the converter.
pub const MIN_SCORE: f32 = 0.5;
/// Weighted NMS IoU threshold (strict `>`) from the converter.
pub const NMS_IOU: f32 = 0.3;

/// One detected face. All values are normalized to the original frame, top-left origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    /// (xmin, ymin, width, height).
    pub box_norm: (f32, f32, f32, f32),
    pub keypoints: [(f32, f32); KEYPOINT_COUNT],
    pub score: f32,
}

/// Errors from running or decoding the face detector.
#[derive(Debug)]
pub enum DetectorError {
    /// A raw output has the wrong element count (`expected` and `got` are element counts).
    WrongOutputShape { expected: usize, got: usize },
    /// ONNX Runtime failed while building inputs or running the session.
    Ort(String),
}

impl fmt::Display for DetectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongOutputShape { expected, got } => {
                write!(f, "wrong detector output shape: expected {expected} elements, got {got}")
            }
            Self::Ort(msg) => write!(f, "face detector ONNX Runtime error: {msg}"),
        }
    }
}

impl std::error::Error for DetectorError {}

/// How the frame was fitted into the square input. Values are fractions of the input square.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Letterbox {
    /// Left padding, as a fraction of the square.
    pub pad_x: f32,
    /// Top padding, as a fraction of the square.
    pub pad_y: f32,
    /// Image width inside the square, as a fraction of the square.
    pub span_x: f32,
    /// Image height inside the square, as a fraction of the square.
    pub span_y: f32,
}

/// Letterbox geometry for a `width` x `height` frame: scale `128 / max(w, h)`, centred.
pub fn letterbox_for(width: u32, height: u32) -> Letterbox {
    let s = INPUT_SIZE as f32 / width.max(height) as f32;
    let span_x = width as f32 * s / INPUT_SIZE as f32;
    let span_y = height as f32 * s / INPUT_SIZE as f32;
    Letterbox {
        pad_x: (1.0 - span_x) / 2.0,
        pad_y: (1.0 - span_y) / 2.0,
        span_x,
        span_y,
    }
}

/// Input tensor (NHWC 1x128x128x3, RGB, `(v/255)*2-1`) and the letterbox used to build it.
/// The frame must be tightly packed RGB24 (`bytes.len() == width * height * 3`).
pub fn preprocess(frame: &Frame) -> (Vec<f32>, Letterbox) {
    let (w, h) = (frame.width as usize, frame.height as usize);
    debug_assert_eq!(frame.bytes.len(), w * h * 3);
    let lb = letterbox_for(frame.width, frame.height);
    let size = INPUT_SIZE as f32;
    let s = size / frame.width.max(frame.height) as f32;
    let (ox, oy) = (lb.pad_x * size, lb.pad_y * size);
    let (cw, ch) = (lb.span_x * size, lb.span_y * size);
    let src = &frame.bytes;

    // Border stays 0.0 (raw black) and is normalized below, so it becomes -1.0.
    let mut out = vec![0.0f32; INPUT_SIZE * INPUT_SIZE * 3];
    for dy in 0..INPUT_SIZE {
        let dyf = dy as f32;
        if dyf < oy || dyf >= oy + ch {
            continue;
        }
        let sy = ((dyf - oy) / s).clamp(0.0, (h - 1) as f32);
        let y0 = sy.floor() as usize;
        let y1 = (y0 + 1).min(h - 1);
        let fy = sy - y0 as f32;
        for dx in 0..INPUT_SIZE {
            let dxf = dx as f32;
            if dxf < ox || dxf >= ox + cw {
                continue;
            }
            let sx = ((dxf - ox) / s).clamp(0.0, (w - 1) as f32);
            let x0 = sx.floor() as usize;
            let x1 = (x0 + 1).min(w - 1);
            let fx = sx - x0 as f32;
            for c in 0..3 {
                let p = |x: usize, y: usize| f32::from(src[(y * w + x) * 3 + c]);
                let top = p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx;
                let bot = p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx;
                out[(dy * INPUT_SIZE + dx) * 3 + c] = top * (1.0 - fy) + bot * fy;
            }
        }
    }
    for v in &mut out {
        *v = *v / 255.0 * 2.0 - 1.0;
    }
    (out, lb)
}

/// The 896 anchor centres, in model order: stride-8 layer first, then the stride-16 group.
/// Each layer is y-major; each cell repeats its centre once per anchor (2 at stride 8, 6 at stride 16).
pub fn generate_anchors() -> Vec<[f32; 2]> {
    const LAYERS: [(usize, usize); 2] = [(8, 2), (16, 6)];
    let mut anchors = Vec::with_capacity(ANCHOR_COUNT);
    for (stride, per_cell) in LAYERS {
        let fm = INPUT_SIZE.div_ceil(stride);
        for y in 0..fm {
            for x in 0..fm {
                let centre = [(x as f32 + 0.5) / fm as f32, (y as f32 + 0.5) / fm as f32];
                anchors.extend(std::iter::repeat_n(centre, per_cell));
            }
        }
    }
    assert_eq!(anchors.len(), ANCHOR_COUNT);
    anchors
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x.clamp(-100.0, 100.0)).exp())
}

/// A decoded anchor in letterbox space (fractions of the square).
#[derive(Clone, Copy)]
struct Candidate {
    score: f32,
    /// (xmin, ymin, width, height).
    bbox: (f32, f32, f32, f32),
    keypoints: [(f32, f32); KEYPOINT_COUNT],
}

fn iou(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let ix = (a.0 + a.2).min(b.0 + b.2) - a.0.max(b.0);
    let iy = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
    if ix <= 0.0 || iy <= 0.0 {
        return 0.0;
    }
    let inter = ix * iy;
    inter / (a.2 * a.3 + b.2 * b.3 - inter)
}

/// Weighted NMS: best-first; every candidate with IoU > `iou_threshold` joins the best's cluster.
/// The cluster's box and keypoints are score-weighted averages, and its score is the best score.
fn weighted_nms(mut pending: Vec<Candidate>, iou_threshold: f32) -> Vec<Candidate> {
    pending.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut out = Vec::new();
    while !pending.is_empty() {
        let best = pending.remove(0);
        let (overlapping, rest): (Vec<Candidate>, Vec<Candidate>) = pending
            .into_iter()
            .partition(|c| iou(best.bbox, c.bbox) > iou_threshold);
        pending = rest;

        let members: Vec<Candidate> = std::iter::once(best).chain(overlapping).collect();
        let total: f32 = members.iter().map(|c| c.score).sum();
        let n = members.len() as f32;
        let mut bbox = (0.0, 0.0, 0.0, 0.0);
        let mut keypoints = [(0.0, 0.0); KEYPOINT_COUNT];
        for c in &members {
            let w = if total > 0.0 { c.score / total } else { 1.0 / n };
            bbox.0 += w * c.bbox.0;
            bbox.1 += w * c.bbox.1;
            bbox.2 += w * c.bbox.2;
            bbox.3 += w * c.bbox.3;
            for (acc, kp) in keypoints.iter_mut().zip(c.keypoints) {
                acc.0 += w * kp.0;
                acc.1 += w * kp.1;
            }
        }
        out.push(Candidate {
            score: members[0].score,
            bbox,
            keypoints,
        });
    }
    out
}

/// Map a letterbox-space candidate to original-frame normalized coordinates.
fn unletterbox(c: Candidate, lb: &Letterbox) -> Detection {
    let map = |x: f32, y: f32| ((x - lb.pad_x) / lb.span_x, (y - lb.pad_y) / lb.span_y);
    let (x0, y0) = map(c.bbox.0, c.bbox.1);
    let (x1, y1) = map(c.bbox.0 + c.bbox.2, c.bbox.1 + c.bbox.3);
    let mut keypoints = [(0.0, 0.0); KEYPOINT_COUNT];
    for (dst, src) in keypoints.iter_mut().zip(c.keypoints) {
        *dst = map(src.0, src.1);
    }
    Detection {
        box_norm: (x0, y0, x1 - x0, y1 - y0),
        keypoints,
        score: c.score,
    }
}

/// Decode raw model outputs into faces in original-frame normalized coordinates.
pub fn decode(
    raw_regressors: &[f32],
    raw_scores: &[f32],
    anchors: &[[f32; 2]],
    letterbox: &Letterbox,
    min_score: f32,
) -> Result<Vec<Detection>, DetectorError> {
    let expected_regs = anchors.len() * REGRESSOR_LEN;
    if raw_regressors.len() != expected_regs {
        return Err(DetectorError::WrongOutputShape {
            expected: expected_regs,
            got: raw_regressors.len(),
        });
    }
    if raw_scores.len() != anchors.len() {
        return Err(DetectorError::WrongOutputShape {
            expected: anchors.len(),
            got: raw_scores.len(),
        });
    }

    let size = INPUT_SIZE as f32;
    let mut candidates = Vec::new();
    for (i, (&logit, anchor)) in raw_scores.iter().zip(anchors).enumerate() {
        let score = sigmoid(logit);
        if score < min_score {
            continue;
        }
        let r = &raw_regressors[i * REGRESSOR_LEN..(i + 1) * REGRESSOR_LEN];
        let cx = r[0] / size + anchor[0];
        let cy = r[1] / size + anchor[1];
        let w = r[2] / size;
        let h = r[3] / size;
        let mut keypoints = [(0.0, 0.0); KEYPOINT_COUNT];
        for (k, kp) in keypoints.iter_mut().enumerate() {
            *kp = (
                r[4 + 2 * k] / size + anchor[0],
                r[5 + 2 * k] / size + anchor[1],
            );
        }
        candidates.push(Candidate {
            score,
            bbox: (cx - w / 2.0, cy - h / 2.0, w, h),
            keypoints,
        });
    }

    Ok(weighted_nms(candidates, NMS_IOU)
        .into_iter()
        .map(|c| unletterbox(c, letterbox))
        .collect())
}

/// Face detector session wrapper.
pub struct Detector {
    session: Session,
    anchors: Vec<[f32; 2]>,
}

impl Detector {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            anchors: generate_anchors(),
        }
    }

    /// Run the detector on one frame.
    pub fn detect(&mut self, frame: &Frame) -> Result<Vec<Detection>, DetectorError> {
        let (input, lb) = preprocess(frame);
        let tensor = Tensor::from_array(([1usize, INPUT_SIZE, INPUT_SIZE, 3], input))
            .map_err(|e| DetectorError::Ort(e.to_string()))?;
        let outputs = self
            .session
            .run(ort::inputs!["input" => tensor])
            .map_err(|e| DetectorError::Ort(e.to_string()))?;
        let (_, regressors) = outputs["regressors"]
            .try_extract_tensor::<f32>()
            .map_err(|e| DetectorError::Ort(e.to_string()))?;
        let (_, scores) = outputs["classificators"]
            .try_extract_tensor::<f32>()
            .map_err(|e| DetectorError::Ort(e.to_string()))?;
        decode(regressors, scores, &self.anchors, &lb, MIN_SCORE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::frame::PixelFormat;
    use std::time::Instant;

    fn frame(width: u32, height: u32, fill: u8) -> Frame {
        Frame {
            width,
            height,
            format: PixelFormat::Rgb24,
            bytes: vec![fill; (width * height * 3) as usize],
            arrived: Instant::now(),
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// Raw regressors with zeros except the given anchor's box (in letterbox units x128).
    fn one_strong_anchor(index: usize, box_raw: [f32; 4]) -> (Vec<f32>, Vec<f32>) {
        let mut regs = vec![0.0f32; ANCHOR_COUNT * REGRESSOR_LEN];
        regs[index * REGRESSOR_LEN..index * REGRESSOR_LEN + 4].copy_from_slice(&box_raw);
        let mut scores = vec![-10.0f32; ANCHOR_COUNT];
        scores[index] = 10.0;
        (regs, scores)
    }

    #[test]
    fn preprocess_640x480_has_tensor_length_and_letterbox_bands() {
        let f = frame(640, 480, 255);
        let (input, lb) = preprocess(&f);
        assert_eq!(input.len(), 128 * 128 * 3);
        assert!(close(lb.pad_x, 0.0) && close(lb.span_x, 1.0));
        assert!(close(lb.pad_y, 0.125) && close(lb.span_y, 0.75));
        let px = |row: usize, col: usize| &input[(row * 128 + col) * 3..(row * 128 + col) * 3 + 3];
        // Top and bottom bands are zero before normalization, so -1.0 after.
        assert_eq!(px(0, 64), &[-1.0, -1.0, -1.0]);
        assert_eq!(px(127, 64), &[-1.0, -1.0, -1.0]);
        // Image interior is white, so 1.0.
        assert_eq!(px(64, 64), &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn preprocess_normalizes_black_to_minus_one_and_channels_are_rgb() {
        let mut f = frame(128, 128, 0);
        // Pure red pixel at (0,0): R=255, G=0, B=0.
        f.bytes[0] = 255;
        let (input, _) = preprocess(&f);
        assert!(close(input[0], 1.0));
        assert!(close(input[1], -1.0));
        assert!(close(input[2], -1.0));
        assert!(close(input[3], -1.0));
    }

    #[test]
    fn anchors_count_and_first_matches_note() {
        let a = generate_anchors();
        assert_eq!(a.len(), ANCHOR_COUNT);
        assert!(close(a[0][0], 0.5 / 16.0));
        assert!(close(a[0][1], 0.5 / 16.0));
        // Stride-8 layer: 16x16 cells x 2 anchors = 512, so index 512 starts the stride-16 group.
        assert!(close(a[512][0], 0.5 / 8.0));
        assert!(close(a[512][1], 0.5 / 8.0));
    }

    #[test]
    fn decode_single_strong_anchor_gives_one_hand_computed_box() {
        // Anchor 0 centre (1/32, 1/32). Raw box cx=64, cy=64, w=32, h=64 (x128 units)
        // -> cx = 0.5 + 1/32, cy = 0.5 + 1/32, w = 0.25, h = 0.5.
        // Letterbox 640x480 (pad_y 0.125, span_y 0.75): normalized box in frame:
        // xmin = (cx - w/2) = 0.5 + 1/32 - 0.125 = 0.40625 ; width 0.25.
        // ymin_lb = cy - h/2 = 0.25 + 1/32 = 0.28125 ; y_orig = (0.28125 - 0.125)/0.75 = 0.20833.
        // height_orig = 0.5/0.75 = 0.66667.
        let lb = letterbox_for(640, 480);
        let anchors = generate_anchors();
        let (regs, scores) = one_strong_anchor(0, [64.0, 64.0, 32.0, 64.0]);
        let dets = decode(&regs, &scores, &anchors, &lb, MIN_SCORE).unwrap();
        assert_eq!(dets.len(), 1);
        let d = &dets[0];
        assert!(close(d.score, 1.0 / (1.0 + (-10.0f32).exp())));
        let (x, y, w, h) = d.box_norm;
        assert!(close(x, 0.40625), "x {x}");
        assert!(close(w, 0.25), "w {w}");
        assert!(close(y, 0.208333), "y {y}");
        assert!(close(h, 0.666667), "h {h}");
    }

    #[test]
    fn decode_rejects_wrong_regressor_length() {
        let lb = letterbox_for(640, 480);
        let anchors = generate_anchors();
        let regs = vec![0.0f32; ANCHOR_COUNT * REGRESSOR_LEN - 1];
        let scores = vec![0.0f32; ANCHOR_COUNT];
        match decode(&regs, &scores, &anchors, &lb, MIN_SCORE) {
            Err(DetectorError::WrongOutputShape { expected, got }) => {
                assert_eq!(expected, ANCHOR_COUNT * REGRESSOR_LEN);
                assert_eq!(got, ANCHOR_COUNT * REGRESSOR_LEN - 1);
            }
            other => panic!("expected WrongOutputShape, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_wrong_score_length() {
        let lb = letterbox_for(640, 480);
        let anchors = generate_anchors();
        let regs = vec![0.0f32; ANCHOR_COUNT * REGRESSOR_LEN];
        let scores = vec![0.0f32; ANCHOR_COUNT + 1];
        assert!(matches!(
            decode(&regs, &scores, &anchors, &lb, MIN_SCORE),
            Err(DetectorError::WrongOutputShape { expected: ANCHOR_COUNT, got: 897 })
        ));
    }

    #[test]
    fn weighted_nms_merges_overlapping_boxes_and_keeps_distant_one() {
        let lb = letterbox_for(128, 128); // identity letterbox: pad 0, span 1
        let anchors = generate_anchors();
        let mut regs = vec![0.0f32; ANCHOR_COUNT * REGRESSOR_LEN];
        let mut scores = vec![-10.0f32; ANCHOR_COUNT];
        // Anchor 0 and anchor 1 (same cell, neighbouring anchors) overlap heavily.
        // Anchor 0: box raw (64,64,64,64) -> cx = 0.5+1/32, w = 0.5.
        // Anchor 1: same size, shifted by 0.02 in x.
        let set = |regs: &mut Vec<f32>, i: usize, b: [f32; 4]| {
            regs[i * REGRESSOR_LEN..i * REGRESSOR_LEN + 4].copy_from_slice(&b);
        };
        set(&mut regs, 0, [64.0, 64.0, 64.0, 64.0]);
        set(&mut regs, 1, [64.0 + 0.02 * 128.0, 64.0, 64.0, 64.0]);
        scores[0] = 4.0;
        scores[1] = 2.0;
        // A distant face in the top-left area (last anchor centre is (0.9375, 0.9375); raw -96 -> cx 0.1875).
        let far = ANCHOR_COUNT - 1;
        set(&mut regs, far, [-96.0, -96.0, 16.0, 16.0]);
        scores[far] = 3.0;
        let dets = decode(&regs, &scores, &anchors, &lb, MIN_SCORE).unwrap();
        assert_eq!(dets.len(), 2, "two clusters expected, got {dets:?}");
        // Best cluster first: merged box is the score-weighted average of anchors 0 and 1.
        let s0 = 1.0 / (1.0 + (-4.0f32).exp());
        let s1 = 1.0 / (1.0 + (-2.0f32).exp());
        let w0 = s0 / (s0 + s1);
        let w1 = s1 / (s0 + s1);
        let cx0 = 0.5 + 1.0 / 32.0;
        let cx1 = cx0 + 0.02;
        let expected_xmin = (w0 * cx0 + w1 * cx1) - 0.25;
        let (x, _, w, _) = dets[0].box_norm;
        assert!(close(x, expected_xmin), "x {x} vs {expected_xmin}");
        assert!(close(w, 0.5));
        assert!(close(dets[0].score, s0));
        // Distant box kept with its own geometry.
        let (fx, _, fw, _) = dets[1].box_norm;
        assert!(close(fw, 0.125));
        assert!(close(fx, 0.125), "fx {fx}");
    }

    #[test]
    fn unletterbox_maps_square_centre_to_frame_centre_640x480() {
        let lb = letterbox_for(640, 480);
        let anchors = generate_anchors();
        // Anchor 0 centre is (1/32, 1/32). Raw offset chosen so the decoded box centre is exactly (0.5, 0.5)
        // in the square, which must map to (0.5, 0.5) of the 640x480 frame.
        let a = anchors[0];
        let raw_cx = (0.5 - a[0]) * 128.0;
        let raw_cy = (0.5 - a[1]) * 128.0;
        let (regs, scores) = one_strong_anchor(0, [raw_cx, raw_cy, 64.0, 64.0]);
        let dets = decode(&regs, &scores, &anchors, &lb, MIN_SCORE).unwrap();
        let (x, y, w, h) = dets[0].box_norm;
        assert!(close(x + w / 2.0, 0.5), "centre x {}", x + w / 2.0);
        assert!(close(y + h / 2.0, 0.5), "centre y {}", y + h / 2.0);
    }
}
