//! Parity and timing of the ONNX Runtime and OpenVINO backends on the recorded golden_take frames.
//!
//! Run in release for meaningful timings:
//! `cargo test --release -p infer --test engine_parity -- --nocapture`
//!
//! Needs `fixtures/private/golden_take`, `models/` and the OpenVINO CPU runtime DLLs. The DLL folder
//! defaults to the throwaway Python venv and can be overridden with `VT_OPENVINO_LIBS`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use facelogic::crop::RotatedRect;
use infer::engine::{load_model, parse_engine, Engine, ModelRun};
use infer::upper_body::{
    decode_detections, letterbox_rect, nms, palm_roi, pose_roi, ssd_anchors, warp_black, Image,
};

const THREADS: usize = 4;
const TOLERANCE: f32 = 1e-3;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn add_openvino_to_path() {
    let libs = std::env::var_os("VT_OPENVINO_LIBS").map(PathBuf::from).unwrap_or_else(|| {
        root().join("fixtures/private/venv_rt/Lib/site-packages/openvino/libs")
    });
    let mut paths = vec![libs];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    // SAFETY: called once at the start of the only test in this binary, before other threads exist.
    unsafe { std::env::set_var("PATH", std::env::join_paths(paths).unwrap()) };
}

struct Pair {
    ort: Box<dyn ModelRun + Send>,
    ov: Box<dyn ModelRun + Send>,
    ort_ms: Vec<f64>,
    ov_ms: Vec<f64>,
    max_err: f32,
    /// Worst error divided by max(1, peak magnitude of that output): the 1e-3 criterion.
    max_rel: f32,
    worst: String,
}

impl Pair {
    fn new(path: &Path) -> Self {
        Self {
            ort: load_model(Engine::Ort, path, THREADS).expect("ort load"),
            ov: load_model(Engine::OpenVino, path, THREADS).expect("openvino load"),
            ort_ms: vec![],
            ov_ms: vec![],
            max_err: 0.0,
            max_rel: 0.0,
            worst: String::new(),
        }
    }

    /// Runs both backends, compares every output by name, returns the ort outputs.
    fn run(&mut self, input: &[f32], shape: &[usize]) -> Vec<infer::engine::NamedOutput> {
        let t = Instant::now();
        let a = self.ort.run(input, shape).expect("ort run");
        self.ort_ms.push(t.elapsed().as_secs_f64() * 1e3);
        let t = Instant::now();
        let b = self.ov.run(input, shape).expect("ov run");
        self.ov_ms.push(t.elapsed().as_secs_f64() * 1e3);
        for o in &a {
            let p = b.iter().find(|x| x.name == o.name).unwrap_or_else(|| panic!("openvino lacks output {}", o.name));
            assert_eq!(o.shape, p.shape, "shape of {}", o.name);
            let peak = o.data.iter().fold(1.0f32, |m, v| m.max(v.abs()));
            for (x, y) in o.data.iter().zip(&p.data) {
                let e = (x - y).abs();
                if e > self.max_err {
                    self.max_err = e;
                    self.worst = format!("{} (peak {peak:.0})", o.name);
                }
                self.max_rel = self.max_rel.max(e / peak);
            }
        }
        a
    }

    fn median(v: &[f64]) -> f64 {
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.total_cmp(b));
        s.get(s.len() / 2).copied().unwrap_or(f64::NAN)
    }

    fn report(&self, name: &str) -> (f64, f64) {
        let (o, v) = (Self::median(&self.ort_ms), Self::median(&self.ov_ms));
        println!("{name:<16} runs {:>4}  ort {o:6.2} ms  openvino {v:6.2} ms  max abs err {:.2e} in {}, max rel err {:.2e}", self.ort_ms.len(), self.max_err, self.worst, self.max_rel);
        (o, v)
    }
}

fn out<'a>(o: &'a [infer::engine::NamedOutput], name: &str) -> &'a [f32] {
    &o.iter().find(|x| x.name == name).unwrap_or_else(|| panic!("no output {name}")).data
}

#[test]
fn engine_parity() {
    add_openvino_to_path();
    let frames_dir = root().join("fixtures/private/golden_take");
    let mut frames: Vec<PathBuf> = std::fs::read_dir(&frames_dir)
        .expect("golden_take missing")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
        .collect();
    frames.sort();
    assert_eq!(frames.len(), 236, "golden_take frame count");

    let models = root().join("models");
    let ub = models.join("upper_body");
    let mut face = Pair::new(&models.join("face_landmarks.onnx"));
    let mut pose_det = Pair::new(&ub.join("pose_detector.onnx"));
    let mut pose_lm = Pair::new(&ub.join("pose_landmarks.onnx"));
    let mut palm = Pair::new(&ub.join("palm_detection_mediapipe_2023feb.onnx"));
    let mut hand = Pair::new(&ub.join("handpose_estimation_mediapipe_2023feb.onnx"));
    let pose_anchors = ssd_anchors(224, &[8, 16, 32, 32, 32]);
    let palm_anchors = ssd_anchors(192, &[8, 16, 16, 16]);

    // Two warm-up passes per model before timing starts (first run compiles kernels).
    for round in 0..(frames.len() + 2) {
        let path = &frames[round.saturating_sub(2) % frames.len()];
        let rgb = image::open(path).expect("decode").to_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);
        let img = Image { rgb: rgb.as_raw(), w, h };
        let warm = round < 2;
        let before = [face.ort_ms.len(), pose_det.ort_ms.len(), pose_lm.ort_ms.len(), palm.ort_ms.len(), hand.ort_ms.len()];

        // face_landmarks: centred square crop (face detector skipped; numerics are what matters).
        let side = h as f32;
        let face_rect = RotatedRect { cx: w as f32 / 2.0, cy: h as f32 / 2.0, width: side, height: side, angle: 0.0 };
        face.run(&warp_black(&img, &face_rect, 256, false), &[1, 256, 256, 3]);

        // pose_detector -> pose ROI -> pose_landmarks.
        let d = pose_det.run(&warp_black(&img, &letterbox_rect(w, h), 224, true), &[1, 224, 224, 3]);
        let dets = decode_detections(out(&d, "Identity"), out(&d, "Identity_1"), &pose_anchors, 224, 4, 0.5).unwrap();
        let roi = nms(dets, 0.3).first().and_then(|d| pose_roi(d, w, h)).unwrap_or_else(|| {
            let s = letterbox_rect(w, h);
            RotatedRect { width: s.width * 1.25, height: s.height * 1.25, ..s }
        });
        pose_lm.run(&warp_black(&img, &roi, 256, false), &[1, 256, 256, 3]);

        // palm -> hand ROI -> handpose (when a palm is found).
        let p = palm.run(&warp_black(&img, &letterbox_rect(w, h), 192, false), &[1, 192, 192, 3]);
        let dets = decode_detections(out(&p, "Identity"), out(&p, "Identity_1"), &palm_anchors, 192, 7, 0.5).unwrap();
        for det in nms(dets, 0.3).iter().take(2) {
            if let Some(rect) = palm_roi(det, w, h) {
                hand.run(&warp_black(&img, &rect, 224, false), &[1, 224, 224, 3]);
            }
        }

        if warm {
            for (m, n) in [&mut face, &mut pose_det, &mut pose_lm, &mut palm, &mut hand].into_iter().zip(before) {
                m.ort_ms.truncate(n);
                m.ov_ms.truncate(n);
            }
        }
    }

    println!("threads {THREADS}, {} frames", frames.len());
    let (face_ort, face_ov) = face.report("face_landmarks");
    pose_det.report("pose_detector");
    pose_lm.report("pose_landmarks");
    palm.report("palm_detection");
    hand.report("handpose");
    for m in [&face, &pose_det, &pose_lm, &palm, &hand] {
        assert!(m.max_rel <= TOLERANCE, "max rel err {} above {TOLERANCE}", m.max_rel);
    }
    assert!(!hand.ort_ms.is_empty(), "no palm found on any frame, handpose untested");
    assert!(face_ov < face_ort, "face_landmarks openvino {face_ov} ms not below ort {face_ort} ms");
}

#[test]
fn engine_names_parse() {
    assert_eq!(parse_engine(Some("ort")), Ok(Some(Engine::Ort)));
    assert_eq!(parse_engine(Some("OpenVINO")), Ok(Some(Engine::OpenVino)));
    assert_eq!(parse_engine(Some("auto")), Ok(None));
    assert_eq!(parse_engine(None), Ok(None));
    assert!(parse_engine(Some("tflite")).is_err());
}
