//! Hand stage for the body path: palm detector (anchor decode, NMS) -> rotated hand ROI -> 21 hand
//! landmarks, with the crop placed from the body model's wrist. The hand stage sits behind
//! [`HandStage`] so the model files and their pre/post-processing can be swapped. The shipped
//! implementation, [`Mp2023Hands`], is PROVISIONAL (2023 port). Image is processed as given (not
//! mirrored); coordinates are source pixels.
use core::body::{HandLandmarks, PoseLandmark, SubjectSide, HAND_POINTS, POSE_POINTS};
use facelogic::crop::{crop_to_frame, RotatedRect};
use ort::session::Session;
use ort::value::Tensor;

const PALM_SIZE: usize = 192;
const HAND_LM_SIZE: usize = 224;
const INPUT_NAME: &str = "input_1";
const DET_SCORE_MIN: f32 = 0.5;
const NMS_IOU: f32 = 0.3;

/// Borrowed RGB image, `w * h * 3` bytes.
#[derive(Clone, Copy)]
pub struct Image<'a> {
    pub rgb: &'a [u8],
    pub w: usize,
    pub h: usize,
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x.clamp(-100.0, 100.0)).exp())
}

fn ort_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Bilinear sample, black outside the image (MediaPipe zero padding).
fn pixel(img: &Image, x: i64, y: i64) -> [f32; 3] {
    if x < 0 || y < 0 || x >= img.w as i64 || y >= img.h as i64 {
        return [0.0; 3];
    }
    let i = (y as usize * img.w + x as usize) * 3;
    [img.rgb[i] as f32, img.rgb[i + 1] as f32, img.rgb[i + 2] as f32]
}

/// Warps `rect` to an `out x out` NHWC RGB tensor, 0..1 (or -1..1 when `signed`), black outside.
pub fn warp_black(img: &Image, rect: &RotatedRect, out: usize, signed: bool) -> Vec<f32> {
    let mut t = Vec::with_capacity(out * out * 3);
    for v in 0..out {
        for u in 0..out {
            let (x, y) = crop_to_frame(u as f32, v as f32, rect, out);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = (x - x0, y - y0);
            let (xi, yi) = (x0 as i64, y0 as i64);
            let (p00, p10) = (pixel(img, xi, yi), pixel(img, xi + 1, yi));
            let (p01, p11) = (pixel(img, xi, yi + 1), pixel(img, xi + 1, yi + 1));
            for c in 0..3 {
                let top = p00[c] * (1.0 - fx) + p10[c] * fx;
                let bot = p01[c] * (1.0 - fx) + p11[c] * fx;
                let v = (top * (1.0 - fy) + bot * fy) / 255.0;
                t.push(if signed { v * 2.0 - 1.0 } else { v });
            }
        }
    }
    t
}

/// Square letterbox rect: the whole image centred in a square of the long side.
pub fn letterbox_rect(w: usize, h: usize) -> RotatedRect {
    let s = w.max(h) as f32;
    RotatedRect { cx: w as f32 / 2.0, cy: h as f32 / 2.0, width: s, height: s, angle: 0.0 }
}

/// SSD anchor centres (normalised) for a detector. `strides` lists one entry per layer; layers with
/// equal stride merge into one grid with two anchors per layer per cell (fixed anchor size).
pub fn ssd_anchors(input: usize, strides: &[usize]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < strides.len() {
        let stride = strides[i];
        let mut layers = 0;
        while i + layers < strides.len() && strides[i + layers] == stride {
            layers += 1;
        }
        let fm = input.div_ceil(stride);
        for y in 0..fm {
            for x in 0..fm {
                for _ in 0..layers * 2 {
                    out.push(((x as f32 + 0.5) / fm as f32, (y as f32 + 0.5) / fm as f32));
                }
            }
        }
        i += layers;
    }
    out
}

/// One decoded detection, normalised to the detector's input square.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub score: f32,
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub keypoints: Vec<(f32, f32)>,
}

/// Decodes raw detector outputs: `boxes` is `[n, 4 + 2*kp]` (cx, cy, w, h, keypoints in input
/// pixels, relative to the anchor), `scores` is `[n]` logits. Keeps scores above `min_score`.
pub fn decode_detections(
    boxes: &[f32],
    scores: &[f32],
    anchors: &[(f32, f32)],
    input: usize,
    kp: usize,
    min_score: f32,
) -> Result<Vec<Detection>, String> {
    let stride = 4 + 2 * kp;
    if boxes.len() != anchors.len() * stride || scores.len() != anchors.len() {
        return Err(format!(
            "detector output sizes {} / {} do not match {} anchors",
            boxes.len(),
            scores.len(),
            anchors.len()
        ));
    }
    let n = input as f32;
    let mut out = Vec::new();
    for (i, &(ax, ay)) in anchors.iter().enumerate() {
        let score = sigmoid(scores[i]);
        if score < min_score {
            continue;
        }
        let b = &boxes[i * stride..(i + 1) * stride];
        out.push(Detection {
            score,
            cx: b[0] / n + ax,
            cy: b[1] / n + ay,
            w: b[2] / n,
            h: b[3] / n,
            keypoints: (0..kp).map(|k| (b[4 + 2 * k] / n + ax, b[5 + 2 * k] / n + ay)).collect(),
        });
    }
    Ok(out)
}

fn iou(a: &Detection, b: &Detection) -> f32 {
    let (ax0, ax1, ay0, ay1) = (a.cx - a.w / 2.0, a.cx + a.w / 2.0, a.cy - a.h / 2.0, a.cy + a.h / 2.0);
    let (bx0, bx1, by0, by1) = (b.cx - b.w / 2.0, b.cx + b.w / 2.0, b.cy - b.h / 2.0, b.cy + b.h / 2.0);
    let iw = (ax1.min(bx1) - ax0.max(bx0)).max(0.0);
    let ih = (ay1.min(by1) - ay0.max(by0)).max(0.0);
    let inter = iw * ih;
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Greedy non-maximum suppression, best score first.
pub fn nms(mut dets: Vec<Detection>, iou_max: f32) -> Vec<Detection> {
    dets.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    for d in dets {
        if kept.iter().all(|k| iou(k, &d) <= iou_max) {
            kept.push(d);
        }
    }
    kept
}

/// Rotation that turns the vector `a -> b` (image pixels, y down) to point up.
fn up_angle(a: (f32, f32), b: (f32, f32)) -> f32 {
    (b.0 - a.0).atan2(-(b.1 - a.1))
}

/// Maps a point normalised to the letterbox square into source pixels.
fn from_letterbox(p: (f32, f32), w: usize, h: usize) -> (f32, f32) {
    let s = w.max(h) as f32;
    (p.0 * s - (s - w as f32) / 2.0, p.1 * s - (s - h as f32) / 2.0)
}

/// Hand ROI from a palm detection: rotation from the wrist keypoint to the middle-finger base,
/// box shifted by -0.5 of its height along the fingers and scaled 2.6, squared on the long side
/// (MediaPipe palm_detection_detection_to_roi).
pub fn palm_roi(det: &Detection, w: usize, h: usize) -> Option<RotatedRect> {
    let (k0, k2) = (*det.keypoints.first()?, *det.keypoints.get(2)?);
    let (p0, p2) = (from_letterbox(k0, w, h), from_letterbox(k2, w, h));
    let s = w.max(h) as f32;
    let (bw, bh) = (det.w * s, det.h * s);
    let angle = up_angle(p0, p2);
    let c = from_letterbox((det.cx, det.cy), w, h);
    let shift = -0.5 * bh;
    let (sin, cos) = angle.sin_cos();
    let side = bw.max(bh) * 2.6;
    (side > 1.0).then_some(RotatedRect {
        cx: c.0 - shift * sin,
        cy: c.1 + shift * cos,
        width: side,
        height: side,
        angle,
    })
}

/// Raw hand model result in the model's input pixel space.
#[derive(Debug, Clone, PartialEq)]
pub struct RawHand {
    pub points: [[f32; 3]; HAND_POINTS],
    pub score: f32,
    pub handedness: f32,
}

/// Decodes the 2023 port outputs: landmarks `[63]` in input pixels, then score and handedness.
/// Score and handedness are used as they come out (probabilities); no sigmoid is applied.
pub fn decode_mp2023(lm: &[f32], score: f32, handedness: f32) -> Result<RawHand, String> {
    if lm.len() != HAND_POINTS * 3 {
        return Err(format!("hand landmarks: expected 63 values, got {}", lm.len()));
    }
    let mut points = [[0.0; 3]; HAND_POINTS];
    for (i, p) in points.iter_mut().enumerate() {
        *p = [lm[i * 3], lm[i * 3 + 1], lm[i * 3 + 2]];
    }
    Ok(RawHand { points, score, handedness })
}

/// Maps hand points from model input pixels to source pixels. z scales with the crop.
pub fn hand_to_frame(
    points: &[[f32; 3]; HAND_POINTS],
    rect: &RotatedRect,
    input: usize,
) -> [[f32; 3]; HAND_POINTS] {
    let mut out = [[0.0; 3]; HAND_POINTS];
    for (o, p) in out.iter_mut().zip(points) {
        let (x, y) = crop_to_frame(p[0], p[1], rect, input);
        *o = [x, y, p[2] / input as f32 * rect.width];
    }
    out
}

/// Hand crop from the pose arm points when the palm detector finds nothing: centred past the wrist
/// along the hand direction (index/pinky mid-point, or the forearm when those sit on the wrist).
pub fn hand_rect(
    elbow: &PoseLandmark,
    wrist: &PoseLandmark,
    index: &PoseLandmark,
    pinky: &PoseLandmark,
) -> Option<RotatedRect> {
    let forearm = ((wrist.x - elbow.x).powi(2) + (wrist.y - elbow.y).powi(2)).sqrt();
    let (kx, ky) = ((index.x + pinky.x) / 2.0 - wrist.x, (index.y + pinky.y) / 2.0 - wrist.y);
    let knuckle = (kx * kx + ky * ky).sqrt();
    let (dir, hand_len) = if knuckle >= 0.12 * forearm && knuckle > 2.0 {
        ((kx / knuckle, ky / knuckle), 2.0 * knuckle)
    } else if forearm > 4.0 {
        (((wrist.x - elbow.x) / forearm, (wrist.y - elbow.y) / forearm), 0.7 * forearm)
    } else {
        return None;
    };
    let side = (1.5 * hand_len).max(32.0);
    Some(RotatedRect {
        cx: wrist.x + dir.0 * 0.45 * hand_len,
        cy: wrist.y + dir.1 * 0.45 * hand_len,
        width: side,
        height: side,
        angle: dir.0.atan2(-dir.1),
    })
}

/// A hand landmark stage: finds hands in the image and returns their landmarks, each assigned to
/// the person's left or right arm. The pose gives the wrist positions for that assignment.
pub trait HandStage {
    /// Short label printed next to every hand result, e.g. "PROVISIONAL (2023 port)".
    fn label(&self) -> String;
    fn run(&mut self, img: &Image, pose: &[PoseLandmark; POSE_POINTS]) -> Result<Vec<HandLandmarks>, String>;
}

/// opencv_zoo MediaPipe palm detector plus handpose, 2023-feb. PROVISIONAL: stopgap until a
/// late-2026 model lands.
pub struct Mp2023Hands {
    palm: Session,
    landmarks: Session,
    anchors: Vec<(f32, f32)>,
}

impl Mp2023Hands {
    pub fn new(palm: Session, landmarks: Session) -> Self {
        Self { palm, landmarks, anchors: ssd_anchors(PALM_SIZE, &[8, 16, 16, 16]) }
    }

    fn palm_detections(&mut self, img: &Image) -> Result<Vec<Detection>, String> {
        let tensor = warp_black(img, &letterbox_rect(img.w, img.h), PALM_SIZE, false);
        let n = PALM_SIZE as i64;
        let input = Tensor::<f32>::from_array((vec![1, n, n, 3], tensor)).map_err(ort_err)?;
        let outputs = self.palm.run(ort::inputs![INPUT_NAME => input]).map_err(ort_err)?;
        let get = |name: &str| -> Result<Vec<f32>, String> {
            outputs[name].try_extract_tensor::<f32>().map(|(_, d)| d.to_vec()).map_err(ort_err)
        };
        let (raw_boxes, raw_scores) = (get("Identity")?, get("Identity_1")?);
        if std::env::var_os("HAND_DEBUG").is_some() {
            let max = raw_scores.iter().cloned().fold(f32::MIN, f32::max);
            let min = raw_scores.iter().cloned().fold(f32::MAX, f32::min);
            eprintln!("palm raw scores min {min} max {max}, {} boxes", raw_boxes.len() / 18);
        }
        let dets = decode_detections(&raw_boxes, &raw_scores, &self.anchors, PALM_SIZE, 7, DET_SCORE_MIN)?;
        Ok(nms(dets, NMS_IOU))
    }

    fn landmarks_in(&mut self, img: &Image, rect: &RotatedRect) -> Result<RawHand, String> {
        let tensor = warp_black(img, rect, HAND_LM_SIZE, false);
        let n = HAND_LM_SIZE as i64;
        let input = Tensor::<f32>::from_array((vec![1, n, n, 3], tensor)).map_err(ort_err)?;
        let outputs = self.landmarks.run(ort::inputs![INPUT_NAME => input]).map_err(ort_err)?;
        let get = |name: &str| -> Result<Vec<f32>, String> {
            outputs[name].try_extract_tensor::<f32>().map(|(_, d)| d.to_vec()).map_err(ort_err)
        };
        let lm = get("Identity")?;
        let score = get("Identity_1")?;
        let handed = get("Identity_2")?;
        decode_mp2023(&lm, *score.first().unwrap_or(&0.0), *handed.first().unwrap_or(&0.0))
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Assigns each hand wrist to the nearest free pose wrist (indices 15 left, 16 right), minimising
/// the total distance. Returns the side per hand.
pub fn assign_sides(wrists: &[(f32, f32)], pose: &[PoseLandmark; POSE_POINTS]) -> Vec<SubjectSide> {
    let (l, r) = ((pose[15].x, pose[15].y), (pose[16].x, pose[16].y));
    match wrists {
        [] => vec![],
        [a] => vec![if dist(*a, l) <= dist(*a, r) { SubjectSide::Left } else { SubjectSide::Right }],
        [a, b, ..] => {
            let straight = dist(*a, l) + dist(*b, r);
            let crossed = dist(*a, r) + dist(*b, l);
            if straight <= crossed {
                vec![SubjectSide::Left, SubjectSide::Right]
            } else {
                vec![SubjectSide::Right, SubjectSide::Left]
            }
        }
    }
}

impl HandStage for Mp2023Hands {
    fn label(&self) -> String {
        "PROVISIONAL (2023 port)".to_string()
    }

    fn run(&mut self, img: &Image, pose: &[PoseLandmark; POSE_POINTS]) -> Result<Vec<HandLandmarks>, String> {
        let mut found: Vec<(RawHand, RotatedRect)> = Vec::new();
        for det in self.palm_detections(img)?.iter().take(2) {
            if let Some(rect) = palm_roi(det, img.w, img.h) {
                found.push((self.landmarks_in(img, &rect)?, rect));
            }
        }
        // Palm detector found nothing for an arm whose wrist is visible: try a pose-derived crop.
        if found.is_empty() {
            for ids in [[13, 15, 19, 17], [14, 16, 20, 18]] {
                let [e, wr, ix, pk] = ids.map(|i| pose[i]);
                if wr.visibility < 0.5 {
                    continue;
                }
                if let Some(rect) = hand_rect(&e, &wr, &ix, &pk) {
                    found.push((self.landmarks_in(img, &rect)?, rect));
                }
            }
        }
        let wrists: Vec<(f32, f32)> = found
            .iter()
            .map(|(raw, rect)| {
                let p = hand_to_frame(&raw.points, rect, HAND_LM_SIZE)[0];
                (p[0], p[1])
            })
            .collect();
        let sides = assign_sides(&wrists, pose);
        Ok(found
            .into_iter()
            .zip(sides)
            .map(|((raw, rect), side)| HandLandmarks {
                points: hand_to_frame(&raw.points, &rect, HAND_LM_SIZE),
                score: raw.score,
                handedness: raw.handedness,
                side,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lm(x: f32, y: f32) -> PoseLandmark {
        PoseLandmark { x, y, z: 0.0, visibility: 1.0, world: [0.0; 3] }
    }

    fn det(cx: f32, cy: f32, w: f32, h: f32, score: f32, kps: Vec<(f32, f32)>) -> Detection {
        Detection { score, cx, cy, w, h, keypoints: kps }
    }

    #[test]
    fn anchor_counts_match_the_two_detectors() {
        assert_eq!(ssd_anchors(224, &[8, 16, 32, 32, 32]).len(), 2254);
        assert_eq!(ssd_anchors(192, &[8, 16, 16, 16]).len(), 2016);
        let a = ssd_anchors(224, &[8, 16, 32, 32, 32]);
        assert_eq!(a[0], (0.5 / 28.0, 0.5 / 28.0));
        assert_eq!(a[1], a[0], "two anchors share a cell");
        assert_eq!(a[2].0, 1.5 / 28.0);
    }

    #[test]
    fn decode_detections_adds_anchor_and_filters_by_score() {
        let anchors = vec![(0.25, 0.25), (0.75, 0.75)];
        // 224 input, 1 keypoint: stride 6. Anchor 0 box centre offset +22.4 px = +0.1.
        let mut boxes = vec![0.0f32; 12];
        boxes[..6].copy_from_slice(&[22.4, 0.0, 112.0, 56.0, 0.0, 22.4]);
        let scores = vec![10.0, -10.0];
        let d = decode_detections(&boxes, &scores, &anchors, 224, 1, 0.5).unwrap();
        assert_eq!(d.len(), 1);
        assert!((d[0].cx - 0.35).abs() < 1e-5 && (d[0].cy - 0.25).abs() < 1e-5);
        assert!((d[0].w - 0.5).abs() < 1e-5 && (d[0].h - 0.25).abs() < 1e-5);
        assert!((d[0].keypoints[0].1 - 0.35).abs() < 1e-5);
        assert!(decode_detections(&boxes[..6], &scores, &anchors, 224, 1, 0.5).is_err());
    }

    #[test]
    fn nms_keeps_the_best_of_overlapping_boxes_and_separate_ones() {
        let kept = nms(
            vec![
                det(0.5, 0.5, 0.2, 0.2, 0.7, vec![]),
                det(0.51, 0.5, 0.2, 0.2, 0.9, vec![]),
                det(0.1, 0.1, 0.1, 0.1, 0.6, vec![]),
            ],
            0.3,
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].score, 0.9);
    }

    #[test]
    fn palm_roi_shifts_along_the_fingers_and_scales_by_2_6() {
        // Fingers straight up: wrist keypoint below the middle-finger keypoint.
        let d = det(0.5, 0.5, 0.2, 0.2, 0.9, vec![(0.5, 0.55), (0.0, 0.0), (0.5, 0.4)]);
        let r = palm_roi(&d, 400, 400).unwrap();
        assert!(r.angle.abs() < 1e-5);
        assert!((r.width - 80.0 * 2.6).abs() < 1e-3);
        // Box centre y 200, shifted up by 0.5 * 80 px.
        assert!((r.cx - 200.0).abs() < 1e-3 && (r.cy - 160.0).abs() < 1e-3, "{r:?}");
        // Fingers pointing right: shift goes to the right.
        let d = det(0.5, 0.5, 0.2, 0.2, 0.9, vec![(0.4, 0.5), (0.0, 0.0), (0.6, 0.5)]);
        let r = palm_roi(&d, 400, 400).unwrap();
        assert!(r.cx > 230.0 && (r.cy - 200.0).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn warp_black_pads_outside_with_zeros_and_signs_the_range() {
        let rgb = vec![255u8; 2 * 2 * 3];
        let img = Image { rgb: &rgb, w: 2, h: 2 };
        let inside = RotatedRect { cx: 1.0, cy: 1.0, width: 1.0, height: 1.0, angle: 0.0 };
        let t = warp_black(&img, &inside, 2, false);
        assert!(t.iter().all(|v| (*v - 1.0).abs() < 1e-4), "{t:?}");
        let far = RotatedRect { cx: 50.0, cy: 50.0, ..inside };
        assert!(warp_black(&img, &far, 2, false).iter().all(|v| *v == 0.0));
        assert!(warp_black(&img, &far, 2, true).iter().all(|v| *v == -1.0));
    }

    #[test]
    fn hand_rect_fingers_up_has_zero_angle_and_centre_above_wrist() {
        let r = hand_rect(&lm(100.0, 300.0), &lm(100.0, 200.0), &lm(90.0, 170.0), &lm(110.0, 170.0)).unwrap();
        assert!(r.angle.abs() < 1e-5);
        assert!(r.cy < 200.0 && (r.cx - 100.0).abs() < 1e-3);
        assert!(hand_rect(&lm(5.0, 5.0), &lm(5.0, 5.0), &lm(5.0, 5.0), &lm(5.0, 5.0)).is_none());
    }

    #[test]
    fn hand_points_round_trip_through_a_rotated_crop() {
        let r = RotatedRect { cx: 120.0, cy: 80.0, width: 90.0, height: 90.0, angle: 0.7 };
        let mut pts = [[0.0; 3]; HAND_POINTS];
        pts[0] = [112.0, 112.0, 0.0];
        pts[8] = [0.0, 224.0, 22.4];
        let f = hand_to_frame(&pts, &r, 224);
        assert!((f[0][0] - 120.0).abs() < 1e-3 && (f[0][1] - 80.0).abs() < 1e-3);
        let (u, v) = facelogic::crop::frame_to_crop(f[8][0], f[8][1], &r, 224);
        assert!(u.abs() < 1e-2 && (v - 224.0).abs() < 1e-2);
        assert!((f[8][2] - 9.0).abs() < 1e-4);
    }

    #[test]
    fn decode_mp2023_uses_score_as_probability_and_rejects_bad_length() {
        let lm = vec![1.0; 63];
        let h = decode_mp2023(&lm, 0.99, 0.2).unwrap();
        assert_eq!((h.score, h.handedness), (0.99, 0.2));
        assert!(decode_mp2023(&lm[..60], 0.9, 0.5).is_err());
    }

    #[test]
    fn hands_go_to_the_nearest_pose_wrist_without_swapping_two_hands() {
        let mut pose = [PoseLandmark::default(); POSE_POINTS];
        pose[15] = lm(300.0, 100.0);
        pose[16] = lm(50.0, 100.0);
        assert_eq!(assign_sides(&[(290.0, 90.0)], &pose), vec![SubjectSide::Left]);
        assert_eq!(assign_sides(&[(60.0, 90.0)], &pose), vec![SubjectSide::Right]);
        assert_eq!(
            assign_sides(&[(60.0, 90.0), (290.0, 90.0)], &pose),
            vec![SubjectSide::Right, SubjectSide::Left]
        );
        assert!(assign_sides(&[], &pose).is_empty());
    }
}





