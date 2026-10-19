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
use smoothing::one_euro::OneEuro;

#[cfg(test)]
use crate::hmr_hands::hand_landmarks;

/// Pose slot (33-point layout, BlazePose order) and InstantHMR joint for each landmark the avatar solver reads.
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

/// One Euro settings of the InstantHMR demo (`smoothing.py`, live mode): (minimum cutoff in Hz, beta).
const SMOOTH_2D: (f32, f32) = (1.0, 0.01);
const SMOOTH_3D: (f32, f32) = (1.0, 4.0);

/// The demo's temporal smoothing, applied to the joints this path reads: one filter per coordinate.
struct JointSmoother {
    xy: Vec<[OneEuro; 2]>,
    xyz: Vec<[OneEuro; 3]>,
    last: Option<std::time::Instant>,
}

impl Default for JointSmoother {
    fn default() -> Self {
        Self {
            xy: (0..SLOTS.len()).map(|_| std::array::from_fn(|_| OneEuro::new(SMOOTH_2D.0, SMOOTH_2D.1))).collect(),
            xyz: (0..SLOTS.len()).map(|_| std::array::from_fn(|_| OneEuro::new(SMOOTH_3D.0, SMOOTH_3D.1))).collect(),
            last: None,
        }
    }
}

impl JointSmoother {
    fn apply(&mut self, j: &mut Joints) {
        self.apply_at(j, std::time::Instant::now());
    }

    fn apply_at(&mut self, j: &mut Joints, now: std::time::Instant) {
        let dt = self.last.map(|t| (now - t).as_secs_f32()).unwrap_or(0.0);
        self.last = Some(now);
        if dt > LEAD_MAX_GAP_S {
            // The person was away or the run stalled: start over instead of gliding from the old place.
            *self = Self { last: Some(now), ..Self::default() };
        }
        // The demo falls back to 1/30 s when no time has passed.
        let dt = if dt > 0.0 && dt <= LEAD_MAX_GAP_S { dt } else { 1.0 / 30.0 };
        for (i, (_, mhr)) in SLOTS.iter().enumerate() {
            for k in 0..2 {
                j.xy[*mhr][k] = self.xy[i][k].filter(j.xy[*mhr][k], dt);
            }
            for k in 0..3 {
                j.xyz[*mhr][k] = self.xyz[i][k].filter(j.xyz[*mhr][k], dt);
            }
        }
    }
}

pub struct HmrBody {
    smooth: JointSmoother,
    lead: LeadFilter,
    camera: CameraClock,
    hmr: InstantHmr,
    boxes: PersonBoxTracker,
    /// MediaPipe hand stage. The fingers come from this hand model only.
    hands: Box<dyn HandStage>,
    mirror: bool,
    /// Debug drawing of the last frame, in frame pixels.
    pub overlay: crate::debug_overlay::Overlay,
    /// Milliseconds the last frame spent in the body model and in the hand stage. The fingers come from the hand model only.
    pub last_ms: (f32, f32),
}

impl HmrBody {
    pub fn new(hmr: InstantHmr, hands: Box<dyn HandStage>, mirror: bool) -> Self {
        Self { smooth: JointSmoother::default(), lead: LeadFilter::default(), camera: CameraClock::default(), hmr, boxes: PersonBoxTracker::new(), hands, mirror, overlay: Default::default(), last_ms: (0.0, 0.0) }
    }

    /// Call once per camera frame with its arrival time, so the lead knows the camera's frame interval.
    #[allow(dead_code)] // only live feeds frames; the main.rs build includes this module without a caller
    pub fn observe_frame(&mut self, arrived: std::time::Instant) {
        self.camera.observe(arrived);
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
        let mut joints = match self.hmr.run(&img, person) {
            Ok(j) => j,
            Err(e) => {
                self.boxes.update(None, fw, fh);
                return Err(e);
            }
        };
        self.smooth.apply(&mut joints);
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
        let age = (self.last_ms.0 + self.last_ms.1) / 1000.0 + self.camera.age_s();
        Ok(self.lead.apply(body, age))
    }
}

const WRIST_FIX_MAX: f32 = 0.5;

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
        let (nx, ny) = (hand.points[0][0], hand.points[0][1]);
        let (dx, dy) = (nx - pose[slot].x, ny - pose[slot].y);
        if dx.hypot(dy) > WRIST_FIX_MAX * px {
            continue;
        }
        let wrist = &mut pose[slot];
        wrist.x = nx;
        wrist.y = ny;
        wrist.world[0] += dx * per_px;
        wrist.world[1] += dy * per_px;
    }
}

/// How old the camera frame already is when the model starts on it, used until a frame interval is
/// measured: half of a 10 fps interval, the rate the camera delivers at 1280x720 on the test laptop.
const CAMERA_AGE_S: f32 = 0.05;
/// Weight of each new frame interval in the running average.
#[allow(dead_code)] // read only by the camera clock, which only live feeds
const CAMERA_INTERVAL_GAIN: f32 = 0.2;
/// Gaps longer than this are a stalled camera, not its frame interval.
#[allow(dead_code)] // read only by the camera clock, which only live feeds
const CAMERA_MAX_GAP_S: f32 = 1.0;

/// Measures the camera's frame interval from frame arrival times and gives half of it as the age.
#[derive(Default)]
struct CameraClock {
    #[allow(dead_code)] // written only by the camera clock, which only live feeds
    last: Option<std::time::Instant>,
    interval_s: Option<f32>,
}

impl CameraClock {
    #[allow(dead_code)] // only live feeds frames; the main.rs build includes this module without a caller
    fn observe(&mut self, arrived: std::time::Instant) {
        if let Some(prev) = self.last {
            let dt = (arrived - prev).as_secs_f32();
            if dt > 0.0 && dt <= CAMERA_MAX_GAP_S {
                self.interval_s = Some(match self.interval_s {
                    None => dt,
                    Some(i) => i + (dt - i) * CAMERA_INTERVAL_GAIN,
                });
            }
        }
        self.last = Some(arrived);
    }

    fn age_s(&self) -> f32 {
        self.interval_s.map_or(CAMERA_AGE_S, |i| i / 2.0)
    }
}
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

/// Pose landmarks in the 33-point layout (pixels plus metric world position) for the joints the solver reads.
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
    fn smoothing_calms_jitter_and_restarts_after_a_gap() {
        use std::time::{Duration, Instant};
        let at = |x: f32| {
            let mut j = joints();
            j.xyz[joint::L_WRIST][0] = x;
            j
        };
        let mut s = JointSmoother::default();
        let t0 = Instant::now();
        let mut first = at(0.0);
        s.apply_at(&mut first, t0);
        assert_eq!(first.xyz[joint::L_WRIST][0], 0.0, "the first sample passes unchanged");
        let mut small = at(0.01);
        s.apply_at(&mut small, t0 + Duration::from_millis(100));
        let moved = small.xyz[joint::L_WRIST][0];
        assert!(moved > 0.0 && moved < 0.01, "a 1 cm jitter is damped, got {moved}");
        let mut later = at(2.0);
        s.apply_at(&mut later, t0 + Duration::from_millis(1500));
        assert_eq!(later.xyz[joint::L_WRIST][0], 2.0, "after a long gap the filter starts over");
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
    fn the_lead_age_grows_with_the_camera_interval() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut clock = CameraClock::default();
        assert_eq!(clock.age_s(), CAMERA_AGE_S, "no frames yet: the fixed default");
        for i in 0..20 {
            clock.observe(t0 + Duration::from_millis(100 * i));
        }
        assert!((clock.age_s() - 0.05).abs() < 1e-4, "10 fps: half of 0.1 s, got {}", clock.age_s());
        let mut slow = CameraClock::default();
        for i in 0..20 {
            slow.observe(t0 + Duration::from_millis(200 * i));
        }
        assert!((slow.age_s() - 0.1).abs() < 1e-4, "5 fps: half of 0.2 s, got {}", slow.age_s());
        assert!(slow.age_s() > clock.age_s());
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

    /// Fingers probe: InstantHMR's 21 hand points against the 2023 hand model on the four hand photos.
    /// Prints the mean pixel error per finger group divided by the hand model's palm length. Not a gate
    /// (`cargo test --release -p app finger_probe -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn finger_probe() {
        use core::body::HAND_POINTS;
        use infer::session::build_session;
        use infer::upper_body::{HandStage, Mp2023Hands};
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = root.join("fixtures/private/upper_body_photos");
        let ub = root.join("models/upper_body");
        let model = ub.join("instanthmr_joints.onnx");
        let (palm_path, lm_path) = (ub.join("palm_detection_mediapipe_2023feb.onnx"), ub.join("handpose_estimation_mediapipe_2023feb.onnx"));
        let photos: Vec<std::path::PathBuf> = ["hands_15", "hands_16", "hands_17", "hands_19"].iter().map(|n| dir.join(format!("{n}.png"))).collect();
        if !model.exists() || !palm_path.exists() || !lm_path.exists() || photos.iter().any(|p| !p.exists()) {
            eprintln!("SKIP finger_probe: photos or models missing");
            return;
        }
        let groups: [(&str, std::ops::Range<usize>); 6] = [("wrist", 0..1), ("thumb", 1..5), ("index", 5..9), ("middle", 9..13), ("ring", 13..17), ("pinky", 17..21)];
        let px = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let mut hmr = InstantHmr::load(infer::engine::Engine::Ort, &model, 4).unwrap();
        let mut old = Mp2023Hands::new(build_session(&palm_path, 4).unwrap().0, build_session(&lm_path, 4).unwrap().0);
        let (mut sum, mut pairs) = ([0.0f32; 6], 0usize);
        // Wrist-relative error per group, and per finger the InstantHMR curl, the 2023 curl and their
        // mean absolute difference, all in radians (the mean over the three joints of each finger).
        let (mut rel_sum, mut curl_sum) = ([0.0f32; 6], [[0.0f32; 3]; 5]);
        let rel = |p: &[[f32; 3]; HAND_POINTS]| -> [[f32; 3]; HAND_POINTS] {
            std::array::from_fn(|i| [p[i][0] - p[0][0], p[i][1] - p[0][1], p[i][2] - p[0][2]])
        };
        let vecs = |p: &[[f32; 3]; HAND_POINTS]| -> [avatar::Vec3; HAND_POINTS] {
            std::array::from_fn(|i| avatar::Vec3::new(p[i][0], p[i][1], p[i][2]))
        };
        for photo in &photos {
            let rgb = image::open(photo).unwrap().to_rgb8();
            let (w, h) = (rgb.width(), rgb.height());
            let img = Image { rgb: rgb.as_raw(), w: w as usize, h: h as usize };
            let person = PersonBox { x: 0.0, y: 0.0, w: w as f32, h: h as f32 };
            let j = hmr.run(&img, person).unwrap();
            let new: Vec<HandLandmarks> = [SubjectSide::Left, SubjectSide::Right].into_iter().map(|side| hand_landmarks(&j, side)).collect();
            let old_hands = old.run(&img, &pose_landmarks(&j)).unwrap();
            println!("{}: InstantHMR {} hands, 2023 hand stage {} hands", photo.display(), new.len(), old_hands.len());
            for n in &new {
                let Some(o) = old_hands.iter().find(|o| o.side == n.side) else {
                    println!("  {:?}: no 2023 hand on this side", n.side);
                    continue;
                };
                let palm = px(o.points[0], o.points[9]);
                if palm <= 0.0 {
                    continue;
                }
                let d: Vec<f32> = (0..HAND_POINTS).map(|i| px(n.points[i], o.points[i]) / palm).collect();
                for (g, (_, r)) in groups.iter().enumerate() {
                    sum[g] += r.clone().map(|i| d[i]).sum::<f32>() / r.len() as f32;
                }
                pairs += 1;
                let (rn, ro) = (rel(&n.points), rel(&o.points));
                let r: Vec<f32> = (0..HAND_POINTS).map(|i| px(rn[i], ro[i]) / palm).collect();
                for (g, (_, range)) in groups.iter().enumerate().skip(1) {
                    rel_sum[g] += range.clone().map(|i| r[i]).sum::<f32>() / range.len() as f32;
                }
                let (cn, co) = (avatar::body::curl_angles(&vecs(&n.points)), avatar::body::curl_angles(&vecs(&o.points)));
                for f in 0..5 {
                    let mean = |c: &[[f32; 3]; 5]| c[f].iter().sum::<f32>() / 3.0;
                    curl_sum[f][0] += mean(&cn);
                    curl_sum[f][1] += mean(&co);
                    curl_sum[f][2] += (0..3).map(|j| (cn[f][j] - co[f][j]).abs()).sum::<f32>() / 3.0;
                }
                let same =d.iter().sum::<f32>() / HAND_POINTS as f32;
                let opposite = old_hands.iter().find(|x| x.side != n.side).map(|x| {
                    (0..HAND_POINTS).map(|i| px(n.points[i], x.points[i])).sum::<f32>() / HAND_POINTS as f32 / palm
                });
                println!("  {:?}: all points / palm, same side {same:.3}, opposite side {:.3}", n.side, opposite.unwrap_or(f32::NAN));
            }
        }
        println!("| finger group | mean error / palm (hands {pairs}) |");
        println!("| --- | --- |");
        for (g, (name, _)) in groups.iter().enumerate() {
            println!("| {name} | {:.3} |", sum[g] / pairs.max(1) as f32);
        }
        let n = pairs.max(1) as f32;
        println!("| finger group (wrist subtracted) | mean error / palm (hands {pairs}) |");
        println!("| --- | --- |");
        for (g, (name, _)) in groups.iter().enumerate().skip(1) {
            println!("| {name} | {:.3} |", rel_sum[g] / n);
        }
        println!("| finger | InstantHMR curl deg | 2023 curl deg | mean abs diff deg (hands {pairs}) |");
        println!("| --- | --- | --- | --- |");
        for (f, name) in ["thumb", "index", "middle", "ring", "pinky"].iter().enumerate() {
            let c = curl_sum[f];
            println!("| {name} | {:.1} | {:.1} | {:.1} |", (c[0] / n).to_degrees(), (c[1] / n).to_degrees(), (c[2] / n).to_degrees());
        }
    }

    /// Image hands probe: the full production path (`HmrBody::run` with the Mp2023 hand stage, three runs so the
    /// joint smoothing settles) on the image in `VT_PROBE_IMG`. Prints body and hand ms per run and the hand count,
    /// and saves the debug overlay drawn on the image to `VT_PROBE_OUT`. Not a gate
    /// (`cargo test -p app image_hands_probe -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn image_hands_probe() {
        let (Ok(img_path), Ok(out_path)) = (std::env::var("VT_PROBE_IMG"), std::env::var("VT_PROBE_OUT")) else {
            eprintln!("SKIP image_hands_probe: set VT_PROBE_IMG and VT_PROBE_OUT");
            return;
        };
        let ub = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/upper_body");
        let load = |n: &str| build_session(&ub.join(n), 4).unwrap().0;
        let hands = Box::new(Mp2023Hands::new(
            load("palm_detection_mediapipe_2023feb.onnx"),
            load("handpose_estimation_mediapipe_2023feb.onnx"),
        ));
        let hmr = InstantHmr::load(resolve_engine(), &ub.join("instanthmr_joints.onnx"), 6).unwrap();
        let mut body = HmrBody::new(hmr, hands, false);

        let rgb = image::open(&img_path).unwrap().to_rgb8();
        let (w, h) = rgb.dimensions();
        println!("image {w}x{h}");
        for run in 0..3 {
            body.run(rgb.as_raw(), w, h, None, 0.5, 0.5).unwrap();
            // Each hand draws 21 lime lines (20 edges and the wrist-to-wrist line); hands below hand_min draw none.
            let hand_lines = body.overlay.lines.iter().filter(|l| l.2 == [150, 255, 0]).count();
            println!(
                "run {run}: body {:.1} ms, hands {:.1} ms, hands drawn {} (overlay has no per-hand score)",
                body.last_ms.0,
                body.last_ms.1,
                hand_lines / 21
            );
        }

        // Thickness grows with the image: 1 px per 400 px of the longer side, so 800 px gives 2 px lines.
        let s = (w.max(h) as i32 / 400).max(1);
        fn dot(img: &mut image::RgbImage, x: f32, y: f32, r: i32, c: [u8; 3]) {
            let (cx, cy) = (x.round() as i32, y.round() as i32);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (px, py) = (cx + dx, cy + dy);
                    if dx * dx + dy * dy <= r * r && px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                        img.put_pixel(px as u32, py as u32, image::Rgb(c));
                    }
                }
            }
        }
        let mut canvas = rgb.clone();
        // The overlay is in source-frame pixels, which is the unmirrored image drawn here.
        for (a, b, c) in &body.overlay.lines {
            let n = (b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil().max(1.0) as usize;
            for i in 0..=n {
                let t = i as f32 / n as f32;
                dot(&mut canvas, a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, s, *c);
            }
        }
        for (p, c, r) in &body.overlay.points {
            dot(&mut canvas, p[0], p[1], *r * s, *c);
        }
        canvas.save(&out_path).unwrap();
        println!("saved {out_path}");
    }
}
