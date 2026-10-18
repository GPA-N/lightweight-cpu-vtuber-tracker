use std::sync::{Arc, Mutex};
use std::time::Instant;

use core::body::UpperBodyOutput;
use core::frame::{Frame, TrackingOutput};
use facelogic::crop::{rect_from_detection, rect_from_landmarks, warp_to_tensor, RotatedRect};
use facelogic::largest::{largest, FaceBox};
use facelogic::schedule::DetectionScheduler;
use pose::fit::fit_head_rotation;
use pose::mirror::apply_mirror;
use smoothing::face_lost::FaceLost;
use smoothing::quat::QuatFilter;
use smoothing::weights::WeightsFilter;

use crate::blendshapes::BlendshapeModel;
use crate::detector::{Detection, Detector};
use crate::landmarks::{landmarks_to_frame, LandmarkModel, LandmarkOutput};

pub trait FaceDetect {
    fn detect(&mut self, frame: &Frame) -> Result<Vec<Detection>, String>;
}

pub trait LandmarkRun {
    fn run(&mut self, tensor: &[f32]) -> Result<LandmarkOutput, String>;
}

pub trait BlendRun {
    fn run(&mut self, landmarks_px: &[[f32; 3]]) -> Result<[f32; 52], String>;
}

impl FaceDetect for Detector {
    fn detect(&mut self, frame: &Frame) -> Result<Vec<Detection>, String> {
        Detector::detect(self, frame).map_err(|e| e.to_string())
    }
}

impl LandmarkRun for LandmarkModel {
    fn run(&mut self, tensor: &[f32]) -> Result<LandmarkOutput, String> {
        LandmarkModel::run(self, tensor).map_err(|e| format!("{e:?}"))
    }
}

impl BlendRun for BlendshapeModel {
    fn run(&mut self, landmarks_px: &[[f32; 3]]) -> Result<[f32; 52], String> {
        BlendshapeModel::run(self, landmarks_px).map_err(|e| e.to_string())
    }
}

pub trait BodyRun {
    fn run(&mut self, frame: &Frame) -> Result<UpperBodyOutput, String>;
}

// design.md: blendshape weights One Euro, min_cutoff 1.0, beta 0.007.
const WEIGHTS_MIN_CUTOFF: f32 = 1.0;
const WEIGHTS_BETA: f32 = 0.007;

/// Per-stage durations in milliseconds, one entry per tracked frame.
#[derive(Debug, Default, Clone)]
pub struct StageTimes {
    /// Only frames where the detector ran (it is scheduled, not every frame).
    pub detect_ms: Vec<f32>,
    pub landmarks_ms: Vec<f32>,
    pub blend_ms: Vec<f32>,
    pub fit_ms: Vec<f32>,
    /// Mirror, One Euro filters.
    pub filter_ms: Vec<f32>,
    pub total_ms: Vec<f32>,
}

/// Shared handle so a caller can read stage timings while the tracker runs on another thread.
pub type StageStats = Arc<Mutex<StageTimes>>;

/// Upper bound on stored samples per stage.
const MAX_STAGE_SAMPLES: usize = 100_000;

fn push_capped(v: &mut Vec<f32>, x: f32) {
    if v.len() < MAX_STAGE_SAMPLES {
        v.push(x);
    }
}

/// One tracking step per frame: detect (scheduled), landmarks, blendshapes, head pose, smoothing.
pub struct Tracker<D, L, B> {
    detect: D,
    landmarks: L,
    blend: B,
    sched: DetectionScheduler,
    rect: Option<RotatedRect>,
    prev_px: Option<Vec<[f32; 3]>>,
    mirror: bool,
    weights: WeightsFilter,
    quat: QuatFilter,
    lost: FaceLost,
    last: Option<Instant>,
    pub fit_failures: u32,
    stage: Option<StageStats>,
    body: Option<Box<dyn BodyRun + Send>>,
}

impl<D: FaceDetect, L: LandmarkRun, B: BlendRun> Tracker<D, L, B> {
    pub fn new(detect: D, landmarks: L, blend: B) -> Self {
        Self {
            detect,
            landmarks,
            blend,
            sched: DetectionScheduler::default(),
            rect: None,
            prev_px: None,
            mirror: true,
            weights: WeightsFilter::new(WEIGHTS_MIN_CUTOFF, WEIGHTS_BETA),
            quat: QuatFilter::default(),
            lost: FaceLost::new(),
            last: None,
            fit_failures: 0,
            stage: None,
            body: None,
        }
    }

    /// Turns the upper-body stage on with `body`. Without it, `step_body` returns `None`.
    pub fn set_body(&mut self, body: Box<dyn BodyRun + Send>) {
        self.body = Some(body);
    }

    /// Runs the upper-body stage on one frame. `None` when the stage is off or the run fails.
    pub fn step_body(&mut self, frame: &Frame) -> Option<UpperBodyOutput> {
        self.body.as_mut()?.run(frame).ok()
    }

    /// Full-frame pixel landmarks from the last successful step, if any.
    pub fn last_landmarks(&self) -> Option<&[[f32; 3]]> {
        self.prev_px.as_deref()
    }

    /// Records per-stage timings into `stats` on every tracked frame.
    pub fn set_stage_stats(&mut self, stats: StageStats) {
        self.stage = Some(stats);
    }

    /// Face landmark presence below this counts as no face (the last face is held, then fades).
    pub fn set_min_presence(&mut self, min: f32) {
        self.sched.presence_threshold = min;
    }

    pub fn set_mirror(&mut self, mirror: bool) {
        self.mirror = mirror;
    }

    pub fn step(&mut self, frame: &Frame) -> Result<TrackingOutput, String> {
        let now = Instant::now();
        let total_start = now;
        let ms = |t: Instant| t.elapsed().as_secs_f32() * 1000.0;
        let mut times = StageTimes::default();
        let dt = self
            .last
            .map_or(1.0 / 30.0, |t| frame.arrived.duration_since(t).as_secs_f32())
            .clamp(1.0 / 120.0, 1.0 / 5.0);
        self.last = Some(frame.arrived);

        if self.sched.should_detect() {
            let t = Instant::now();
            let dets = self.detect.detect(frame)?;
            times.detect_ms.push(ms(t));
            let boxes: Vec<FaceBox> = dets
                .iter()
                .map(|d| FaceBox { x: d.box_norm.0, y: d.box_norm.1, w: d.box_norm.2, h: d.box_norm.3 })
                .collect();
            self.rect = largest(&boxes).map(|i| {
                let d = &dets[i];
                rect_from_detection(d.box_norm, d.keypoints[0], d.keypoints[1], frame.width, frame.height)
            });
        } else if let Some(px) = self.prev_px.as_ref().filter(|p| p.len() > 263) {
            let norm: Vec<(f32, f32)> =
                px.iter().map(|p| (p[0] / frame.width as f32, p[1] / frame.height as f32)).collect();
            self.rect = Some(rect_from_landmarks(&norm, 33, 263, frame.width, frame.height));
        }

        let Some(rect) = self.rect else {
            self.prev_px = None;
            self.sched.report(false, None);
            return Ok(self.lost.update(None, now));
        };

        let tensor = warp_to_tensor(&frame.bytes, frame.width, frame.height, &rect, 256);
        let t = Instant::now();
        let lm = self.landmarks.run(&tensor)?;
        let landmarks_ms = ms(t);
        let found = lm.presence >= self.sched.presence_threshold;
        self.sched.report(found, Some(lm.presence));
        if !found {
            self.prev_px = None;
            return Ok(self.lost.update(None, now));
        }

        let px = landmarks_to_frame(&lm.points, &rect);
        let t = Instant::now();
        let raw = self.blend.run(&px)?;
        let blend_ms = ms(t);
        let t = Instant::now();
        let rotation = match fit_head_rotation(&px) {
            Ok(fit) => fit.rotation,
            Err(_) => {
                self.fit_failures += 1;
                FaceLost::new().update(None, now).head_rotation
            }
        };
        let fit_ms = ms(t);
        self.prev_px = Some(px);

        let t = Instant::now();
        let (weights, rotation) = apply_mirror(self.mirror, &raw, rotation);
        let out = TrackingOutput {
            weights: self.weights.filter(&weights, dt),
            head_rotation: self.quat.filter(rotation, dt),
            face_found: true,
            ready: Instant::now(),
        };
        if let Some(stats) = &self.stage
            && let Ok(mut st) = stats.lock()
        {
            if let Some(d) = times.detect_ms.first() {
                push_capped(&mut st.detect_ms, *d);
            }
            push_capped(&mut st.landmarks_ms, landmarks_ms);
            push_capped(&mut st.blend_ms, blend_ms);
            push_capped(&mut st.fit_ms, fit_ms);
            push_capped(&mut st.filter_ms, ms(t));
            push_capped(&mut st.total_ms, ms(total_start));
        }
        Ok(self.lost.update(Some(&out), now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::frame::PixelFormat;
    use crate::detector::KEYPOINT_COUNT;
    use crate::landmarks::RawOutputs;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Log = Rc<RefCell<Vec<&'static str>>>;

    struct FakeDetect {
        log: Log,
        boxes: Vec<(f32, f32, f32, f32)>,
    }

    impl FaceDetect for FakeDetect {
        fn detect(&mut self, _frame: &Frame) -> Result<Vec<Detection>, String> {
            self.log.borrow_mut().push("detect");
            Ok(self.boxes.iter().map(|&b| face(b)).collect())
        }
    }

    struct FakeLandmarks {
        log: Log,
        presence: f32,
        points: usize,
        dx: f32,
        calls: usize,
    }

    impl LandmarkRun for FakeLandmarks {
        fn run(&mut self, _tensor: &[f32]) -> Result<LandmarkOutput, String> {
            self.log.borrow_mut().push("landmark");
            let shift = self.dx * self.calls as f32;
            self.calls += 1;
            Ok(LandmarkOutput {
                points: (0..self.points).map(|i| [i as f32 % 256.0 + shift, 100.0, 1.0]).collect(),
                presence: self.presence,
                raw_outputs: RawOutputs { logit: 0.0, probability: self.presence, sigmoid_logit: 0.5 },
            })
        }
    }

    struct FakeBlend {
        log: Log,
        weights: [f32; 52],
    }

    impl BlendRun for FakeBlend {
        fn run(&mut self, _landmarks_px: &[[f32; 3]]) -> Result<[f32; 52], String> {
            self.log.borrow_mut().push("blend");
            Ok(self.weights)
        }
    }

    fn face(b: (f32, f32, f32, f32)) -> Detection {
        Detection {
            box_norm: b,
            keypoints: std::array::from_fn(|i| (b.0 + 0.01 * i as f32, b.1)),
            score: 0.9,
        }
    }

    fn frame() -> Frame {
        Frame {
            width: 64,
            height: 64,
            format: PixelFormat::Rgb24,
            bytes: vec![0; 64 * 64 * 3],
            arrived: Instant::now(),
        }
    }

    fn make(
        boxes: Vec<(f32, f32, f32, f32)>,
        presence: f32,
        points: usize,
        weights: [f32; 52],
    ) -> (Tracker<FakeDetect, FakeLandmarks, FakeBlend>, Log) {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let t = Tracker::new(
            FakeDetect { log: log.clone(), boxes },
            FakeLandmarks { log: log.clone(), presence, points, dx: 0.0, calls: 0 },
            FakeBlend { log: log.clone(), weights },
        );
        (t, log)
    }

    #[test]
    fn frame_zero_runs_detect_then_landmark_then_blend() {
        let (mut t, log) = make(vec![(0.25, 0.25, 0.5, 0.5)], 0.9, 478, [0.0; 52]);
        t.step(&frame()).unwrap();
        assert_eq!(*log.borrow(), vec!["detect", "landmark", "blend"]);
    }

    #[test]
    fn frames_one_to_four_skip_detector() {
        let (mut t, log) = make(vec![(0.25, 0.25, 0.5, 0.5)], 0.9, 478, [0.0; 52]);
        t.step(&frame()).unwrap();
        log.borrow_mut().clear();
        for _ in 1..=4 {
            t.step(&frame()).unwrap();
        }
        let calls = log.borrow().clone();
        assert_eq!(calls.len(), 8);
        assert!(calls.iter().all(|c| *c != "detect"));
        assert_eq!(calls[0..2], ["landmark", "blend"]);
    }

    #[test]
    fn no_face_reports_not_found_and_skips_landmarks_and_blend() {
        let (mut t, log) = make(vec![], 0.9, 478, [0.0; 52]);
        let out = t.step(&frame()).unwrap();
        assert!(!out.face_found);
        assert_eq!(*log.borrow(), vec!["detect"]);
    }

    #[test]
    fn largest_face_sets_the_crop() {
        let small = (0.0, 0.0, 0.1, 0.1);
        let large = (0.5, 0.5, 0.4, 0.4);
        let (mut t, _log) = make(vec![small, large], 0.9, 478, [0.0; 52]);
        t.step(&frame()).unwrap();
        let rect = t.rect.expect("crop set from face");
        assert!(rect.cx > 32.0, "crop centre {} should be on the large face", rect.cx);
    }

    #[test]
    fn low_presence_reports_not_found_and_forces_detection_next_frame() {
        let (mut t, log) = make(vec![(0.25, 0.25, 0.5, 0.5)], 0.3, 478, [0.0; 52]);
        let out = t.step(&frame()).unwrap();
        assert!(!out.face_found);
        log.borrow_mut().clear();
        t.step(&frame()).unwrap();
        assert_eq!(log.borrow()[0], "detect");
    }

    #[test]
    fn mirror_on_swaps_left_and_right_weights() {
        let left = core::names::index_of("eyeBlinkLeft").unwrap();
        let right = core::names::index_of("eyeBlinkRight").unwrap();
        let mut w = [0.0; 52];
        w[left] = 1.0;
        let (mut t, _log) = make(vec![(0.25, 0.25, 0.5, 0.5)], 0.9, 478, w);
        let out = t.step(&frame()).unwrap();
        assert_eq!(out.weights[right], 1.0);
        assert_eq!(out.weights[left], 0.0);
    }

    #[test]
    fn mirror_off_keeps_left_and_right_weights() {
        let left = core::names::index_of("eyeBlinkLeft").unwrap();
        let right = core::names::index_of("eyeBlinkRight").unwrap();
        let mut w = [0.0; 52];
        w[left] = 1.0;
        let (mut t, _log) = make(vec![(0.25, 0.25, 0.5, 0.5)], 0.9, 478, w);
        t.set_mirror(false);
        let out = t.step(&frame()).unwrap();
        assert_eq!(out.weights[left], 1.0);
        assert_eq!(out.weights[right], 0.0);
    }

    #[test]
    fn weights_stay_within_zero_to_one() {
        let mut w = [0.5; 52];
        w[0] = 3.0;
        w[1] = -2.0;
        let (mut t, _log) = make(vec![(0.25, 0.25, 0.5, 0.5)], 0.9, 478, w);
        let out = t.step(&frame()).unwrap();
        assert!(out.weights.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn short_landmark_list_counts_fit_failure_and_keeps_identity() {
        let (mut t, _log) = make(vec![(0.25, 0.25, 0.5, 0.5)], 0.9, 10, [0.0; 52]);
        let out = t.step(&frame()).unwrap();
        assert_eq!(t.fit_failures, 1);
        assert!(out.head_rotation.angle().abs() < 1e-6);
    }

    #[test]
    fn frame_one_crop_comes_from_previous_landmarks() {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let mut t = Tracker::new(
            FakeDetect { log: log.clone(), boxes: vec![(0.25, 0.25, 0.5, 0.5)] },
            FakeLandmarks { log: log.clone(), presence: 0.9, points: 478, dx: 8.0, calls: 0 },
            FakeBlend { log: log.clone(), weights: [0.0; 52] },
        );
        t.step(&frame()).unwrap();
        let det_rect = t.rect.unwrap();
        let prev = t.prev_px.clone().unwrap();
        t.step(&frame()).unwrap();
        let norm: Vec<(f32, f32)> = prev.iter().map(|p| (p[0] / 64.0, p[1] / 64.0)).collect();
        assert_eq!(t.rect, Some(rect_from_landmarks(&norm, 33, 263, 64, 64)));
        assert_ne!(t.rect, Some(det_rect));
    }

    #[test]
    fn weights_filter_uses_design_constants() {
        assert_eq!(WEIGHTS_MIN_CUTOFF, 1.0);
        assert_eq!(WEIGHTS_BETA, 0.007);
    }

    #[test]
    fn body_stage_off_step_body_returns_none_and_skips_fake_runners() {
        let (mut t, log) = make(vec![(0.25, 0.25, 0.5, 0.5)], 0.9, 478, [0.0; 52]);
        assert!(t.step_body(&frame()).is_none());
        assert!(log.borrow().is_empty(), "face runners called: {:?}", log.borrow());
    }

    #[test]
    fn keypoint_count_is_used_by_fake_detections() {
        assert_eq!(face((0.0, 0.0, 0.1, 0.1)).keypoints.len(), KEYPOINT_COUNT);
    }
}
