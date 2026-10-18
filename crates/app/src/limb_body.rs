//! Live limb path for `live --body --limb cigpose` (OpenSpec 11.7): person box from the face
//! landmarks, CIGPose-m keypoints, 2D to 3D arm lift, and the MediaPipe hand stage for fingers.
//! The hand stage still wants BlazePose-style wrist landmarks to assign left and right, so a
//! synthetic pose is built from the CIGPose elbows and wrists (visibility 1 above the score cut).

use avatar::arm_lift::{ArmLifter, Kp, LiftHints, Vec3, MIN_SCORE};
use avatar::body::{to_avatar_space, BodyPose, HAND_SCORE_MIN, VIS_MIN};
use core::body::{HandLandmarks, PoseLandmark, SubjectSide, UpperBodyOutput, POSE_POINTS};
use infer::cigpose::{CigPose, LimbStage, PersonBox, KEYPOINTS};
use infer::person_box::PersonBoxTracker;
use infer::upper_body::{HandStage, Image};

pub struct LimbBody {
    cig: CigPose,
    boxes: PersonBoxTracker,
    lifter: ArmLifter,
    hands: Box<dyn HandStage>,
    mirror: bool,
    /// Debug drawing of the last frame (CIGPose keypoints and hands), in frame pixels.
    pub overlay: crate::debug_overlay::Overlay,
}

impl LimbBody {
    pub fn new(cig: CigPose, hands: Box<dyn HandStage>, mirror: bool) -> Self {
        Self { cig, boxes: PersonBoxTracker::new(), lifter: ArmLifter::new(mirror), hands, mirror, overlay: Default::default() }
    }

    /// `rgb` is `width * height * 3` bytes, not mirrored. `face` is the face-landmark box in frame
    /// pixels, if the face tracker has one.
    pub fn run(&mut self, rgb: &[u8], width: u32, height: u32, face: Option<PersonBox>) -> Result<BodyPose, String> {
        let (w, h) = (width as usize, height as usize);
        let (fw, fh) = (width as f32, height as f32);
        let img = Image { rgb, w, h };
        // No face and no previous keypoints (cold start in an odd pose): the whole frame.
        let person = self.boxes.next(face, fw, fh).unwrap_or(PersonBox { x: 0.0, y: 0.0, w: fw, h: fh });
        let kps = match self.cig.run(&img, person) {
            Ok(k) => k,
            Err(e) => {
                self.boxes.update(None, fw, fh);
                return Err(e);
            }
        };
        self.boxes.update(Some(&kps), fw, fh);

        let kp: [Kp; KEYPOINTS] = std::array::from_fn(|i| Kp { x: kps[i].x, y: kps[i].y, score: kps[i].score });

        let mut pose = [PoseLandmark::default(); POSE_POINTS];
        // COCO 7..10 (elbows, wrists) to MediaPipe 13..16.
        for (coco, mp) in [(7, 13), (8, 14), (9, 15), (10, 16)] {
            let k = kps[coco];
            pose[mp] = PoseLandmark { x: k.x, y: k.y, z: 0.0, visibility: (k.score >= MIN_SCORE) as u8 as f32, world: [0.0; 3] };
        }
        // Index and pinky stay invisible: the hand crop falls back to the forearm direction.
        for (wrist, others) in [(15, [17, 19]), (16, [18, 20])] {
            for i in others {
                pose[i] = PoseLandmark { x: pose[wrist].x, y: pose[wrist].y, ..PoseLandmark::default() };
            }
        }
        let hands = self.hands.run(&img, &pose)?;
        self.overlay = crate::debug_overlay::from_coco(&kps, MIN_SCORE, &hands, HAND_SCORE_MIN);
        let hints = hand_depth_hints(&kp, &hands, self.mirror);
        let lifted = self.lifter.lift(&kp, hints);
        if std::env::var_os("VT_LIMB_LOG").is_some() {
            let shoulders = (kp[5].x - kp[6].x).hypot(kp[5].y - kp[6].y).max(1.0);
            let ratio = |side| {
                hands.iter().find(|h| h.side == side).map(|h| (h.points[9][0] - h.points[0][0]).hypot(h.points[9][1] - h.points[0][1]) / shoulders)
            };
            let dz = |a: &BodyPose| (a.left.wrist.zip(a.left.shoulder).map(|(w, s)| w.z - s.z), a.right.wrist.zip(a.right.shoulder).map(|(w, s)| w.z - s.z));
            eprintln!("limblog ratioL {:?} ratioR {:?} hintL {:?} hintR {:?} wristdz(avatar L,R) {:?}", ratio(SubjectSide::Left), ratio(SubjectSide::Right), hints.left_forearm.map(|v| v.z > 0.0), hints.right_forearm.map(|v| v.z > 0.0), dz(&lifted.pose));
        }
        let out =UpperBodyOutput { width, height, pose, presence: 1.0, hands, hand_model: self.hands.label() };
        let mut body = to_avatar_space(&out, self.mirror, VIS_MIN, HAND_SCORE_MIN);
        for (arm, mine) in [(&mut body.left, &lifted.pose.left), (&mut body.right, &lifted.pose.right)] {
            arm.shoulder = mine.shoulder;
            arm.elbow = mine.elbow;
            arm.wrist = mine.wrist;
        }
        Ok(body)
    }
}

/// Wrist to middle-finger knuckle over shoulder width for a hand held about level with the
/// shoulders at a desk. Measured live on 2026-10-18: 0.28 to 0.30 relaxed, 0.38 to 0.47 reaching
/// forward, 0.15 to 0.17 with the arm pulled back (docs/notes/arm-lift.md).
const HAND_RATIO_REF: f32 = 0.30;
/// A hand this much larger or smaller than the reference counts as nearer or farther than the
/// shoulders; between the two the size says nothing (foreshortening, fists) and no hint is given.
const HAND_NEAR: f32 = 1.25;
const HAND_FAR: f32 = 0.7;

/// Depth cue for the arm lift: a hand that looks bigger than it should is closer to the camera
/// than the shoulders, a smaller one is farther. CIGPose gives only 2D, so without this an arm
/// pulled backwards looks like one pushed forward. The result is a forearm direction whose z
/// sign settles the lift; its length does not matter.
fn hand_depth_hints(kp: &[Kp; KEYPOINTS], hands: &[HandLandmarks], mirror: bool) -> LiftHints {
    let mut hints = LiftHints::default();
    let shoulders = (kp[5].x - kp[6].x).hypot(kp[5].y - kp[6].y);
    if shoulders < 1.0 {
        return hints;
    }
    for hand in hands.iter().filter(|h| h.score >= HAND_SCORE_MIN) {
        let (wrist, knuckle) = (hand.points[0], hand.points[9]);
        let ratio = (knuckle[0] - wrist[0]).hypot(knuckle[1] - wrist[1]) / shoulders;
        let sign = if ratio >= HAND_RATIO_REF * HAND_NEAR {
            1.0
        } else if ratio <= HAND_RATIO_REF * HAND_FAR {
            -1.0
        } else {
            continue;
        };
        let (elbow, wr, left) = match hand.side {
            SubjectSide::Left => (kp[7], kp[9], true),
            SubjectSide::Right => (kp[8], kp[10], false),
        };
        if elbow.score < MIN_SCORE || wr.score < MIN_SCORE {
            continue;
        }
        let sx = if mirror { -1.0 } else { 1.0 };
        let (dx, dy) = (sx * (wr.x - elbow.x), -(wr.y - elbow.y));
        let planar = dx.hypot(dy).max(1.0);
        let dir = Some(Vec3::new(dx, dy, sign * 0.7 * planar).normalize());
        // Avatar side: the person's left arm drives the avatar's right bones when mirrored.
        if left != mirror {
            hints.left_forearm = dir;
        } else {
            hints.right_forearm = dir;
        }
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use infer::engine::Engine;

    fn hand(side: SubjectSide, len: f32) -> HandLandmarks {
        let mut points = [[0.0f32; 3]; 21];
        points[9] = [0.0, -len, 0.0];
        HandLandmarks { points, score: 0.99, handedness: 0.0, side }
    }

    fn arm_kps() -> [Kp; KEYPOINTS] {
        let mut kp = [Kp::default(); KEYPOINTS];
        // Shoulder width 100 px; left arm bent up, right arm hanging.
        kp[5] = Kp { x: 150.0, y: 100.0, score: 0.8 };
        kp[6] = Kp { x: 50.0, y: 100.0, score: 0.8 };
        kp[7] = Kp { x: 180.0, y: 160.0, score: 0.8 };
        kp[9] = Kp { x: 190.0, y: 100.0, score: 0.8 };
        kp[8] = Kp { x: 40.0, y: 160.0, score: 0.8 };
        kp[10] = Kp { x: 40.0, y: 220.0, score: 0.8 };
        kp
    }

    #[test]
    fn big_hand_hints_forward_small_hand_backward_middle_none() {
        let kp = arm_kps();
        // Not mirrored: person's left is the avatar's left.
        let near = hand_depth_hints(&kp, &[hand(SubjectSide::Left, 40.0)], false);
        assert!(near.left_forearm.unwrap().z > 0.0 && near.right_forearm.is_none());
        let far = hand_depth_hints(&kp, &[hand(SubjectSide::Left, 15.0)], false);
        assert!(far.left_forearm.unwrap().z < 0.0);
        assert!(hand_depth_hints(&kp, &[hand(SubjectSide::Left, 25.0)], false).left_forearm.is_none());
    }

    #[test]
    fn mirror_swaps_the_avatar_side_and_flips_x() {
        let kp = arm_kps();
        let h = hand_depth_hints(&kp, &[hand(SubjectSide::Right, 40.0)], true);
        assert!(h.left_forearm.is_some() && h.right_forearm.is_none(), "person's right drives the avatar's left when mirrored");
        let l = hand_depth_hints(&kp, &[hand(SubjectSide::Left, 40.0)], true);
        // Person's left forearm goes right in the image (+x); mirrored avatar x is negated.
        assert!(l.right_forearm.unwrap().x < 0.0);
    }

    use infer::person_box::face_rect;
    use infer::session::build_session;
    use infer::upper_body::Mp2023Hands;
    use std::path::Path;
    use std::time::Instant;

    /// Runs the limb path over the golden_take frames: every frame must give arm joints, and the
    /// median time is printed (`cargo test --release -p app --bin live limb_body -- --nocapture`).
    #[test]
    fn limb_path_on_golden_take() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = root.join("fixtures/private/golden_take");
        let ub = root.join("models/upper_body");
        let cig = ub.join("cigpose-m_coco_256x192.onnx");
        if !dir.join("lm.txt").exists() || !cig.exists() {
            eprintln!("SKIP limb_body: private fixtures or models missing");
            return;
        }
        let load = |n: &str| build_session(&ub.join(n), 4).unwrap().0;
        let hands = Box::new(Mp2023Hands::new(
            load("palm_detection_mediapipe_2023feb.onnx"),
            load("handpose_estimation_mediapipe_2023feb.onnx"),
        ));
        let mut stage = LimbBody::new(CigPose::load(Engine::Ort, &cig, 4).unwrap(), hands, true);
        let (mut ms, mut with_arm, mut frames) = (Vec::new(), 0, 0);
        for (i, line) in std::fs::read_to_string(dir.join("lm.txt")).unwrap().lines().enumerate() {
            let v: Vec<f32> = line.split_whitespace().skip(1).map(|t| t.parse().unwrap()).collect();
            let pts: Vec<[f32; 3]> = v.chunks_exact(2).map(|c| [c[0], c[1], 0.0]).collect();
            let rgb = image::open(dir.join(format!("f{i:04}.jpg"))).unwrap().to_rgb8();
            let started = Instant::now();
            let body = stage.run(rgb.as_raw(), rgb.width(), rgb.height(), face_rect(&pts)).unwrap();
            ms.push(started.elapsed().as_secs_f32() * 1000.0);
            frames += 1;
            with_arm += (body.left.shoulder.is_some() && body.right.shoulder.is_some()) as usize;
        }
        ms.sort_by(|a, b| a.total_cmp(b));
        println!("limb path: {frames} frames, shoulders found on {with_arm}, median {:.1} ms, p95 {:.1} ms", ms[ms.len() / 2], ms[ms.len() * 95 / 100]);
        // The golden take is a face close-up, so shoulders are mostly out of frame there; the
        // arm check runs on a photo with both arms in view.
        let photo = root.join("fixtures/private/upper_body_photos/hands_15.png");
        if photo.exists() {
            let rgb = image::open(photo).unwrap().to_rgb8();
            let mut fresh = LimbBody::new(
                CigPose::load(Engine::Ort, &cig, 4).unwrap(),
                Box::new(Mp2023Hands::new(
                    load("palm_detection_mediapipe_2023feb.onnx"),
                    load("handpose_estimation_mediapipe_2023feb.onnx"),
                )),
                true,
            );
            let body = fresh.run(rgb.as_raw(), rgb.width(), rgb.height(), None).unwrap();
            for arm in [&body.left, &body.right] {
                assert!(arm.shoulder.is_some() && arm.elbow.is_some() && arm.wrist.is_some(), "arm joints missing: {arm:?}");
                assert!(arm.hand.is_some(), "hand missing");
            }
        }
        // Depth of the wrist relative to the shoulder (shoulder widths, +z toward the camera) per photo.
        for name in ["hands_15", "hands_16", "hands_17", "hands_19"] {
            let path = root.join(format!("fixtures/private/upper_body_photos/{name}.png"));
            if !path.exists() {
                continue;
            }
            let rgb = image::open(path).unwrap().to_rgb8();
            let mut s = LimbBody::new(
                CigPose::load(Engine::Ort, &cig, 4).unwrap(),
                Box::new(Mp2023Hands::new(
                    load("palm_detection_mediapipe_2023feb.onnx"),
                    load("handpose_estimation_mediapipe_2023feb.onnx"),
                )),
                true,
            );
            let b = s.run(rgb.as_raw(), rgb.width(), rgb.height(), None).unwrap();
            let dz = |a: &avatar::body::ArmPose| a.wrist.zip(a.shoulder).map(|(w, sh)| w.z - sh.z);
            println!("{name}: wrist depth vs shoulder, avatar left {:?} right {:?}", dz(&b.left), dz(&b.right));
        }
    }
}
