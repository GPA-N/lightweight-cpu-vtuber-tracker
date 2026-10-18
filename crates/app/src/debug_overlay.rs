//! Debug drawing on the live camera preview: stick lines and pointers for the arms, hands, torso,
//! head and eyes, so tracking and detection problems can be seen at a glance.
//! Colours: person's left arm cyan, right arm orange, shoulders and neck yellow, head magenta,
//! fingers lime, eye outlines blue, irises red with a red gaze pointer. Joints below the visibility
//! or score threshold are not drawn, so a missing limb shows up as a missing line.

use core::body::{HandLandmarks, SubjectSide, UpperBodyOutput};
use infer::cigpose::Keypoint;

pub type Rgb = [u8; 3];
const CYAN: Rgb = [0, 220, 255];
const ORANGE: Rgb = [255, 150, 0];
const YELLOW: Rgb = [255, 230, 0];
const MAGENTA: Rgb = [255, 0, 255];
const LIME: Rgb = [150, 255, 0];
const BLUE: Rgb = [60, 120, 255];
const RED: Rgb = [255, 40, 40];

/// Shapes in source-frame pixels (not mirrored).
#[derive(Debug, Clone, Default)]
pub struct Overlay {
    pub lines: Vec<([f32; 2], [f32; 2], Rgb)>,
    /// Point, colour, radius in pixels.
    pub points: Vec<([f32; 2], Rgb, i32)>,
}

impl Overlay {
    fn line(&mut self, a: [f32; 2], b: [f32; 2], c: Rgb) {
        self.lines.push((a, b, c));
    }
    fn dot(&mut self, p: [f32; 2], c: Rgb, r: i32) {
        self.points.push((p, c, r));
    }
}

/// Finger and palm edges of the 21-point hand model.
const HAND_EDGES: [(usize, usize); 20] = [
    (0, 1), (1, 2), (2, 3), (3, 4),
    (0, 5), (5, 6), (6, 7), (7, 8),
    (9, 10), (10, 11), (11, 12),
    (13, 14), (14, 15), (15, 16),
    (17, 18), (18, 19), (19, 20),
    (5, 9), (9, 13), (13, 17),
];

fn hands(o: &mut Overlay, hands: &[HandLandmarks], min_score: f32) {
    for h in hands.iter().filter(|h| h.score >= min_score) {
        let tone = if h.side == SubjectSide::Left { CYAN } else { ORANGE };
        for (a, b) in HAND_EDGES {
            o.line([h.points[a][0], h.points[a][1]], [h.points[b][0], h.points[b][1]], LIME);
        }
        o.line([h.points[0][0], h.points[0][1]], [h.points[17][0], h.points[17][1]], LIME);
        o.dot([h.points[0][0], h.points[0][1]], tone, 3);
        for i in [4, 8, 12, 16, 20] {
            o.dot([h.points[i][0], h.points[i][1]], LIME, 2);
        }
    }
}

fn mid(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// BlazePose result: shoulders, arms, wrists to index and pinky, ears and the neck line, plus hands.
pub fn from_upper_body(out: &UpperBodyOutput, vis_min: f32, hand_min: f32) -> Overlay {
    let mut o = Overlay::default();
    let p = |i: usize| {
        let l = out.pose[i];
        (l.visibility >= vis_min).then_some([l.x, l.y])
    };
    let seg = |o: &mut Overlay, a: usize, b: usize, c: Rgb| {
        if let (Some(pa), Some(pb)) = (p(a), p(b)) {
            o.line(pa, pb, c);
        }
    };
    seg(&mut o, 11, 12, YELLOW);
    seg(&mut o, 11, 13, CYAN);
    seg(&mut o, 13, 15, CYAN);
    seg(&mut o, 15, 19, CYAN);
    seg(&mut o, 15, 17, CYAN);
    seg(&mut o, 12, 14, ORANGE);
    seg(&mut o, 14, 16, ORANGE);
    seg(&mut o, 16, 20, ORANGE);
    seg(&mut o, 16, 18, ORANGE);
    seg(&mut o, 7, 8, MAGENTA);
    if let (Some(l), Some(r), Some(nose)) = (p(11), p(12), p(0)) {
        o.line(mid(l, r), nose, YELLOW);
    }
    for (i, c) in [(11, CYAN), (13, CYAN), (15, CYAN), (12, ORANGE), (14, ORANGE), (16, ORANGE), (0, MAGENTA)] {
        if let Some(pt) = p(i) {
            o.dot(pt, c, 3);
        }
    }
    hands(&mut o, &out.hands, hand_min);
    o
}

/// CIGPose-m COCO keypoints (score threshold) plus the hand model results.
pub fn from_coco(kps: &[Keypoint; 17], min_score: f32, found: &[HandLandmarks], hand_min: f32) -> Overlay {
    let mut o = Overlay::default();
    let p = |i: usize| (kps[i].score >= min_score).then_some([kps[i].x, kps[i].y]);
    let seg = |o: &mut Overlay, a: usize, b: usize, c: Rgb| {
        if let (Some(pa), Some(pb)) = (p(a), p(b)) {
            o.line(pa, pb, c);
        }
    };
    seg(&mut o, 5, 6, YELLOW);
    seg(&mut o, 5, 7, CYAN);
    seg(&mut o, 7, 9, CYAN);
    seg(&mut o, 6, 8, ORANGE);
    seg(&mut o, 8, 10, ORANGE);
    seg(&mut o, 3, 4, MAGENTA);
    if let (Some(l), Some(r), Some(nose)) = (p(5), p(6), p(0)) {
        o.line(mid(l, r), nose, YELLOW);
    }
    for (i, c) in [(5, CYAN), (7, CYAN), (9, CYAN), (6, ORANGE), (8, ORANGE), (10, ORANGE), (0, MAGENTA)] {
        if let Some(pt) = p(i) {
            o.dot(pt, c, 3);
        }
    }
    hands(&mut o, found, hand_min);
    o
}

/// Eye outlines, irises and gaze pointers from the 478 face landmarks (frame pixels).
pub fn eyes(landmarks: &[[f32; 3]]) -> Overlay {
    let mut o = Overlay::default();
    if landmarks.len() < 478 {
        return o;
    }
    // Contours of the subject's right and left eye, then (outer corner, inner corner, iris centre).
    const RIGHT: [usize; 16] = [33, 246, 161, 160, 159, 158, 157, 173, 133, 155, 154, 153, 145, 144, 163, 7];
    const LEFT: [usize; 16] = [263, 466, 388, 387, 386, 385, 384, 398, 362, 382, 381, 380, 374, 373, 390, 249];
    let pt = |i: usize| [landmarks[i][0], landmarks[i][1]];
    for (ring, corners, iris) in [(RIGHT, (33, 133), 468), (LEFT, (263, 362), 473)] {
        for k in 0..ring.len() {
            o.line(pt(ring[k]), pt(ring[(k + 1) % ring.len()]), BLUE);
        }
        let centre = mid(pt(corners.0), pt(corners.1));
        let iris_c = pt(iris);
        o.dot(iris_c, RED, 2);
        // Gaze pointer: the iris offset from the eye centre, stretched so small movements show.
        let tip = [centre[0] + (iris_c[0] - centre[0]) * 5.0, centre[1] + (iris_c[1] - centre[1]) * 5.0];
        o.line(centre, tip, RED);
    }
    o
}

/// Draws `overlay` onto a mirrored RGBA preview of size `w` x `h` (x is flipped to match).
pub fn draw(rgba: &mut [u8], w: usize, h: usize, overlay: &Overlay) {
    let mut put = |x: i32, y: i32, c: Rgb| {
        let x = w as i32 - 1 - x;
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let i = (y as usize * w + x as usize) * 4;
            rgba[i..i + 3].copy_from_slice(&c);
        }
    };
    for (a, b, c) in &overlay.lines {
        let (mut x0, mut y0) = (a[0].round() as i32, a[1].round() as i32);
        let (x1, y1) = (b[0].round() as i32, b[1].round() as i32);
        if (x0.abs() > 20000) || (y0.abs() > 20000) || (x1.abs() > 20000) || (y1.abs() > 20000) {
            continue;
        }
        // Bresenham.
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let mut err = dx + dy;
        loop {
            put(x0, y0, *c);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
    for (p, c, r) in &overlay.points {
        let (cx, cy) = (p[0].round() as i32, p[1].round() as i32);
        for dy in -r..=*r {
            for dx in -r..=*r {
                if dx * dx + dy * dy <= r * r {
                    put(cx + dx, cy + dy, *c);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_dot_land_on_the_mirrored_pixels() {
        let (w, h) = (20usize, 10usize);
        let mut rgba = vec![0u8; w * h * 4];
        let mut o = Overlay::default();
        o.line([2.0, 5.0], [6.0, 5.0], CYAN);
        o.dot([10.0, 2.0], RED, 1);
        draw(&mut rgba, w, h, &o);
        let px = |x: usize, y: usize| rgba[(y * w + x) * 4..(y * w + x) * 4 + 3].to_vec();
        // Source x 2..=6 becomes preview x 17..=13.
        for sx in 2..=6 {
            assert_eq!(px(w - 1 - sx, 5), CYAN.to_vec(), "line pixel {sx}");
        }
        assert_eq!(px(w - 1 - 10, 2), RED.to_vec());
        assert_eq!(px(w - 1 - 3, 5), CYAN.to_vec());
        assert_eq!(px(0, 0), vec![0, 0, 0]);
    }

    #[test]
    fn eyes_overlay_needs_all_478_landmarks_and_points_the_gaze_pointer_at_the_iris_side() {
        assert!(eyes(&vec![[0.0; 3]; 400]).lines.is_empty());
        let mut lm = vec![[0.0f32; 3]; 478];
        lm[33] = [100.0, 50.0, 0.0];
        lm[133] = [120.0, 50.0, 0.0];
        lm[468] = [114.0, 50.0, 0.0]; // iris right of the centre (110)
        let o = eyes(&lm);
        let tip = o.lines.iter().find(|l| l.2 == RED && l.0 == [110.0, 50.0]).expect("gaze line").1;
        assert!(tip[0] > 129.9, "pointer tip {tip:?}");
    }

    #[test]
    fn low_visibility_joints_leave_no_lines() {
        use core::body::{PoseLandmark, POSE_POINTS};
        let mut pose = [PoseLandmark::default(); POSE_POINTS];
        for i in [11, 12, 13] {
            pose[i] = PoseLandmark { x: i as f32 * 10.0, y: 20.0, z: 0.0, visibility: 0.9, world: [0.0; 3] };
        }
        let out = UpperBodyOutput { width: 100, height: 100, pose, presence: 1.0, hands: vec![], hand_model: String::new() };
        let o = from_upper_body(&out, 0.5, 0.5);
        // Shoulder line and left upper arm only (the wrist is not visible).
        assert_eq!(o.lines.len(), 2, "{:?}", o.lines);
    }
}
