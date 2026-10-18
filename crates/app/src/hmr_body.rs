//! Live body path for `live --body` (default): person box from the face landmarks (then from the
//! previous frame's joints), InstantHMR for 3D shoulders, elbows, wrists and ears, and the MediaPipe
//! hand stage for fingers.
//!
//! InstantHMR returns metric 3D joints (x right, y down, z away, the same frame as BlazePose's
//! world landmarks), so they go straight into the pose structure that `to_avatar_space` reads and no
//! 2D-to-3D lift is needed. The model gives no per-joint confidence, so every joint counts as seen;
//! the "inside the picture" check in `to_avatar_space` still drops joints that fall outside the frame.

use avatar::body::{to_avatar_space, BodyPose};
use core::body::{PoseLandmark, UpperBodyOutput, POSE_POINTS};
use infer::cigpose::{Keypoint, PersonBox, KEYPOINTS};
use infer::instant_hmr::{joint, InstantHmr, Joints};
use infer::person_box::PersonBoxTracker;
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
    hmr: InstantHmr,
    boxes: PersonBoxTracker,
    hands: Box<dyn HandStage>,
    mirror: bool,
    /// Debug drawing of the last frame, in frame pixels.
    pub overlay: crate::debug_overlay::Overlay,
}

impl HmrBody {
    pub fn new(hmr: InstantHmr, hands: Box<dyn HandStage>, mirror: bool) -> Self {
        Self { hmr, boxes: PersonBoxTracker::new(), hands, mirror, overlay: Default::default() }
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
        let joints = match self.hmr.run(&img, person) {
            Ok(j) => j,
            Err(e) => {
                self.boxes.update(None, fw, fh);
                return Err(e);
            }
        };
        self.boxes.update(Some(&box_keypoints(&joints)), fw, fh);

        let pose = pose_landmarks(&joints);
        let hands = self.hands.run(&img, &pose)?;
        let out = UpperBodyOutput { width, height, pose, presence: 1.0, hands, hand_model: self.hands.label() };
        self.overlay = crate::debug_overlay::from_upper_body(&out, vis_min, hand_min);
        let mut body = to_avatar_space(&out, self.mirror, vis_min, hand_min);
        drop_arms_without_elbow(&mut body);
        Ok(body)
    }
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
        use avatar::arm_lift::Vec3;
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
        for (i, line) in std::fs::read_to_string(dir.join("lm.txt")).unwrap().lines().enumerate() {
            let v: Vec<f32> = line.split_whitespace().skip(1).map(|t| t.parse().unwrap()).collect();
            let pts: Vec<[f32; 3]> = v.chunks_exact(2).map(|c| [c[0], c[1], 0.0]).collect();
            let rgb = image::open(dir.join(format!("f{i:04}.jpg"))).unwrap().to_rgb8();
            let started = Instant::now();
            let body = stage.run(rgb.as_raw(), rgb.width(), rgb.height(), face_rect(&pts), 0.8, 0.8).unwrap();
            ms.push(started.elapsed().as_secs_f32() * 1000.0);
            frames += 1;
            with_arm += (body.left.shoulder.is_some() && body.right.shoulder.is_some()) as usize;
        }
        ms.sort_by(|a, b| a.total_cmp(b));
        let mem = timing::memory::process_memory().map(|m| format!("{m:?}")).unwrap_or_default();
        println!("hmr path ({engine:?}): {frames} frames, shoulders found on {with_arm}, median {:.1} ms, p95 {:.1} ms, memory {mem}", ms[ms.len() / 2], ms[ms.len() * 95 / 100]);
    }

    /// Arm accuracy probe on the four hands photos, scored against CIGPose-m (2D, COCO-trained, the most
    /// accurate model of the earlier consensus test): pixel distance over CIGPose's shoulder width, per
    /// joint kind. Reference and models share no weights, but a COCO reference favours COCO-style joints.
    /// `cargo test --release -p app --bin live arm_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn arm_probe_against_cigpose() {
        use infer::cigpose::{CigPose, LimbStage};
        use infer::person_box::from_keypoints;
        use infer::upper_body::UpperBody;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ub = root.join("models/upper_body");
        let load = |n: &str| build_session(&ub.join(n), 4).unwrap().0;
        let hands = || Box::new(Mp2023Hands::new(load("palm_detection_mediapipe_2023feb.onnx"), load("handpose_estimation_mediapipe_2023feb.onnx")));
        let mut cig = CigPose::load(infer::engine::Engine::Ort, &ub.join("cigpose-m_coco_256x192.onnx"), 4).unwrap();
        let mut hmr = InstantHmr::load(infer::engine::Engine::Ort, &ub.join("instanthmr_joints.onnx"), 4).unwrap();
        let mut blaze = UpperBody::new(load("pose_detector.onnx"), load("pose_landmarks.onnx"), hands());
        // COCO shoulders, elbows, wrists (left, right) against the matching MHR70 and BlazePose indices.
        let coco = [5, 6, 7, 8, 9, 10];
        let mhr = [joint::L_SHOULDER, joint::R_SHOULDER, joint::L_ELBOW, joint::R_ELBOW, joint::L_WRIST, joint::R_WRIST];
        let bp = [11, 12, 13, 14, 15, 16];
        let kinds = ["shoulder", "shoulder", "elbow", "elbow", "wrist", "wrist"];
        let mut sums: std::collections::BTreeMap<(&str, &str), (f32, usize)> = Default::default();
        for name in ["hands_15", "hands_16", "hands_17", "hands_19"] {
            let path = root.join(format!("fixtures/private/upper_body_photos/{name}.png"));
            if !path.exists() {
                eprintln!("SKIP {name}");
                continue;
            }
            let rgb = image::open(path).unwrap().to_rgb8();
            let (w, h) = (rgb.width() as f32, rgb.height() as f32);
            let img = Image { rgb: rgb.as_raw(), w: rgb.width() as usize, h: rgb.height() as usize };
            let whole = PersonBox { x: 0.0, y: 0.0, w, h };
            let reference = cig.run(&img, whole).unwrap();
            println!("REF {name} {}", coco.iter().map(|&i| format!("{:.1},{:.1},{:.2}", reference[i].x, reference[i].y, reference[i].score)).collect::<Vec<_>>().join(" "));
            let sw = (reference[5].x - reference[6].x).hypot(reference[5].y - reference[6].y);
            let upper = from_keypoints(&reference, w, h).unwrap_or(whole);
            let hmr_whole = hmr.run(&img, whole).unwrap();
            let hmr_upper = hmr.run(&img, upper).unwrap();
            let blaze_out = blaze.run(rgb.as_raw(), rgb.width(), rgb.height()).unwrap();
            for (k, kind) in kinds.iter().enumerate() {
                let r = reference[coco[k]];
                if r.score < 0.5 || r.x < 0.0 || r.y < 0.0 || r.x > w || r.y > h {
                    continue;
                }
                let d = |x: f32, y: f32| (x - r.x).hypot(y - r.y) / sw;
                for (label, e) in [
                    ("InstantHMR, whole-image box", d(hmr_whole.xy[mhr[k]][0], hmr_whole.xy[mhr[k]][1])),
                    ("InstantHMR, upper-body box from joints", d(hmr_upper.xy[mhr[k]][0], hmr_upper.xy[mhr[k]][1])),
                    ("BlazePose port", d(blaze_out.pose[bp[k]].x, blaze_out.pose[bp[k]].y)),
                ] {
                    let s = sums.entry((label, *kind)).or_insert((0.0, 0));
                    s.0 += e;
                    s.1 += 1;
                }
            }
        }
        for label in ["InstantHMR, whole-image box", "InstantHMR, upper-body box from joints", "BlazePose port"] {
            let mean = |kind: &str| sums.get(&(label, kind)).map(|s| s.0 / s.1 as f32).unwrap_or(f32::NAN);
            let all: (f32, usize) = sums.iter().filter(|((l, _), _)| *l == label).fold((0.0, 0), |a, (_, s)| (a.0 + s.0, a.1 + s.1));
            println!("{label:42} shoulder {:.3} elbow {:.3} wrist {:.3} all {:.3} (n {})", mean("shoulder"), mean("elbow"), mean("wrist"), all.0 / all.1.max(1) as f32, all.1);
        }
    }
}