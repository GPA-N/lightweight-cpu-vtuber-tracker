//! Person box for the InstantHMR run without a person detector (OpenSpec task 11.4).
//!
//! Frame 1 (or any frame after the keypoints were lost): the box is derived from the face box.
//! Later frames: the box is expanded from the previous frame's confident arm keypoints, padded,
//! smoothed (grows fast, shrinks slowly) and clamped to the frame. Pure code, no model calls.

/// Number of COCO keypoints the person-box tracker reads (the layout is COCO-17).
pub const KEYPOINTS: usize = 17;
/// COCO-17 indices of the shoulders.
pub const L_SHOULDER: usize = 5;
pub const R_SHOULDER: usize = 6;

/// One keypoint in frame pixels with a confidence score.
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

/// Face-derived box: width = `FACE_WIDTH_FACTOR` x face width, height from the model aspect
/// (256/192), 6.5 is the smallest tested value with margin that keeps both raised arms of hands_17 inside; top `FACE_TOP_FACTOR` face heights above the face top.
pub const FACE_WIDTH_FACTOR: f32 = 6.5;
pub const FACE_TOP_FACTOR: f32 = 0.5;
/// Keypoint score from which a joint counts as confident. Set on the old CIGPose-m scores, which ran 0.49..0.85 for
/// visible joints and 0.12..0.39 for joints outside the frame (docs/notes/person-box.md).
pub const MIN_SCORE: f32 = 0.45;
/// Padding around the confident keypoints, as a fraction of their extent / shoulder width.
pub const KP_PAD: f32 = 0.25;
/// Per-frame smoothing of each box edge: moving outward, moving inward.
pub const ALPHA_OUT: f32 = 0.8;
pub const ALPHA_IN: f32 = 0.2;
/// Frames the last face box is reused while the face tracker has no landmarks (about 1 s at 30 fps).
pub const FACE_HOLD_FRAMES: u32 = 30;
/// Joints used for the box: nose, eyes, ears, shoulders, elbows, wrists (COCO 0..=10).
const UPPER_JOINTS: usize = 11;
const ASPECT_H_OVER_W: f32 = 256.0 / 192.0;

/// Bounding box of the landmark points (frame pixels). `None` when empty.
pub fn face_rect(landmarks: &[[f32; 3]]) -> Option<PersonBox> {
    let first = landmarks.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first[0], first[1], first[0], first[1]);
    for p in landmarks {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    Some(PersonBox { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
}

/// Clamps edges `(x0, y0, x1, y1)` to the frame. `None` when nothing is left.
fn clamped(x0: f32, y0: f32, x1: f32, y1: f32, frame_w: f32, frame_h: f32) -> Option<PersonBox> {
    let (x0, y0) = (x0.max(0.0), y0.max(0.0));
    let (x1, y1) = (x1.min(frame_w), y1.min(frame_h));
    (x1 > x0 && y1 > y0).then_some(PersonBox { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
}

/// Shoulders-and-arms box from the face box, clamped to the frame.
pub fn from_face(face: PersonBox, frame_w: f32, frame_h: f32) -> PersonBox {
    from_face_with(face, frame_w, frame_h, FACE_WIDTH_FACTOR, FACE_TOP_FACTOR)
}

/// [`from_face`] with explicit factors (used to tune them on data).
pub fn from_face_with(face: PersonBox, frame_w: f32, frame_h: f32, width_factor: f32, top_factor: f32) -> PersonBox {
    let w = face.w * width_factor;
    let cx = face.x + face.w / 2.0;
    let top = face.y - face.h * top_factor;
    let (x0, y0, x1, y1) = (cx - w / 2.0, top, cx + w / 2.0, top + w * ASPECT_H_OVER_W);
    // A face outside the frame leaves nothing to clamp to: fall back to the whole frame.
    clamped(x0, y0, x1, y1, frame_w, frame_h)
        .unwrap_or(PersonBox { x: 0.0, y: 0.0, w: frame_w, h: frame_h })
}

/// Box around the confident upper-body keypoints. `None` unless both shoulders are confident.
pub fn from_keypoints(kps: &[Keypoint; KEYPOINTS], frame_w: f32, frame_h: f32) -> Option<PersonBox> {
    if kps[L_SHOULDER].score < MIN_SCORE || kps[R_SHOULDER].score < MIN_SCORE {
        return None;
    }
    let mut it = kps[..UPPER_JOINTS].iter().filter(|k| k.score >= MIN_SCORE);
    let first = it.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for k in it {
        x0 = x0.min(k.x);
        y0 = y0.min(k.y);
        x1 = x1.max(k.x);
        y1 = y1.max(k.y);
    }
    let pad = KP_PAD * (x1 - x0).max(y1 - y0);
    clamped(x0 - pad, y0 - pad, x1 + pad, y1 + pad, frame_w, frame_h)
}

/// Remembers the smoothed box between frames.
#[derive(Debug, Default)]
pub struct PersonBoxTracker {
    prev: Option<PersonBox>,
    /// Last face-derived box and the number of frames since the face was seen.
    face: Option<(PersonBox, u32)>,
}

impl PersonBoxTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Box for the next InstantHMR run: the previous-keypoint box when there is one, else the
    /// face-derived box (held for `FACE_HOLD_FRAMES` frames without a face), else `None`.
    pub fn next(&mut self, face: Option<PersonBox>, frame_w: f32, frame_h: f32) -> Option<PersonBox> {
        let seen = face.map(|f| (from_face(f, frame_w, frame_h), 0));
        self.face = seen.or(self.face.map(|(b, age)| (b, age + 1))).filter(|&(_, age)| age <= FACE_HOLD_FRAMES);
        self.prev.or(self.face.map(|(b, _)| b))
    }

    /// Feeds the keypoints of the frame just run (`None` when the run failed). Too few confident
    /// joints drop the memory, so the next box comes from the face again.
    pub fn update(&mut self, kps: Option<&[Keypoint; KEYPOINTS]>, frame_w: f32, frame_h: f32) {
        let target = kps.and_then(|k| from_keypoints(k, frame_w, frame_h));
        self.prev = match (self.prev, target) {
            (Some(p), Some(t)) => {
                // Per edge: outward moves are fast, inward moves slow.
                let edge = |old: f32, new: f32, outward: bool| {
                    old + (new - old) * if outward { ALPHA_OUT } else { ALPHA_IN }
                };
                let (px1, py1, tx1, ty1) = (p.x + p.w, p.y + p.h, t.x + t.w, t.y + t.h);
                clamped(
                    edge(p.x, t.x, t.x < p.x),
                    edge(p.y, t.y, t.y < p.y),
                    edge(px1, tx1, tx1 > px1),
                    edge(py1, ty1, ty1 > py1),
                    frame_w,
                    frame_h,
                )
            }
            (_, t) => t,
        };
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 640.0;
    const H: f32 = 480.0;

    fn kp(x: f32, y: f32, score: f32) -> Keypoint {
        Keypoint { x, y, score }
    }

    /// All joints low-confidence except the ones given.
    fn kps(good: &[(usize, f32, f32)]) -> [Keypoint; KEYPOINTS] {
        let mut k = [kp(0.0, 0.0, -5.0); KEYPOINTS];
        for &(i, x, y) in good {
            k[i] = kp(x, y, 10.0);
        }
        k
    }

    fn right(b: PersonBox) -> f32 {
        b.x + b.w
    }
    fn bottom(b: PersonBox) -> f32 {
        b.y + b.h
    }

    #[test]
    fn face_rect_is_landmark_extent() {
        let b = face_rect(&[[10.0, 20.0, 0.0], [50.0, 90.0, 0.0], [30.0, 40.0, 0.0]]).unwrap();
        assert_eq!((b.x, b.y, b.w, b.h), (10.0, 20.0, 40.0, 70.0));
        assert!(face_rect(&[]).is_none());
    }

    #[test]
    fn face_box_is_centred_wider_and_reaches_below_the_face() {
        let face = PersonBox { x: 300.0, y: 100.0, w: 40.0, h: 50.0 };
        let b = from_face(face, 10_000.0, 10_000.0);
        assert!((b.x + b.w / 2.0 - 320.0).abs() < 1e-3);
        assert!((b.w - 40.0 * FACE_WIDTH_FACTOR).abs() < 1e-3);
        assert!((b.h - b.w * ASPECT_H_OVER_W).abs() < 1e-3);
        assert!(b.y < face.y && bottom(b) > face.y + face.h * 3.0);
    }

    #[test]
    fn face_box_is_clamped_to_the_frame() {
        let face = PersonBox { x: 500.0, y: 300.0, w: 120.0, h: 150.0 };
        let b = from_face(face, W, H);
        assert!(b.x >= 0.0 && b.y >= 0.0 && right(b) <= W && bottom(b) <= H, "{b:?}");
        assert!(b.w > 0.0 && b.h > 0.0);
    }

    #[test]
    fn keypoint_box_needs_both_shoulders() {
        let only_left = kps(&[(L_SHOULDER, 200.0, 300.0), (7, 150.0, 400.0)]);
        assert!(from_keypoints(&only_left, W, H).is_none());
    }

    #[test]
    fn keypoint_box_contains_confident_joints_with_padding_and_ignores_weak_ones() {
        let mut k = kps(&[(L_SHOULDER, 400.0, 300.0), (R_SHOULDER, 200.0, 300.0), (9, 100.0, 250.0)]);
        k[8] = kp(5.0, 5.0, MIN_SCORE - 0.5); // weak elbow far away must not stretch the box
        let b = from_keypoints(&k, W, H).unwrap();
        assert!(b.x < 100.0 && right(b) > 400.0 && b.y < 250.0 && bottom(b) > 300.0, "{b:?}");
        assert!(b.x > 5.0, "weak joint leaked into the box: {b:?}");
    }

    #[test]
    fn keypoint_box_is_clamped() {
        let k = kps(&[(L_SHOULDER, 630.0, 470.0), (R_SHOULDER, 400.0, 470.0)]);
        let b = from_keypoints(&k, W, H).unwrap();
        assert!(right(b) <= W && bottom(b) <= H && b.x >= 0.0 && b.y >= 0.0);
    }

    #[test]
    fn tracker_uses_face_first_then_keypoints_then_face_again() {
        let face = PersonBox { x: 300.0, y: 100.0, w: 40.0, h: 50.0 };
        let mut t = PersonBoxTracker::new();
        assert!(t.next(None, W, H).is_none());
        let first = t.next(Some(face), W, H).unwrap();
        assert_eq!(first, from_face(face, W, H));

        let k = kps(&[(L_SHOULDER, 400.0, 300.0), (R_SHOULDER, 250.0, 300.0)]);
        t.update(Some(&k), W, H);
        let second = t.next(Some(face), W, H).unwrap();
        assert_ne!(second, first);
        assert_eq!(second, from_keypoints(&k, W, H).unwrap(), "first keypoint box is taken as is");

        t.update(None, W, H); // run failed or joints lost
        assert_eq!(t.next(Some(face), W, H).unwrap(), first);
        assert_eq!(t.next(None, W, H).unwrap(), first, "face lost for a frame: hold the last face box");
    }

    #[test]
    fn tracker_drops_the_held_face_box_after_the_hold_limit() {
        let face = PersonBox { x: 300.0, y: 100.0, w: 40.0, h: 50.0 };
        let mut t = PersonBoxTracker::new();
        assert!(t.next(Some(face), W, H).is_some());
        for _ in 0..FACE_HOLD_FRAMES {
            assert!(t.next(None, W, H).is_some());
        }
        assert!(t.next(None, W, H).is_none(), "face gone too long: nothing to run on");
        assert!(t.next(Some(face), W, H).is_some(), "face back: box back");
    }

    #[test]
    fn tracker_without_face_keeps_following_keypoints() {
        let mut t = PersonBoxTracker::new();
        let k = kps(&[(L_SHOULDER, 400.0, 300.0), (R_SHOULDER, 250.0, 300.0)]);
        t.update(Some(&k), W, H);
        assert!(t.next(None, W, H).is_some());
    }

    #[test]
    fn tracker_grows_fast_and_shrinks_slowly() {
        let mut t = PersonBoxTracker::new();
        let small = kps(&[(L_SHOULDER, 340.0, 300.0), (R_SHOULDER, 300.0, 300.0)]);
        let wide = kps(&[(L_SHOULDER, 500.0, 300.0), (R_SHOULDER, 100.0, 300.0)]);
        t.update(Some(&small), W, H);
        let s = t.next(None, W, H).unwrap();
        let target = from_keypoints(&wide, W, H).unwrap();
        t.update(Some(&wide), W, H);
        let grown = t.next(None, W, H).unwrap();
        // outward: most of the way to the target in one frame
        assert!(grown.x < s.x && (s.x - grown.x) > 0.6 * (s.x - target.x), "{s:?} {grown:?} {target:?}");
        // inward: only a small step
        t.update(Some(&small), W, H);
        let shrunk = t.next(None, W, H).unwrap();
        assert!(shrunk.x > grown.x && (shrunk.x - grown.x) < 0.4 * (s.x - grown.x), "{grown:?} {shrunk:?}");
    }
}
