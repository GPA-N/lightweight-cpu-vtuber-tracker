//! Debug drawing on the live camera preview: stick lines and pointers for the arms, hands, torso,
//! head and eyes, so tracking and detection problems can be seen at a glance.
//! Colours: person's left arm cyan, right arm orange, shoulders and neck yellow, head magenta,
//! fingers lime, eye outlines blue, irises red with a red gaze pointer. Joints below the visibility
//! or score threshold are not drawn, so a missing limb shows up as a missing line.

use core::body::{HandLandmarks, SubjectSide, UpperBodyOutput};

pub type Rgb = [u8; 3];
const CYAN: Rgb = [0, 220, 255];
const ORANGE: Rgb = [255, 150, 0];
const YELLOW: Rgb = [255, 230, 0];
const MAGENTA: Rgb = [255, 0, 255];
const LIME: Rgb = [150, 255, 0];
const BLUE: Rgb = [60, 120, 255];
const RED: Rgb = [255, 40, 40];
const WHITE: Rgb = [255, 255, 255];

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
    /// The person box the body model was run on (x, y, width, height in frame pixels), as a white rectangle.
    pub fn person_box(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (x1, y1) = (x + w, y + h);
        for (a, b) in [([x, y], [x1, y]), ([x1, y], [x1, y1]), ([x1, y1], [x, y1]), ([x, y1], [x, y])] {
            self.line(a, b, WHITE);
        }
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

/// Pose result in the 33-point pose layout: shoulders, arms, wrists to index and pinky, ears and the neck line, plus hands.
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

/// Seven coarse face points (forehead, nose, chin, mouth corners, eye centres) from the 478 face
/// landmarks, for recordings where the full mesh or the eye outlines would identify the person.
pub fn face_dots(landmarks: &[[f32; 3]]) -> Overlay {
    let mut o = Overlay::default();
    if landmarks.len() < 478 {
        return o;
    }
    let pt = |i: usize| [landmarks[i][0], landmarks[i][1]];
    for i in [10, 1, 152, 61, 291] {
        o.dot(pt(i), MAGENTA, 4);
    }
    for (outer, inner) in [(33, 133), (263, 362)] {
        o.dot(mid(pt(outer), pt(inner)), BLUE, 4);
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
    fn face_dots_give_seven_points_and_no_lines_from_a_full_landmark_set() {
        assert!(face_dots(&vec![[0.0; 3]; 400]).points.is_empty());
        let o = face_dots(&vec![[5.0; 3]; 478]);
        assert_eq!(o.points.len(), 7);
        assert!(o.lines.is_empty());
    }

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

    /// Full-body overlay as the live app draws it: upper body (about 13 joints and their lines),
    /// two hands (21 points, 20 edges each) and both eyes from 478 face landmarks.
    fn typical_overlay() -> Overlay {
        use core::body::{HandLandmarks, PoseLandmark, POSE_POINTS, HAND_POINTS};
        let mut pose = [PoseLandmark::default(); POSE_POINTS];
        for (i, l) in pose.iter_mut().enumerate() {
            *l = PoseLandmark { x: 300.0 + (i as f32 * 37.0) % 600.0, y: 150.0 + (i as f32 * 53.0) % 400.0, z: 0.0, visibility: 0.9, world: [0.0; 3] };
        }
        let hands = [SubjectSide::Left, SubjectSide::Right].map(|side| {
            let mut points = [[0.0f32; 3]; HAND_POINTS];
            for (j, p) in points.iter_mut().enumerate() {
                *p = [600.0 + (j % 5) as f32 * 25.0, 400.0 + (j / 5) as f32 * 25.0 + j as f32, 0.0];
            }
            HandLandmarks { points, score: 0.9, handedness: 0.5, side }
        });
        let out = UpperBodyOutput { width: 1280, height: 720, pose, presence: 1.0, hands: hands.to_vec(), hand_model: String::new() };
        let mut o = from_upper_body(&out, 0.5, 0.5);
        let lm: Vec<[f32; 3]> = (0..478).map(|i| [500.0 + (i % 20) as f32 * 3.0, 250.0 + (i / 20) as f32 * 2.0, 0.0]).collect();
        let e = eyes(&lm);
        o.lines.extend(e.lines);
        o.points.extend(e.points);
        o
    }

    /// Median time of `draw` for a typical overlay on 1280x720 RGBA. Run with `--ignored --nocapture` in release.
    #[test]
    #[ignore]
    fn overlay_draw_bench() {
        let (w, h) = (1280usize, 720usize);
        let o = typical_overlay();
        let mut rgba = vec![0u8; w * h * 4];
        let mut times = Vec::new();
        for _ in 0..50 {
            let started = std::time::Instant::now();
            draw(&mut rgba, w, h, &o);
            times.push(started.elapsed().as_secs_f64() * 1000.0);
            std::hint::black_box(&rgba);
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = (times[24] + times[25]) / 2.0;
        println!("overlay draw median {median:.3} ms over 50 draws ({} lines, {} dots), 1280x720", o.lines.len(), o.points.len());
    }
}
