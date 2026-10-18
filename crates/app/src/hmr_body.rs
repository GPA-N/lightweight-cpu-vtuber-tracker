//! Live body path for `live --body` (default): person box from the face landmarks (then from the
//! previous frame's joints), InstantHMR for 3D shoulders, elbows, wrists and ears, and the MediaPipe
//! hand stage for fingers.
//!
//! InstantHMR returns metric 3D joints (x right, y down, z away, the same frame as BlazePose's
//! world landmarks), so they go straight into the pose structure that `to_avatar_space` reads and no
//! 2D-to-3D lift is needed. The model gives no per-joint confidence, so every joint counts as seen;
//! the "inside the picture" check in `to_avatar_space` still drops joints that fall outside the frame.

use avatar::Vec3;
use avatar::body::{to_avatar_space, BodyPose};
use core::body::{HandLandmarks, PoseLandmark, SubjectSide, UpperBodyOutput, POSE_POINTS};
use infer::instant_hmr::{joint, InstantHmr, Joints};
use infer::person_box::{Keypoint, PersonBox, PersonBoxTracker, KEYPOINTS};
use infer::upper_body::{HandStage, Image};

/// BlazePose slot and InstantHMR joint for each landmark the avatar solver and the hand stage read.
const SLOTS: [(usize, usize); 13] = [
    (7, joint::L_EAR),
    (8, joint::R_EAR),
    (11, joint::L_SHOULDER),
    (12, joint::R_SHOULDER),
    (13, joint::L_ELBOW),
    (14, joint::R_ELBOW),
    (15, joint::L_WRIST),
    (16, joint::R_WRIST),
    (17, joint::L_PINKY_KNUCKLE),
    (18, joint::R_PINKY_KNUCKLE),
    (19, joint::L_INDEX_KNUCKLE),
    (20, joint::R_INDEX_KNUCKLE),
    (0, joint::NOSE),
];

pub struct HmrBody {
    lead: LeadFilter,
    hmr: InstantHmr,
    boxes: PersonBoxTracker,
    hands: Box<dyn HandStage>,
    mirror: bool,
    /// Debug drawing of the last frame, in frame pixels.
    pub overlay: crate::debug_overlay::Overlay,
    /// Milliseconds the last frame spent in the body model and in the hand stage.
    pub last_ms: (f32, f32),
}

impl HmrBody {
    pub fn new(hmr: InstantHmr, hands: Box<dyn HandStage>, mirror: bool) -> Self {
        Self { lead: LeadFilter::default(), hmr, boxes: PersonBoxTracker::new(), hands, mirror, overlay: Default::default(), last_ms: (0.0, 0.0) }
    }

    /// `rgb` is `width * height * 3` bytes, not mirrored. `face` is the face-landmark box in frame
    /// pixels, if the face tracker has one. `vis_min` and `hand_min` are the confidence settings.
    pub fn run(
        &mut self,
        rgb: &[u8],
        width: u32,
        height: u32,
        face: Option<PersonBox>,
        vis_min: f32,
        hand_min: f32,
    ) -> Result<BodyPose, String> {
        let (w, h) = (width as usize, height as usize);
        let (fw, fh) = (width as f32, height as f32);
        let img = Image { rgb, w, h };
        let person = self.boxes.next(face, fw, fh).unwrap_or(PersonBox { x: 0.0, y: 0.0, w: fw, h: fh });
        let t0 = std::time::Instant::now();
        let joints = match self.hmr.run(&img, person) {
            Ok(j) => j,
            Err(e) => {
                self.boxes.update(None, fw, fh);
                return Err(e);
            }
        };
        self.boxes.update(Some(&box_keypoints(&joints)), fw, fh);

        let mut pose = pose_landmarks(&joints);
        let t1 = std::time::Instant::now();
        let hands = self.hands.run(&img, &pose)?;
        refine_wrists(&mut pose, &hands, hand_min);
        self.last_ms = ((t1 - t0).as_secs_f32() * 1000.0, t1.elapsed().as_secs_f32() * 1000.0);
        let out = UpperBodyOutput { width, height, pose, presence: 1.0, hands, hand_model: self.hands.label() };
        self.overlay = crate::debug_overlay::from_upper_body(&out, vis_min, hand_min);
        let mut body = to_avatar_space(&out, self.mirror, vis_min, hand_min);
        drop_arms_without_elbow(&mut body);
        // The pose describes the person about one model run ago; the camera frame is older still.
        let age = (self.last_ms.0 + self.last_ms.1) / 1000.0 + CAMERA_AGE_S;
        Ok(self.lead.apply(body, age))
    }
}

/// The hand model's wrist may differ from the body model's wrist by at most this many shoulder widths
/// (more means the hand belongs to someone or something else) before it is ignored.
const WRIST_FIX_MAX: f32 = 0.5;

/// The hand model sees the hand at a much higher resolution than the 224-pixel body crop, so its wrist
/// point is the better one (error against CIGPose-m 0.054 against 0.14 for the body model's wrist, see
/// docs/notes/instanthmr.md). A found hand moves the wrist in the image plane; the depth stays the
/// body model's. The shift in metres comes from the shoulder width in pixels and in metres.
fn refine_wrists(pose: &mut [PoseLandmark; POSE_POINTS], hands: &[HandLandmarks], hand_min: f32) {
    let (l, r) = (pose[11], pose[12]);
    let px = (l.x - r.x).hypot(l.y - r.y);
    let m = ((l.world[0] - r.world[0]).powi(2) + (l.world[1] - r.world[1]).powi(2) + (l.world[2] - r.world[2]).powi(2)).sqrt();
    if px < 1.0 || m < 0.05 {
        return;
    }
    let per_px = m / px;
    for hand in hands.iter().filter(|h| h.score >= hand_min) {
        let slot = match hand.side {
            SubjectSide::Left => 15,
            SubjectSide::Right => 16,
        };
        let wrist = &mut pose[slot];
        let (nx, ny) = (hand.points[0][0], hand.points[0][1]);
        let (dx, dy) = (nx - wrist.x, ny - wrist.y);
        if dx.hypot(dy) > WRIST_FIX_MAX * px {
            continue;
        }
        wrist.x = nx;
        wrist.y = ny;
        wrist.world[0] += dx * per_px;
        wrist.world[1] += dy * per_px;
    }
}

/// How old the camera frame already is when the model starts on it: half a frame interval at the 10 fps
/// the camera delivers at 1280x720 on the test laptop.
const CAMERA_AGE_S: f32 = 0.05;
/// Share of the measured delay that is made up by extrapolating. Below 1 so a stop does not overshoot far.
const LEAD_GAIN: f32 = 0.6;
/// Arm speeds below this (metres per second) are treated as standing still, so joint noise is not amplified.
const LEAD_DEADZONE: f32 = 0.08;
/// No joint is pushed further than this (metres).
const LEAD_MAX: f32 = 0.12;
/// Two poses further apart in time than this say nothing about the speed (a gap, a lost frame).
const LEAD_MAX_GAP_S: f32 = 0.4;

/// Pushes the elbows and wrists forward along their recent velocity, to make up part of the pipeline delay
/// (camera interval plus model time, about 100 to 150 ms): at 10 fps the arms otherwise trail the person.
#[derive(Default)]
struct LeadFilter {
    prev: Option<(BodyPose, std::time::Instant)>,
}

impl LeadFilter {
    fn apply(&mut self, pose: BodyPose, age_s: f32) -> BodyPose {
        self.apply_at(pose, age_s, std::time::Instant::now())
    }

    fn apply_at(&mut self, pose: BodyPose, age_s: f32, now: std::time::Instant) -> BodyPose {
        let mut out = pose.clone();
        if let Some((prev, then)) = &self.prev {
            let dt = (now - *then).as_secs_f32();
            if dt > 0.02 && dt < LEAD_MAX_GAP_S {
                for (o, (cur, old)) in [(&mut out.left, (&pose.left, &prev.left)), (&mut out.right, (&pose.right, &prev.right))] {
                    o.elbow = lead(cur.elbow, old.elbow, dt, age_s);
                    o.wrist = lead(cur.wrist, old.wrist, dt, age_s);
                }
            }
        }
        self.prev = Some((pose, now));
        out
    }
}

fn lead(cur: Option<Vec3>, old: Option<Vec3>, dt: f32, age_s: f32) -> Option<Vec3> {
    let c = cur?;
    // No earlier position (the joint was missing): nothing to take a speed from, so it stays where it is.
    let Some(o) = old else { return Some(c) };
    let v = (c - o) / dt;
    let speed = v.length();
    if speed <= LEAD_DEADZONE {
        return Some(c);
    }
    let eased = v / speed * (speed - LEAD_DEADZONE);
    let push = eased * age_s * LEAD_GAIN;
    let len = push.length();
    Some(if len > LEAD_MAX { c + push / len * LEAD_MAX } else { c + push })
}

/// The model fills in every joint, seen or not. A wrist or hand whose elbow lies outside the picture
/// (an arm cut off by the frame) has nothing to hang from and swings the arm out sideways, so such an
/// arm keeps only its shoulder and the avatar holds it at rest.
fn drop_arms_without_elbow(body: &mut BodyPose) {
    for arm in [&mut body.left, &mut body.right] {
        if arm.elbow.is_none() {
            arm.wrist = None;
            arm.index = None;
            arm.pinky = None;
            arm.hand = None;
        }
    }
}

/// The 17 COCO keypoints the person-box tracker reads: the head, shoulders, elbows and wrists.
fn box_keypoints(j: &Joints) -> [Keypoint; KEYPOINTS] {
    let mut kps = [Keypoint { x: 0.0, y: 0.0, score: 0.0 }; KEYPOINTS];
    // COCO 0..=8 are the MHR70 joints 0..=8 (nose, eyes, ears, shoulders, elbows); COCO 9 and 10 are the wrists.
    for (coco, mhr) in (0..=8).map(|i| (i, i)).chain([(9, joint::L_WRIST), (10, joint::R_WRIST)]) {
        kps[coco] = Keypoint { x: j.xy[mhr][0], y: j.xy[mhr][1], score: 1.0 };
    }
    kps
}

/// BlazePose-style landmarks (pixels plus metric world position) for the joints the solver reads.
fn pose_landmarks(j: &Joints) -> [PoseLandmark; POSE_POINTS] {
    let mut pose = [PoseLandmark::default(); POSE_POINTS];
    for (slot, mhr) in SLOTS {
        pose[slot] = PoseLandmark { x: j.xy[mhr][0], y: j.xy[mhr][1], z: 0.0, visibility: 1.0, world: j.xyz[mhr] };
    }
    pose
}

#[cfg(test)]
mod tests {
    use super::*;
    use infer::instant_hmr::JOINTS;

    fn joints() -> Joints {
        let mut j = Joints { xy: [[0.0; 2]; JOINTS], xyz: [[0.0; 3]; JOINTS] };
        j.xy[joint::L_WRIST] = [10.0, 20.0];
        j.xyz[joint::L_WRIST] = [0.3, -0.1, -0.2];
        j.xy[joint::R_SHOULDER] = [50.0, 60.0];
        j
    }

    #[test]
    fn landmarks_land_in_the_blazepose_slots_the_solver_reads() {
        let pose = pose_landmarks(&joints());
        assert_eq!((pose[15].x, pose[15].y, pose[15].world), (10.0, 20.0, [0.3, -0.1, -0.2]));
        assert_eq!(pose[12].x, 50.0);
        assert_eq!(pose[15].visibility, 1.0);
        assert_eq!(pose[30].visibility, 0.0, "slots the model does not fill stay empty (hips are never read)");
    }

    #[test]
    fn an_arm_without_an_elbow_keeps_only_its_shoulder() {
        use avatar::body::ArmPose;
        use avatar::Vec3;
        let seen = Some(Vec3::ONE);
        let mut body = BodyPose {
            left: ArmPose { shoulder: seen, elbow: None, wrist: seen, index: seen, pinky: seen, hand: None },
            right: ArmPose { shoulder: seen, elbow: seen, wrist: seen, ..Default::default() },
            head: None,
        };
        drop_arms_without_elbow(&mut body);
        assert!(body.left.shoulder.is_some() && body.left.wrist.is_none() && body.left.index.is_none());
        assert!(body.right.wrist.is_some(), "an arm with its elbow is untouched");
    }

    #[test]
    fn a_moving_wrist_is_pushed_ahead_a_still_one_is_not() {
        use avatar::body::ArmPose;
        use std::time::{Duration, Instant};
        let pose = |x: f32| BodyPose {
            left: ArmPose { elbow: Some(Vec3::new(x, 1.0, 0.0)), wrist: Some(Vec3::new(x, 1.2, 0.0)), ..Default::default() },
            right: ArmPose { wrist: Some(Vec3::new(0.0, 1.0, 0.0)), ..Default::default() },
            head: None,
        };
        let mut f = LeadFilter::default();
        let t0 = Instant::now();
        let first = f.apply_at(pose(0.0), 0.15, t0);
        assert_eq!(first.left.wrist, pose(0.0).left.wrist, "the first pose has no velocity yet");
        // 0.1 m in 0.1 s is 1 m/s: ahead by about (1 - deadzone) * 0.15 s * gain.
        let moved = f.apply_at(pose(0.1), 0.15, t0 + Duration::from_millis(100));
        let ahead = moved.left.wrist.unwrap().x - 0.1;
        assert!((ahead - (1.0 - LEAD_DEADZONE) * 0.15 * LEAD_GAIN).abs() < 1e-3, "ahead {ahead}");
        assert_eq!(moved.right.wrist, Some(Vec3::new(0.0, 1.0, 0.0)), "a still joint stays where it is");
    }

    #[test]
    fn the_lead_is_clamped_and_ignores_gaps_and_missing_joints() {
        use avatar::body::ArmPose;
        use std::time::{Duration, Instant};
        let at = |x: f32, wrist: bool| BodyPose {
            left: ArmPose { wrist: wrist.then(|| Vec3::new(x, 1.0, 0.0)), ..Default::default() },
            ..Default::default()
        };
        let mut f = LeadFilter::default();
        let t0 = Instant::now();
        f.apply_at(at(0.0, true), 0.15, t0);
        let fast = f.apply_at(at(1.0, true), 0.15, t0 + Duration::from_millis(100));
        assert!((fast.left.wrist.unwrap().x - 1.0 - LEAD_MAX).abs() < 1e-4, "a 10 m/s jump is clamped");
        let after_gap = f.apply_at(at(2.0, true), 0.15, t0 + Duration::from_millis(900));
        assert_eq!(after_gap.left.wrist.unwrap().x, 2.0, "no speed from a pose 0.8 s old");
        f.apply_at(at(2.0, false), 0.15, t0 + Duration::from_millis(1000));
        let back = f.apply_at(at(2.5, true), 0.15, t0 + Duration::from_millis(1100));
        assert_eq!(back.left.wrist.unwrap().x, 2.5, "a joint that was missing has no velocity");
    }

    #[test]
    fn the_hand_models_wrist_moves_the_body_models_wrist_in_the_image_plane() {
        let mut j = joints();
        j.xy[joint::L_SHOULDER] = [100.0, 100.0];
        j.xy[joint::R_SHOULDER] = [0.0, 100.0];
        j.xyz[joint::L_SHOULDER] = [0.2, 0.0, 0.0];
        j.xyz[joint::R_SHOULDER] = [-0.2, 0.0, 0.0];
        let mut pose = pose_landmarks(&j);
        let before = pose[15];
        let mut points = [[0.0f32; 3]; 21];
        points[0] = [before.x + 10.0, before.y - 20.0, 0.0];
        let hand = |score, side| HandLandmarks { points, score, handedness: 0.0, side };
        // 100 px is 0.4 m, so 10 px right and 20 px up is 0.04 m and 0.08 m.
        refine_wrists(&mut pose, &[hand(0.3, SubjectSide::Left)], 0.5);
        assert_eq!(pose[15], before, "a hand below the confidence threshold changes nothing");
        refine_wrists(&mut pose, &[hand(0.9, SubjectSide::Left)], 0.5);
        assert_eq!((pose[15].x, pose[15].y), (before.x + 10.0, before.y - 20.0));
        assert!((pose[15].world[0] - before.world[0] - 0.04).abs() < 1e-4 && (pose[15].world[1] - before.world[1] + 0.08).abs() < 1e-4);
        assert_eq!(pose[15].world[2], before.world[2], "depth stays the body model's");
        assert_eq!(pose[16].x, joints().xy[joint::R_WRIST][0], "the other wrist is untouched");
        // A hand far from the wrist (more than half a shoulder width) is somebody else's.
        let mut far = [[0.0f32; 3]; 21];
        far[0] = [before.x + 90.0, before.y, 0.0];
        let mut pose = pose_landmarks(&j);
        refine_wrists(&mut pose, &[HandLandmarks { points: far, score: 0.9, handedness: 0.0, side: SubjectSide::Left }], 0.5);
        assert_eq!(pose[15], before);
    }

    #[test]
    fn the_box_tracker_gets_the_head_shoulders_elbows_and_wrists() {
        let kps = box_keypoints(&joints());
        assert_eq!((kps[9].x, kps[9].y, kps[9].score), (10.0, 20.0, 1.0), "COCO left wrist is MHR 62");
        assert_eq!(kps[6].x, 50.0);
        assert_eq!(kps[11].score, 0.0, "hips stay unused");
    }

    use infer::engine::resolve_engine;
    use infer::person_box::face_rect;
    use infer::session::build_session;
    use infer::upper_body::Mp2023Hands;
    use std::path::Path;
    use std::time::Instant;

    /// Runs the whole path over the golden_take frames and prints the median and p95 time per frame and the
    /// peak working set (`VT_ENGINE=ort|openvino cargo test --release -p app --bin live hmr_path -- --nocapture`).
    #[test]
    fn hmr_path_on_golden_take() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = root.join("fixtures/private/golden_take");
        let ub = root.join("models/upper_body");
        let model = ub.join("instanthmr_joints.onnx");
        if !dir.join("lm.txt").exists() || !model.exists() {
            eprintln!("SKIP hmr_body: private fixtures or models missing");
            return;
        }
        let load = |n: &str| build_session(&ub.join(n), 4).unwrap().0;
        let hands = Box::new(Mp2023Hands::new(
            load("palm_detection_mediapipe_2023feb.onnx"),
            load("handpose_estimation_mediapipe_2023feb.onnx"),
        ));
        let engine = resolve_engine();
        let mut stage = HmrBody::new(InstantHmr::load(engine, &model, 6).unwrap(), hands, true);
        let (mut ms, mut with_arm, mut frames) = (Vec::new(), 0, 0);
        let (mut hmr_ms, mut hand_ms) = (Vec::new(), Vec::new());
        for (i, line) in std::fs::read_to_string(dir.join("lm.txt")).unwrap().lines().enumerate() {
            let v: Vec<f32> = line.split_whitespace().skip(1).map(|t| t.parse().unwrap()).collect();
            let pts: Vec<[f32; 3]> = v.chunks_exact(2).map(|c| [c[0], c[1], 0.0]).collect();
            let rgb = image::open(dir.join(format!("f{i:04}.jpg"))).unwrap().to_rgb8();
            let started = Instant::now();
            let body = stage.run(rgb.as_raw(), rgb.width(), rgb.height(), face_rect(&pts), 0.8, 0.8).unwrap();
            ms.push(started.elapsed().as_secs_f32() * 1000.0);
            hmr_ms.push(stage.last_ms.0);
            hand_ms.push(stage.last_ms.1);
            frames += 1;
            with_arm += (body.left.shoulder.is_some() && body.right.shoulder.is_some()) as usize;
        }
        ms.sort_by(|a, b| a.total_cmp(b));
        hmr_ms.sort_by(|a, b| a.total_cmp(b));
        hand_ms.sort_by(|a, b| a.total_cmp(b));
        println!("split: model median {:.1} ms, hand stage median {:.1} ms", hmr_ms[hmr_ms.len() / 2], hand_ms[hand_ms.len() / 2]);
        let mem = timing::memory::process_memory().map(|m| format!("{m:?}")).unwrap_or_default();
        println!("hmr path ({engine:?}): {frames} frames, shoulders found on {with_arm}, median {:.1} ms, p95 {:.1} ms, memory {mem}", ms[ms.len() / 2], ms[ms.len() * 95 / 100]);
    }

}
