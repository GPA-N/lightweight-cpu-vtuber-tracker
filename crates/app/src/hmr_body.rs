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
use core::body::{HandLandmarks, PoseLandmark, SubjectSide, UpperBodyOutput, HAND_POINTS, POSE_POINTS};
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
const SMOOTH_2D: (f32, f32) = (1.0, 0.04);
const SMOOTH_3D: (f32, f32) = (1.0, 8.0);

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

/// One Euro settings for the hand points, in pixels (the hand model gives z on the x scale too):
/// minimum cutoff in Hz, and beta (cutoff rise per pixel per second of speed).
const HAND_MINCUTOFF: f32 = 1.5;
const HAND_BETA: f32 = 0.02;
/// A side missing for longer than this starts over when its hand comes back, instead of gliding from the old place.
const HAND_MAX_ABSENT_S: f32 = 0.3;

/// Filters of one hand side: a One Euro filter per coordinate of each of the 21 points.
struct SideFilters {
    xyz: [[OneEuro; 3]; HAND_POINTS],
    /// When this side last had a hand.
    last: Option<std::time::Instant>,
}

impl SideFilters {
    fn new() -> Self {
        Self { xyz: std::array::from_fn(|_| std::array::from_fn(|_| OneEuro::new(HAND_MINCUTOFF, HAND_BETA))), last: None }
    }
}

/// Smooths the hand points of each side with its own filters, so left and right never mix.
pub struct HandSmoother {
    /// Index 0 is the left side, 1 the right (see `side_index`).
    sides: [SideFilters; 2],
}

impl Default for HandSmoother {
    fn default() -> Self {
        Self { sides: [SideFilters::new(), SideFilters::new()] }
    }
}

fn side_index(side: SubjectSide) -> usize {
    match side {
        SubjectSide::Left => 0,
        SubjectSide::Right => 1,
    }
}

impl HandSmoother {
    fn apply(&mut self, hands: &mut [HandLandmarks]) {
        self.apply_at(hands, std::time::Instant::now());
    }

    /// Filters the first hand of each side in `hands`. A side with no hand this frame keeps its filters.
    fn apply_at(&mut self, hands: &mut [HandLandmarks], now: std::time::Instant) {
        for side in [SubjectSide::Left, SubjectSide::Right] {
            let Some(hand) = hands.iter_mut().find(|h| h.side == side) else { continue };
            let f = &mut self.sides[side_index(side)];
            let gap = f.last.map_or(f32::INFINITY, |t| (now - t).as_secs_f32());
            if gap > HAND_MAX_ABSENT_S {
                *f = SideFilters::new();
            }
            // The demo's fallback for a frame without elapsed time; the first sample ignores dt anyway.
            let dt = if gap > 0.0 && gap <= HAND_MAX_ABSENT_S { gap } else { 1.0 / 30.0 };
            for (p, filters) in hand.points.iter_mut().zip(f.xyz.iter_mut()) {
                for k in 0..3 {
                    p[k] = filters[k].filter(p[k], dt);
                }
            }
            f.last = Some(now);
        }
    }
}

pub struct HmrBody {
    smooth: JointSmoother,
    /// One Euro filters for the hand points, applied after the wrists are refined.
    hand_smooth: HandSmoother,
    lead: LeadFilter,
    camera: CameraClock,
    hmr: InstantHmr,
    boxes: PersonBoxTracker,
    /// MediaPipe hand stage. The fingers come from this hand model only.
    hands: Box<dyn HandStage + Send>,
    /// Pose of the last good frame. The hand stage runs on it while the body model runs on the new frame.
    prev_pose: Option<[PoseLandmark; POSE_POINTS]>,
    mirror: bool,
    /// Debug drawing of the last frame, in frame pixels.
    pub overlay: crate::debug_overlay::Overlay,
    /// Milliseconds the last frame spent in the body model and in the hand stage. The fingers come from the hand model only.
    pub last_ms: (f32, f32),
    /// Smoothed joints of the last frame, for the body bench only.
    #[cfg(test)]
    pub last_joints: Option<Joints>,
}

impl HmrBody {
    pub fn new(hmr: InstantHmr, hands: Box<dyn HandStage + Send>, mirror: bool) -> Self {
        Self { smooth: JointSmoother::default(), hand_smooth: HandSmoother::default(), lead: LeadFilter::default(), camera: CameraClock::default(), hmr, boxes: PersonBoxTracker::new(), hands, prev_pose: None, mirror, overlay: Default::default(), last_ms: (0.0, 0.0), #[cfg(test)] last_joints: None }
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
        // With a previous pose, the hand stage runs beside the body model. Otherwise (first frame, or after an error) it runs after it.
        let prev = self.prev_pose.take();
        let overlapped = prev.is_some();
        let (hmr, stage) = (&mut self.hmr, &mut self.hands);
        let (body, early_hands) = match prev {
            Some(prev_pose) => {
                let (body, hand) = overlap(|| timed(|| hmr.run(&img, person)), || timed(|| stage.run(&img, &prev_pose)));
                (body, Some(hand))
            }
            None => (timed(|| hmr.run(&img, person)), None),
        };
        let (joints, model_ms) = body;
        let mut joints = match joints {
            Ok(j) => j,
            Err(e) => {
                self.boxes.update(None, fw, fh);
                return Err(e);
            }
        };
        self.smooth.apply(&mut joints);
        #[cfg(test)]
        {
            self.last_joints = Some(joints.clone());
        }
        self.boxes.update(Some(&box_keypoints(&joints)), fw, fh);

        let mut pose = pose_landmarks(&joints);
        let (hand_res, hand_ms) = match early_hands {
            Some(hand) => hand,
            None => timed(|| stage.run(&img, &pose)),
        };
        let mut hands = hand_res?;
        refine_wrists(&mut pose, &hands, hand_min);
        self.hand_smooth.apply(&mut hands);
        self.last_ms = (model_ms, hand_ms);
        self.prev_pose = Some(pose);
        let out = UpperBodyOutput { width, height, pose, presence: 1.0, hands, hand_model: stage.label() };
        self.overlay = crate::debug_overlay::from_upper_body(&out, vis_min, hand_min);
        let mut body = to_avatar_space(&out, self.mirror, vis_min, hand_min);
        drop_arms_without_elbow(&mut body);
        // The pose describes the person about one model run ago; the camera frame is older still.
        // Overlapped stages cost the longer of the two; sequential stages add up.
        let (model_ms, hand_ms) = self.last_ms;
        let stage_ms = if overlapped { model_ms.max(hand_ms) } else { model_ms + hand_ms };
        let age = stage_ms / 1000.0 + self.camera.age_s();
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

/// Runs `side` on a scoped thread while `main` runs on the calling thread, then returns both results.
fn overlap<M, S>(main: impl FnOnce() -> M, side: impl FnOnce() -> S + Send) -> (M, S)
where
    S: Send,
{
    std::thread::scope(|scope| {
        let side = scope.spawn(side);
        let m = main();
        (m, side.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
    })
}

/// Runs `f` and returns its result with the milliseconds it took.
fn timed<T>(f: impl FnOnce() -> T) -> (T, f32) {
    let t = std::time::Instant::now();
    let r = f();
    (r, t.elapsed().as_secs_f32() * 1000.0)
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

    /// Body stage period in seconds (about 18 fps).
    const FRAME_S: f32 = 0.055;

    /// Local copy of the smoothing crate's test generator (that one is not visible from this crate).
    struct Lcg(u64);

    impl Lcg {
        /// Uniform in [-1, 1).
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        }

        /// Noise with standard deviation `sigma` (uniform on [-a, a] has std a / sqrt 3).
        fn noise(&mut self, sigma: f32) -> f32 {
            self.next() * sigma * 3f32.sqrt()
        }
    }

    fn std_dev(v: &[f32]) -> f32 {
        let n = v.len() as f32;
        let mean = v.iter().sum::<f32>() / n;
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n).sqrt()
    }

    fn at_frame(t0: std::time::Instant, i: usize) -> std::time::Instant {
        t0 + std::time::Duration::from_secs_f32(i as f32 * FRAME_S)
    }

    fn hand_at(side: SubjectSide, at: [f32; 3]) -> HandLandmarks {
        HandLandmarks { points: [at; HAND_POINTS], score: 1.0, handedness: 1.0, side }
    }

    /// Feeds a 0 input for 20 frames, then `step` for the rest. Returns the time from the step until
    /// the output is at 90% of `step`. `feed(frame, value)` returns the output for that frame.
    fn time_to_90(mut feed: impl FnMut(usize, f32) -> f32, step: f32) -> f32 {
        for i in 0..20 {
            feed(i, 0.0);
        }
        for j in 20..60 {
            if feed(j, step) >= 0.9 * step {
                return (j - 19) as f32 * FRAME_S;
            }
        }
        f32::INFINITY
    }

    #[test]
    fn still_hand_jitter_is_halved() {
        let truth = [120.0f32, 80.0, 5.0];
        let mut rng = Lcg(7);
        let mut s = HandSmoother::default();
        let t0 = std::time::Instant::now();
        let (mut input, mut output) = (Vec::new(), Vec::new());
        for i in 0..600 {
            let noisy = [0usize, 1, 2].map(|k| truth[k] + rng.noise(3.0));
            let mut hands = vec![hand_at(SubjectSide::Left, noisy)];
            s.apply_at(&mut hands, at_frame(t0, i));
            if i >= 100 {
                for k in 0..3 {
                    input.push(noisy[k] - truth[k]);
                    output.push(hands[0].points[0][k] - truth[k]);
                }
            }
        }
        let ratio = std_dev(&output) / std_dev(&input);
        println!("hand still jitter ratio {ratio:.3}");
        assert!(ratio <= 0.5, "hand jitter ratio {ratio:.3} is above 0.5");
    }

    #[test]
    fn hand_step_of_100_px_reaches_90_percent_within_120_ms() {
        for axis in 0..3 {
            let mut s = HandSmoother::default();
            let t0 = std::time::Instant::now();
            let t = time_to_90(
                |i, v| {
                    let mut at = [0.0f32; 3];
                    at[axis] = v;
                    let mut hands = vec![hand_at(SubjectSide::Right, at)];
                    s.apply_at(&mut hands, at_frame(t0, i));
                    hands[0].points[0][axis]
                },
                100.0,
            );
            println!("hand step axis {axis}: 90% in {:.0} ms", t * 1000.0);
            assert!(t <= 0.120, "axis {axis}: 90% after {:.0} ms", t * 1000.0);
        }
    }

    #[test]
    fn hand_back_after_a_gap_starts_at_its_new_place() {
        let mut s = HandSmoother::default();
        let t0 = std::time::Instant::now();
        for i in 0..30 {
            let mut hands = vec![hand_at(SubjectSide::Left, [100.0, 100.0, 0.0])];
            s.apply_at(&mut hands, at_frame(t0, i));
        }
        // Absent for 0.5 s, more than HAND_MAX_ABSENT_S, so the new place must not be eased into.
        let back = at_frame(t0, 29) + std::time::Duration::from_millis(555);
        let mut hands = vec![hand_at(SubjectSide::Left, [300.0, 50.0, 0.0])];
        s.apply_at(&mut hands, back);
        assert_eq!(hands[0].points[0], [300.0, 50.0, 0.0], "after a gap the filter starts over");
    }

    #[test]
    fn left_and_right_hands_filter_on_their_own() {
        let t0 = std::time::Instant::now();
        let mut both = HandSmoother::default();
        let mut right_only = HandSmoother::default();
        for i in 0..80 {
            let right = hand_at(SubjectSide::Right, [200.0 + 0.5 * i as f32, 90.0, 3.0]);
            // The left hand is seen for 20 frames only, then gone, so its filters reset.
            let mut hands = Vec::new();
            if i < 20 {
                hands.push(hand_at(SubjectSide::Left, [10.0 + i as f32, 0.0, 0.0]));
            }
            hands.push(right.clone());
            both.apply_at(&mut hands, at_frame(t0, i));
            let mut solo = vec![right];
            right_only.apply_at(&mut solo, at_frame(t0, i));
            let got = hands.iter().find(|h| h.side == SubjectSide::Right).expect("right hand");
            assert_eq!(got.points, solo[0].points, "frame {i}: right hand depends on left");
        }
    }

    /// Arm joint smoothing: still wrist with 3 px (xy) and 0.01 m (xyz) noise.
    #[test]
    fn still_arm_jitter_is_halved() {
        let mut rng = Lcg(11);
        let mut s = JointSmoother::default();
        let t0 = std::time::Instant::now();
        let base = joints();
        let (xy0, xyz0) = (base.xy[joint::L_WRIST], base.xyz[joint::L_WRIST]);
        let (mut xy_in, mut xy_out, mut xyz_in, mut xyz_out) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for i in 0..600 {
            let mut j = joints();
            let xy = [0usize, 1].map(|k| xy0[k] + rng.noise(3.0));
            let xyz = [0usize, 1, 2].map(|k| xyz0[k] + rng.noise(0.01));
            j.xy[joint::L_WRIST] = xy;
            j.xyz[joint::L_WRIST] = xyz;
            s.apply_at(&mut j, at_frame(t0, i));
            if i >= 100 {
                for k in 0..2 {
                    xy_in.push(xy[k] - xy0[k]);
                    xy_out.push(j.xy[joint::L_WRIST][k] - xy0[k]);
                }
                for k in 0..3 {
                    xyz_in.push(xyz[k] - xyz0[k]);
                    xyz_out.push(j.xyz[joint::L_WRIST][k] - xyz0[k]);
                }
            }
        }
        let (r2, r3) = (std_dev(&xy_out) / std_dev(&xy_in), std_dev(&xyz_out) / std_dev(&xyz_in));
        println!("arm still jitter ratio xy {r2:.3} xyz {r3:.3}");
        assert!(r2 <= 0.5, "arm xy jitter ratio {r2:.3}");
        assert!(r3 <= 0.5, "arm xyz jitter ratio {r3:.3}");
    }

    #[test]
    fn arm_step_reaches_90_percent_within_120_ms() {
        let mut s = JointSmoother::default();
        let t0 = std::time::Instant::now();
        let t = time_to_90(
            |i, v| {
                let mut j = joints();
                j.xy[joint::L_WRIST] = [v, 0.0];
                s.apply_at(&mut j, at_frame(t0, i));
                j.xy[joint::L_WRIST][0]
            },
            100.0,
        );
        println!("arm xy step 100 px: 90% in {:.0} ms", t * 1000.0);
        assert!(t <= 0.120, "arm xy step: 90% after {:.0} ms", t * 1000.0);
    }

    #[test]
    fn arm_3d_step_reaches_90_percent_within_120_ms() {
        let mut s = JointSmoother::default();
        let t0 = std::time::Instant::now();
        let t = time_to_90(
            |i, v| {
                let mut j = joints();
                j.xyz[joint::L_WRIST] = [v, 0.0, 0.0];
                s.apply_at(&mut j, at_frame(t0, i));
                j.xyz[joint::L_WRIST][0]
            },
            0.2,
        );
        println!("arm xyz step 0.2 m: 90% in {:.0} ms", t * 1000.0);
        assert!(t <= 0.120, "arm xyz step: 90% after {:.0} ms", t * 1000.0);
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
    fn overlap_runs_side_while_main_is_busy() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = AtomicBool::new(false);
        let (main_saw, side_out) = overlap(
            || {
                let t = std::time::Instant::now();
                while !flag.load(Ordering::SeqCst) && t.elapsed() < std::time::Duration::from_secs(2) {
                    std::hint::spin_loop();
                }
                flag.load(Ordering::SeqCst)
            },
            || {
                flag.store(true, Ordering::SeqCst);
                7
            },
        );
        assert!(main_saw, "side did not run while main was busy");
        assert_eq!(side_out, 7);
    }

    #[test]
    fn the_box_tracker_gets_the_head_shoulders_elbows_and_wrists() {
        let kps = box_keypoints(&joints());
        assert_eq!((kps[9].x, kps[9].y, kps[9].score), (10.0, 20.0, 1.0), "COCO left wrist is MHR 62");
        assert_eq!(kps[6].x, 50.0);
        assert_eq!(kps[11].score, 0.0, "hips stay unused");
    }

    #[test]
    fn a_phantom_skeleton_box_is_dropped_when_a_person_is_found_elsewhere() {
        // The empty scene's skeleton sits in the top-left corner; the face of the person who walks in is far from it.
        let mut boxes = infer::person_box::PersonBoxTracker::new();
        boxes.update(Some(&box_keypoints(&joints())), 640.0, 480.0);
        let face = infer::person_box::PersonBox { x: 500.0, y: 300.0, w: 40.0, h: 50.0 };
        let person = boxes.next(Some(face), 640.0, 480.0);
        assert_eq!(person, Some(infer::person_box::from_face(face, 640.0, 480.0)));
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

    /// Body bench over `fixtures/private/upper_body_photos`: one fresh `HmrBody` per photo, 3 warm-up runs then 30
    /// timed runs, no face box, thresholds 0.5. Prints median and p95 of the body, hand and whole-run times, the peak
    /// working set, and the mean joint error in cm against `%TEMP%\hmr_baseline_joints.json`. The baseline is written
    /// from the first run of each photo when the file does not exist. Env: `VT_ENGINE` (default as the app:
    /// ort when unset), `VT_THREADS` (default 5), `VT_HAND_THREADS` (default 2, hand sessions only), `VT_RESULTS` (append one TSV row), `VT_ID`, `VT_CHANGE`
    /// (`cargo test --release -p app --bin live body_stage_bench -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn body_stage_bench() {
        const WARM: usize = 3;
        const TIMED: usize = 30;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = root.join("fixtures/private/upper_body_photos");
        let ub = root.join("models/upper_body");
        let engine = resolve_engine();
        let threads: usize = std::env::var("VT_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
        let hand_threads: usize = std::env::var("VT_HAND_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
        let mut photos: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "png" | "jpg")))
            .collect();
        photos.sort();
        if photos.is_empty() || !ub.join("instanthmr_joints.onnx").exists() {
            eprintln!("SKIP body_stage_bench: photos or models missing");
            return;
        }

        let baseline_path = std::env::temp_dir().join("hmr_baseline_joints.json");
        let baseline_exists = baseline_path.exists();
        let baseline = if baseline_exists { parse_joint_file(&std::fs::read_to_string(&baseline_path).unwrap()) } else { Default::default() };
        let mut first: std::collections::BTreeMap<String, Vec<f32>> = Default::default();
        let load = |n: &str, t: usize| build_session(&ub.join(n), t).unwrap().0;
        let (mut model_ms, mut hand_ms, mut whole_ms) = (Vec::new(), Vec::new(), Vec::new());
        for photo in &photos {
            let name = photo.file_name().unwrap().to_string_lossy().into_owned();
            let rgb = image::open(photo).unwrap_or_else(|e| panic!("{name}: {e}")).to_rgb8();
            let (w, h) = rgb.dimensions();
            let hands = Box::new(Mp2023Hands::new(load("palm_detection_mediapipe_2023feb.onnx", hand_threads), load("handpose_estimation_mediapipe_2023feb.onnx", hand_threads)));
            let hmr = InstantHmr::load(engine, &ub.join("instanthmr_joints.onnx"), threads).unwrap();
            let mut body = HmrBody::new(hmr, hands, false);
            for run in 0..WARM + TIMED {
                let started = Instant::now();
                body.run(rgb.as_raw(), w, h, None, 0.5, 0.5).unwrap_or_else(|e| panic!("{name}: {e}"));
                let whole = started.elapsed().as_secs_f32() * 1000.0;
                if run == 0 {
                    let j = body.last_joints.as_ref().unwrap();
                    first.insert(name.clone(), SLOTS.iter().flat_map(|(_, m)| j.xyz[*m]).collect());
                }
                if run >= WARM {
                    model_ms.push(body.last_ms.0);
                    hand_ms.push(body.last_ms.1);
                    whole_ms.push(whole);
                }
            }
        }

        let mut err = (0.0f32, 0usize);
        if baseline_exists {
            for (name, now) in &first {
                let Some(base) = baseline.get(name) else { continue };
                for (a, b) in now.chunks_exact(3).zip(base.chunks_exact(3)) {
                    err.0 += ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() * 100.0;
                    err.1 += 1;
                }
            }
        } else {
            std::fs::write(&baseline_path, joint_file_text(&first)).unwrap();
            println!("baseline saved: {}", baseline_path.display());
        }
        let joint_err = (err.1 > 0).then(|| err.0 / err.1 as f32);

        fn pct(v: &mut [f32], p: usize) -> f32 {
            v.sort_by(|a, b| a.total_cmp(b));
            v[(v.len() * p / 100).min(v.len() - 1)]
        }
        let (bm, bp) = (pct(&mut model_ms, 50), pct(&mut model_ms, 95));
        let (hm, hp) = (pct(&mut hand_ms, 50), pct(&mut hand_ms, 95));
        let (wm, wp) = (pct(&mut whole_ms, 50), pct(&mut whole_ms, 95));
        let peak = timing::memory::process_memory().map(|m| m.peak_working_set_mb).unwrap_or(f64::NAN);
        println!("body stage bench ({engine:?}, {threads} threads): {} photos, {TIMED} timed runs each", photos.len());
        println!("| stage | median ms | p95 ms |");
        println!("| --- | --- | --- |");
        println!("| body model | {bm:.1} | {bp:.1} |");
        println!("| hand stage | {hm:.1} | {hp:.1} |");
        println!("| whole run | {wm:.1} | {wp:.1} |");
        println!("peak working set {peak:.0} MB");
        match joint_err {
            Some(e) => println!("joint error vs baseline {e:.2} cm (mean over {} joints)", err.1),
            None => println!("joint error: no baseline to compare"),
        }

        if let Ok(path) = std::env::var("VT_RESULTS") {
            let row = [
                std::env::var("VT_ID").unwrap_or_else(|_| "run".into()),
                std::env::var("VT_CHANGE").unwrap_or_default(),
                format!("{bm:.1}"),
                format!("{bp:.1}"),
                format!("{hm:.1}"),
                format!("{wm:.1}"),
                format!("{wp:.1}"),
                format!("{peak:.0}"),
                joint_err.map(|e| format!("{e:.2}")).unwrap_or_default(),
                String::new(),
            ]
            .join("\t");
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            writeln!(f, "{row}").unwrap();
            println!("row appended to {path}");
        }
    }

    /// Baseline file: one line per photo, `  "name": [x, y, z, ...]`, the 13 SLOTS joints in metres.
    fn joint_file_text(map: &std::collections::BTreeMap<String, Vec<f32>>) -> String {
        let rows: Vec<String> = map
            .iter()
            .map(|(k, v)| format!("  \"{k}\": [{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")))
            .collect();
        format!("{{\n{}\n}}\n", rows.join(",\n"))
    }

    fn parse_joint_file(text: &str) -> std::collections::BTreeMap<String, Vec<f32>> {
        text.lines()
            .filter_map(|line| {
                let (key, rest) = line.trim().strip_prefix('"')?.split_once("\": [")?;
                let nums = rest.trim_end_matches(',').trim_end_matches(']');
                Some((key.to_string(), nums.split(',').map(|t| t.trim().parse().unwrap()).collect()))
            })
            .collect()
    }
}
