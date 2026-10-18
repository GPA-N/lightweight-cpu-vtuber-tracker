//! Face tracking spike: camera capture, ONNX tracking, and the avatar window in one binary.
//!
//! Bevy must own the main thread, so capture and inference run on their own threads and
//! the window runs last on this thread.

#[path = "../../bench/src/pipeline.rs"]
mod pipeline;
#[allow(dead_code)]
#[path = "debug_overlay.rs"]
mod debug_overlay;
#[path = "hmr_body.rs"]
mod hmr_body;

use std::path::PathBuf;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use avatar::render_report::DxgiMemory;
use avatar::window::{AvatarConfig, RunSummary};
use capture::thread::{spawn_capture, FrameSource, NokhwaSource};
use infer::blendshapes::BlendshapeModel;
use infer::detector::Detector;
use infer::landmarks::LandmarkModel;
use infer::session::{build_session, default_threads};
use infer::tracker::Tracker;
use infer::upper_body::Mp2023Hands;
use pipeline::{spawn_inference, tracking_fps, PipelineStats, TrackingShare};
use timing::cpu::CpuSampler;
use timing::hardware::{cpu_model, power_state, total_ram_mb, PowerState};
use timing::memory::{process_memory, ProcessMemory};
use timing::report::Report;
use capture::slot::FrameSlot;

const USAGE: &str = "usage: app --vrm PATH [--map PATH] [--models DIR] [--camera INDEX] [--threads N] [--no-mirror] [--seconds N] [--help]";

const DEFAULT_MAP: &str = "data/blendshape_map.json";
const DEFAULT_MODELS: &str = "models";
const CAMERA_WIDTH: u32 = 640;
const CAMERA_HEIGHT: u32 = 480;
const CAMERA_FPS: u32 = 30;

#[derive(Debug, PartialEq)]
struct Args {
    vrm: PathBuf,
    map: PathBuf,
    models: PathBuf,
    camera: u32,
    threads: usize,
    mirror: bool,
    seconds: Option<u64>,
}

#[derive(Debug, PartialEq)]
enum Parsed {
    Run(Args),
    Help,
}

/// Parses the command line (without the program name). Returns one line of error text on failure.
fn parse_args(args: &[String]) -> Result<Parsed, String> {
    let mut vrm = None;
    let mut map = PathBuf::from(DEFAULT_MAP);
    let mut models = PathBuf::from(DEFAULT_MODELS);
    let mut camera = 0u32;
    let mut threads = default_threads();
    let mut mirror = true;
    let mut seconds = None;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--help" => return Ok(Parsed::Help),
            "--no-mirror" => mirror = false,
            "--vrm" => vrm = Some(PathBuf::from(value(&mut it, arg)?)),
            "--map" => map = PathBuf::from(value(&mut it, arg)?),
            "--models" => models = PathBuf::from(value(&mut it, arg)?),
            "--camera" => camera = number(&mut it, arg)?,
            "--threads" => threads = number(&mut it, arg)?,
            "--seconds" => seconds = Some(number(&mut it, arg)?),
            other => return Err(format!("unknown argument '{other}'")),
        }
    }

    let vrm = vrm.ok_or_else(|| "missing required --vrm".to_string())?;
    Ok(Parsed::Run(Args { vrm, map, models, camera, threads, mirror, seconds }))
}

fn value<'a>(it: &mut std::slice::Iter<'a, String>, flag: &str) -> Result<&'a String, String> {
    it.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn number<'a, T: std::str::FromStr>(
    it: &mut std::slice::Iter<'a, String>,
    flag: &str,
) -> Result<T, String> {
    let text = value(it, flag)?;
    text.parse()
        .map_err(|_| format!("{flag} expects a number, got '{text}'"))
}

/// Measurements gathered during a run, passed to `build_report`.
struct RunFigures<'a> {
    latencies: &'a [Duration],
    camera_fps: Option<f64>,
    tracking_fps: Option<f64>,
    render_fps: Option<f64>,
    memory: Option<ProcessMemory>,
    dxgi: Option<DxgiMemory>,
    onnx_threads: Option<u32>,
    avg_cpu: Option<f64>,
    cpu_model: String,
    ram_mb: Option<f64>,
    power: PowerState,
}

fn power_text(power: PowerState) -> &'static str {
    match power {
        PowerState::PluggedIn => "plugged in",
        PowerState::OnBattery => "on battery",
        PowerState::Unknown => "unknown",
    }
}

/// Turns the measurements into the printed report. Pure, so it can be tested without hardware.
fn build_report(f: &RunFigures) -> Report {
    let mut report = Report::from_samples(f.latencies, f.camera_fps, f.tracking_fps, f.render_fps);
    if let Some(m) = f.memory {
        report.peak_working_set_mb = Some(m.peak_working_set_mb);
        report.peak_private_mb = Some(m.peak_private_mb);
    }
    if let Some(d) = f.dxgi {
        report.dxgi_local_mb = Some(d.local_mb);
        report.dxgi_nonlocal_mb = Some(d.non_local_mb);
    }
    report.onnx_threads = f.onnx_threads;
    report.avg_cpu_percent = f.avg_cpu;
    report.cpu_model = f.cpu_model.clone();
    report.ram_mb = f.ram_mb;
    report.power_state = power_text(f.power).to_string();
    report
}

/// Prints an error line and exits with status 1.
fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    exit(1);
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return;
        }
        Ok(Parsed::Run(args)) => args,
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!("{USAGE}");
            exit(2);
        }
    };
    if args.seconds.is_some() {
        eprintln!("warning: --seconds is parsed but not enforced; the window closes only when the user closes it");
    }

    // Debug harness: APP_REPLAY=dir renders recorded tracker output (dir/track.txt) frame by frame.
    if std::env::var_os("APP_REPLAY").is_some() {
        let share: TrackingShare = Arc::new(std::sync::Mutex::new(None));
        if let Err(e) = avatar::window::run(AvatarConfig {
            vrm_path: std::fs::canonicalize(&args.vrm).unwrap_or(args.vrm.clone()),
            map_path: args.map,
            tracking: share,
            live: None,
        }) {
            fail(e);
        }
        return;
    }

    let (detector_session, onnx_threads) =
        build_session(&args.models.join("face_detector.onnx"), args.threads)
            .unwrap_or_else(|e| fail(e));
    let (landmarks_session, _) =
        build_session(&args.models.join("face_landmarks.onnx"), args.threads)
            .unwrap_or_else(|e| fail(e));
    let (blend_session, _) =
        build_session(&args.models.join("face_blendshapes.onnx"), args.threads)
            .unwrap_or_else(|e| fail(e));

    let mut tracker = Tracker::new(
        Detector::new(detector_session),
        LandmarkModel::new(landmarks_session),
        BlendshapeModel::new(blend_session),
    );
    tracker.set_mirror(args.mirror);

    // Debug harness: APP_STILL_BODY=photo.png runs the face tracker and the body stage (InstantHMR and the
    // hand models) on one photo and poses the avatar's arms, hands and fingers from it. Use with
    // AVATAR_SHOT=out.png. The hand model is PROVISIONAL (2023 port).
    if let Some(image_path) = std::env::var_os("APP_STILL_BODY") {
        let rgb = image::open(&image_path).unwrap_or_else(|e| fail(e)).to_rgb8();
        let (w, h) = rgb.dimensions();
        let bytes = rgb.into_raw();
        let frame = core::frame::Frame {
            width: w,
            height: h,
            format: core::frame::PixelFormat::Rgb24,
            bytes: bytes.clone(),
            arrived: Instant::now(),
        };
        let out = tracker.step(&frame).unwrap_or_else(|e| fail(e));
        println!("still body: face_found {}", out.face_found);
        let strong: Vec<String> = core::names::BLENDSHAPE_NAMES
            .iter()
            .zip(out.weights.iter())
            .filter(|(_, w)| **w >= 0.25)
            .map(|(n, w)| format!("{n} {w:.2}"))
            .collect();
        println!("still face weights >= 0.25: {}", strong.join(", "));
        // The body path of `live --body`: InstantHMR for the arms, the hand models for the fingers. The
        // person box comes from the face landmarks; with no face the whole frame is used.
        let upper = args.models.join("upper_body");
        let load = |name: &str| build_session(&upper.join(name), args.threads).unwrap_or_else(|e| fail(e)).0;
        let engine = std::env::var("VT_ENGINE")
            .ok()
            .and_then(|v| infer::engine::parse_engine(Some(&v)).ok().flatten())
            .unwrap_or(infer::engine::Engine::Ort);
        let hmr = infer::instant_hmr::InstantHmr::load(engine, &upper.join("instanthmr_joints.onnx"), args.threads)
            .unwrap_or_else(|e| fail(format!("{e} (scripts\\fetch_instanthmr.ps1 installs the model)")));
        let hands = Box::new(Mp2023Hands::new(
            load("palm_detection_mediapipe_2023feb.onnx"),
            load("handpose_estimation_mediapipe_2023feb.onnx"),
        ));
        let mut body = hmr_body::HmrBody::new(hmr, hands, args.mirror);
        let (fw, fh) = (w as f32, h as f32);
        let face = tracker
            .last_landmarks()
            .and_then(infer::person_box::face_rect)
            .map(|f| infer::person_box::from_face(f, fw, fh))
            .unwrap_or(infer::cigpose::PersonBox { x: 0.0, y: 0.0, w: fw, h: fh });
        let started = Instant::now();
        let pose = body
            .run(&bytes, w, h, Some(face), avatar::body::VIS_MIN, avatar::body::HAND_SCORE_MIN)
            .unwrap_or_else(|e| fail(e));
        println!("still body: first run {:.0} ms", started.elapsed().as_secs_f32() * 1000.0);
        for (name, arm) in [("left", &pose.left), ("right", &pose.right)] {
            println!("arm {name}: shoulder {:?} elbow {:?} wrist {:?} hand {}", arm.shoulder, arm.elbow, arm.wrist, arm.hand.is_some());
        }
        let share: TrackingShare = Arc::new(std::sync::Mutex::new(Some(out.clone())));
        let refill = Arc::clone(&share);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            *refill.lock().unwrap() = Some(out.clone());
        });
        let cfg = AvatarConfig {
            vrm_path: std::fs::canonicalize(&args.vrm).unwrap_or(args.vrm.clone()),
            map_path: args.map,
            tracking: share,
            live: None,
        };
        if let Err(e) = avatar::window::run_with_body(cfg, Some(pose)) {
            fail(e);
        }
        return;
    }

    // Debug harness: APP_STILL=image.png tracks one photo instead of the camera, and
    // AVATAR_SHOT=out.png makes the window save a screenshot and exit.
    if let Some(image_path) = std::env::var_os("APP_STILL") {
        let rgb = image::open(&image_path).unwrap_or_else(|e| fail(e)).to_rgb8();
        let (w, h) = rgb.dimensions();
        let frame = core::frame::Frame {
            width: w,
            height: h,
            format: core::frame::PixelFormat::Rgb24,
            bytes: rgb.into_raw(),
            arrived: Instant::now(),
        };
        let out = tracker.step(&frame).unwrap_or_else(|e| fail(e));
        println!("still: face_found {} head {:?}", out.face_found, out.head_rotation.coords.as_slice());
        let share: TrackingShare = Arc::new(std::sync::Mutex::new(Some(out.clone())));
        // The window takes each output once, and the model may still be loading, so keep re-publishing.
        let refill = Arc::clone(&share);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            *refill.lock().unwrap() = Some(out.clone());
        });
        if let Err(e) = avatar::window::run(AvatarConfig { vrm_path: std::fs::canonicalize(&args.vrm).unwrap_or(args.vrm.clone()), map_path: args.map, tracking: share, live: None }) {
            fail(e);
        }
        return;
    }

    let slot = Arc::new(FrameSlot::new());
    let stop = Arc::new(AtomicBool::new(false));
    let share: TrackingShare = Arc::new(std::sync::Mutex::new(None));
    let stats = Arc::new(PipelineStats::new());

    // The camera is opened on the capture thread. Its outcome comes back over a channel so an
    // open failure can exit 1 before the window starts.
    let (opened_tx, opened_rx) = mpsc::channel::<Option<String>>();
    let camera = args.camera;
    let mut capture = spawn_capture(
        move || {
            let result = NokhwaSource::open(camera, CAMERA_WIDTH, CAMERA_HEIGHT, CAMERA_FPS);
            let _ = opened_tx.send(result.as_ref().err().map(|e| e.to_string()));
            result.map(|source| Box::new(source) as Box<dyn FrameSource>)
        },
        Arc::clone(&slot),
    );
    match opened_rx.recv() {
        Ok(None) => {}
        Ok(Some(msg)) => {
            capture.join();
            fail(msg);
        }
        Err(_) => {
            capture.join();
            fail("capture thread stopped before opening the camera");
        }
    }

    let inference = spawn_inference(
        tracker,
        Arc::clone(&slot),
        Arc::clone(&share),
        Arc::clone(&stats),
        Arc::clone(&stop),
    );

    let cpu = CpuSampler::start().ok();
    let started = Instant::now();
    let outcome = avatar::window::run(AvatarConfig {
        vrm_path: std::fs::canonicalize(&args.vrm).unwrap_or(args.vrm.clone()),
        map_path: args.map,
        tracking: Arc::clone(&share),
        live: None,
    });
    let elapsed = started.elapsed();

    stop.store(true, Ordering::SeqCst);
    capture.stop();
    if inference.join().is_err() {
        eprintln!("warning: inference thread panicked");
    }

    let summary: RunSummary = match outcome {
        Ok(summary) => summary,
        Err(e) => fail(e),
    };

    if let Some(err) = capture.error() {
        eprintln!("warning: camera stopped during the run: {err}");
    }

    let latencies = stats.latencies.lock().map(|g| g.clone()).unwrap_or_default();
    let figures = RunFigures {
        latencies: &latencies,
        camera_fps: Some(f64::from(capture.delivered_fps())),
        tracking_fps: Some(tracking_fps(&stats, elapsed)),
        render_fps: summary.average_fps,
        memory: process_memory().ok(),
        dxgi: summary.memory,
        onnx_threads: Some(onnx_threads as u32),
        avg_cpu: cpu.as_ref().and_then(|s| s.percent().ok()),
        cpu_model: cpu_model(),
        ram_mb: Some(total_ram_mb() as f64),
        power: power_state(),
    };
    println!("{}", build_report(&figures).to_text());

    let errors = stats.errors.load(Ordering::SeqCst);
    let last_error = stats.last_error.lock().ok().and_then(|g| g.clone());
    println!("pipeline errors: {errors}");
    if let Some(msg) = last_error {
        println!("last_error: {msg}");
    }
}

#[cfg(test)]
mod cli {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_apply_when_only_vrm_is_given() {
        let parsed = parse_args(&argv(&["--vrm", "a.vrm"])).unwrap();
        assert_eq!(
            parsed,
            Parsed::Run(Args {
                vrm: PathBuf::from("a.vrm"),
                map: PathBuf::from("data/blendshape_map.json"),
                models: PathBuf::from("models"),
                camera: 0,
                threads: default_threads(),
                mirror: true,
                seconds: None,
            })
        );
    }

    #[test]
    fn every_flag_is_parsed() {
        let parsed = parse_args(&argv(&[
            "--vrm", "a.vrm", "--map", "m.json", "--models", "mods", "--camera", "2",
            "--threads", "3", "--no-mirror", "--seconds", "5",
        ]))
        .unwrap();
        assert_eq!(
            parsed,
            Parsed::Run(Args {
                vrm: PathBuf::from("a.vrm"),
                map: PathBuf::from("m.json"),
                models: PathBuf::from("mods"),
                camera: 2,
                threads: 3,
                mirror: false,
                seconds: Some(5),
            })
        );
    }

    #[test]
    fn missing_vrm_is_an_error() {
        let err = parse_args(&argv(&["--camera", "1"])).unwrap_err();
        assert!(err.contains("--vrm"), "{err}");
    }

    #[test]
    fn help_is_recognised_without_vrm() {
        assert_eq!(parse_args(&argv(&["--help"])), Ok(Parsed::Help));
    }

    #[test]
    fn help_text_names_every_flag() {
        for flag in ["--vrm", "--map", "--models", "--camera", "--threads", "--no-mirror", "--seconds", "--help"] {
            assert!(USAGE.contains(flag), "usage lacks {flag}");
        }
    }

    #[test]
    fn unknown_flag_is_an_error() {
        let err = parse_args(&argv(&["--vrm", "a.vrm", "--bogus"])).unwrap_err();
        assert!(err.contains("--bogus"), "{err}");
    }

    #[test]
    fn missing_value_is_an_error() {
        let err = parse_args(&argv(&["--vrm"])).unwrap_err();
        assert!(err.contains("--vrm needs a value"), "{err}");
        let err = parse_args(&argv(&["--vrm", "a.vrm", "--camera"])).unwrap_err();
        assert!(err.contains("--camera needs a value"), "{err}");
    }

    #[test]
    fn non_numeric_value_is_an_error() {
        let err = parse_args(&argv(&["--vrm", "a.vrm", "--camera", "x"])).unwrap_err();
        assert!(err.contains("--camera expects a number"), "{err}");
    }

    #[test]
    fn build_report_copies_figures_into_report() {
        let latencies = [Duration::from_millis(10), Duration::from_millis(20)];
        let figures = RunFigures {
            latencies: &latencies,
            camera_fps: Some(30.0),
            tracking_fps: Some(25.0),
            render_fps: Some(60.0),
            memory: Some(ProcessMemory {
                working_set_mb: 100.0,
                peak_working_set_mb: 120.0,
                peak_private_mb: 130.0,
            }),
            dxgi: Some(DxgiMemory {
                local_mb: 512.0,
                non_local_mb: 64.0,
                local_budget_mb: 2048.0,
                non_local_budget_mb: 4096.0,
            }),
            onnx_threads: Some(4),
            avg_cpu: Some(12.5),
            cpu_model: "test cpu".to_string(),
            ram_mb: Some(16384.0),
            power: PowerState::OnBattery,
        };
        let report = build_report(&figures);
        assert!(report.median_latency_ms.is_some());
        assert_eq!(report.delivered_camera_fps, Some(30.0));
        assert_eq!(report.tracking_fps, Some(25.0));
        assert_eq!(report.render_fps, Some(60.0));
        assert_eq!(report.peak_working_set_mb, Some(120.0));
        assert_eq!(report.peak_private_mb, Some(130.0));
        assert_eq!(report.dxgi_local_mb, Some(512.0));
        assert_eq!(report.dxgi_nonlocal_mb, Some(64.0));
        assert_eq!(report.onnx_threads, Some(4));
        assert_eq!(report.avg_cpu_percent, Some(12.5));
        assert_eq!(report.cpu_model, "test cpu");
        assert_eq!(report.power_state, "on battery");
    }
}

