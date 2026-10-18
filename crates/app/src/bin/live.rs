//! Live demo and profiler: camera with the tracked landmarks on the left, the avatar driven by
//! the same frames on the right, in one window, for a fixed time (30 seconds by default).
//! Capture, inference, the monitor and rendering all run live; nothing is recorded first.
//! A stats line prints every second and a full report is printed and saved at the end.

#[path = "../../../bench/src/pipeline.rs"]
mod pipeline;
#[path = "../debug_overlay.rs"]
mod debug_overlay;
#[path = "../limb_body.rs"]
mod limb_body;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use avatar::body::to_avatar_space;
use avatar::settings::{LiveTuning, Settings};
use avatar::live::{BodyShare, LiveConfig, Preview, RenderSamples};
use avatar::window::{AvatarConfig, RunSummary};
use capture::slot::FrameSlot;
use capture::thread::{spawn_capture, FrameSource, NokhwaSource};
use core::frame::Frame;
use infer::blendshapes::BlendshapeModel;
use infer::detector::Detector;
use infer::landmarks::LandmarkModel;
use infer::session::{build_session, default_threads};
use infer::tracker::{StageStats, StageTimes, Tracker};
use infer::cigpose::{CigPose, PersonBox};
use infer::person_box::face_rect;
use infer::upper_body::{Mp2023Hands, UpperBody};
use pipeline::{spawn_inference_hooked, PipelineStats, TrackingShare};
use timing::cpu::CpuSampler;
use timing::hardware::{cpu_model, power_state, total_ram_mb, PowerState};
use timing::memory::process_memory;
use timing::report::Report;
use timing::stats::{median, p95};

const USAGE: &str = "usage: live [--vrm PATH] [--map PATH] [--models DIR] [--camera INDEX] [--threads N] [--seconds N] [--out DIR] [--no-mirror] [--body] [--body-models DIR] [--limb blaze|cigpose] [--no-overlay] [--focus] [--camera-size WxH]\nin the window: S settings, F focus mode (no preview, no debug drawing), L lighting, H help, Ctrl+O open a model, or drop a .vrm file. Settings are saved in settings.json next to the program.";
/// Whole-pose presence below this means no person; joint confidence is `--min-confidence`.
const POSE_PRESENCE_MIN: f32 = 0.5;
const CAMERA_FPS: u32 = 30;

/// One body-stage call: frame in, avatar-space arms and hands out.
type BodyRun = Box<dyn FnMut(&Frame) -> Result<avatar::body::BodyPose, String>>;

struct Args {
    vrm: PathBuf,
    map: PathBuf,
    models: PathBuf,
    camera: u32,
    threads: usize,
    seconds: f32,
    out: PathBuf,
    mirror: bool,
    body: bool,
    body_models: PathBuf,
    limb_cigpose: bool,
    overlay: bool,
    focus: bool,
    camera_size: (u32, u32),
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    exit(1);
}

fn parse_args() -> Args {
    // Camera size and mirroring come from the saved settings; command-line flags override them.
    let (saved, _) = Settings::load(&avatar::settings::default_path());
    let mut a = Args {
        vrm: PathBuf::from("samples/Seed-san.vrm"),
        map: PathBuf::from("data/blendshape_map.json"),
        models: PathBuf::from("models"),
        camera: 0,
        threads: default_threads(),
        seconds: 30.0,
        out: PathBuf::from("fixtures/private/out/live"),
        mirror: saved.mirror,
        body: false,
        body_models: PathBuf::from("models/upper_body"),
        limb_cigpose: false,
        overlay: true,
        focus: false,
        camera_size: (saved.camera_width, saved.camera_height),
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().unwrap_or_else(|| fail(format!("{name} needs a value")));
        match arg.as_str() {
            "--help" => {
                println!("{USAGE}");
                exit(0);
            }
            "--no-mirror" => a.mirror = false,
            "--body" => a.body = true,
            "--body-models" => a.body_models = PathBuf::from(value("--body-models")),
            "--no-overlay" => a.overlay = false,
            "--focus" => a.focus = true,
            "--camera-size" => {
                let v = value("--camera-size");
                a.camera_size = v
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .unwrap_or_else(|| fail("--camera-size expects WIDTHxHEIGHT, for example 1280x720"));
            }
            "--limb" => {
                a.limb_cigpose = match value("--limb").as_str() {
                    "cigpose" => true,
                    "blaze" => false,
                    _ => fail("--limb expects blaze or cigpose"),
                }
            }
            "--vrm" => a.vrm = PathBuf::from(value("--vrm")),
            "--map" => a.map = PathBuf::from(value("--map")),
            "--models" => a.models = PathBuf::from(value("--models")),
            "--out" => a.out = PathBuf::from(value("--out")),
            "--camera" => a.camera = value("--camera").parse().unwrap_or_else(|_| fail("--camera expects a number")),
            "--threads" => a.threads = value("--threads").parse().unwrap_or_else(|_| fail("--threads expects a number")),
            "--seconds" => a.seconds = value("--seconds").parse().unwrap_or_else(|_| fail("--seconds expects a number")),
            other => {
                eprintln!("error: unknown argument '{other}'\n{USAGE}");
                exit(2);
            }
        }
    }
    a
}

/// Camera frame to a mirrored RGBA preview with a small green square on every landmark.
fn build_preview(frame: &Frame, landmarks: Option<&[[f32; 3]]>) -> Preview {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let mut rgba = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let src = (y * w + x) * 3;
            let dst = (y * w + (w - 1 - x)) * 4;
            rgba[dst..dst + 3].copy_from_slice(&frame.bytes[src..src + 3]);
        }
    }
    if let Some(points) = landmarks {
        for p in points {
            let (cx, cy) = (w as i32 - 1 - p[0].round() as i32, p[1].round() as i32);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (x, y) = (cx + dx, cy + dy);
                    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                        let i = (y as usize * w + x as usize) * 4;
                        rgba[i..i + 3].copy_from_slice(&[0, 255, 0]);
                    }
                }
            }
        }
    }
    Preview { width: frame.width, height: frame.height, rgba }
}

fn pct(values: &[f32], p: f64) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let rank = (p / 100.0 * v.len() as f64).ceil() as usize;
    Some(v[rank.clamp(1, v.len()) - 1])
}

fn row(name: &str, values: &[f32]) -> String {
    if values.is_empty() {
        return format!("{name:<34} n/a\n");
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    format!(
        "{name:<34} median {:>7.2}  p95 {:>7.2}  p99 {:>7.2}  max {:>7.2}  mean {:>7.2}  (n={})\n",
        pct(values, 50.0).unwrap_or(0.0),
        pct(values, 95.0).unwrap_or(0.0),
        pct(values, 99.0).unwrap_or(0.0),
        pct(values, 100.0).unwrap_or(0.0),
        mean,
        values.len()
    )
}

fn ms(d: &[Duration]) -> Vec<f32> {
    d.iter().map(|x| x.as_secs_f32() * 1000.0).collect()
}

struct Second {
    t: f64,
    camera_fps: f64,
    tracking_fps: f64,
    render_fps: f64,
    latency_ms: f64,
    cpu_core_pct: f64,
    working_set_mb: f64,
}

/// Upper-body thread results: per-frame stage time and count of failed frames.
#[derive(Default)]
struct BodyTimes {
    ms: Vec<f32>,
    errors: u64,
}

fn main() {
    let args = parse_args();

    let load = |name: &str| build_session(&args.models.join(name), args.threads).unwrap_or_else(|e| fail(e));
    let (detector_session, onnx_threads) = load("face_detector.onnx");
    let (landmarks_session, _) = load("face_landmarks.onnx");
    let (blend_session, _) = load("face_blendshapes.onnx");
    let mut tracker = Tracker::new(
        Detector::new(detector_session),
        LandmarkModel::new(landmarks_session),
        BlendshapeModel::new(blend_session),
    );
    tracker.set_mirror(args.mirror);
    let (saved, _) = Settings::load(&avatar::settings::default_path());
    let tuning = Arc::new(LiveTuning::new(&saved));
    tracker.set_min_presence(tuning.face_confidence());
    let stage: StageStats = Arc::new(Mutex::new(StageTimes::default()));
    tracker.set_stage_stats(Arc::clone(&stage));

    let slot = Arc::new(FrameSlot::new());
    let stop = Arc::new(AtomicBool::new(false));
    let share: TrackingShare = Arc::new(Mutex::new(None));
    let pstats = Arc::new(PipelineStats::new());
    let preview = Arc::new(Mutex::new(None));
    let samples = Arc::new(Mutex::new(RenderSamples::default()));
    let hud = Arc::new(Mutex::new(String::new()));
    let preview_ms: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let body_slot = Arc::new(FrameSlot::new());
    let body_share: BodyShare = Arc::new(Mutex::new(None));
    // Face-landmark box for the CIGPose person box, refreshed by the tracking thread every frame.
    let face_box: Arc<Mutex<Option<PersonBox>>> = Arc::new(Mutex::new(None));
    // Debug drawing from the body thread (arms, hands, torso, head) in source-frame pixels.
    let body_overlay: Arc<Mutex<Option<debug_overlay::Overlay>>> = Arc::new(Mutex::new(None));
    let focus = Arc::new(AtomicBool::new(args.focus));

    let (opened_tx, opened_rx) = mpsc::channel::<Option<String>>();
    let camera = args.camera;
    let (cam_w, cam_h) = args.camera_size;
    let mut capture = spawn_capture(
        move || {
            let result = NokhwaSource::open(camera, cam_w, cam_h, CAMERA_FPS);
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

    // Upper body: sessions load here; UpperBody is built on its own thread because the
    // Box<dyn HandStage> inside it is not Send.
    let body_thread = if args.body {
        let load = |name: &str| build_session(&args.body_models.join(name), args.threads).unwrap_or_else(|e| fail(e)).0;
        let (palm, hand) = (load("palm_detection_mediapipe_2023feb.onnx"), load("handpose_estimation_mediapipe_2023feb.onnx"));
        // BlazePose sessions are only loaded for the BlazePose limb path; CIGPose replaces them.
        let blaze = (!args.limb_cigpose).then(|| (load("pose_detector.onnx"), load("pose_landmarks.onnx")));
        let cig = args.limb_cigpose.then(|| {
            // ONNX Runtime unless VT_ENGINE says otherwise: OpenVINO gave no gain on CIGPose-m and
            // cost about 100 MB (docs/notes/live-limb-gate.md).
            let engine = std::env::var("VT_ENGINE")
                .ok()
                .and_then(|v| infer::engine::parse_engine(Some(&v)).ok().flatten())
                .unwrap_or(infer::engine::Engine::Ort);
            CigPose::load(engine, &args.body_models.join("cigpose-m_coco_256x192.onnx"), args.threads)
                .unwrap_or_else(|e| fail(e))
        });
        let (body_in, share, stop, mirror) = (Arc::clone(&body_slot), Arc::clone(&body_share), Arc::clone(&stop), args.mirror);
        let face = Arc::clone(&face_box);
        let overlay_out = Arc::clone(&body_overlay);
        let focus_body = Arc::clone(&focus);
        let tuning_body = Arc::clone(&tuning);
        Some(std::thread::spawn(move || {
            let hands = Box::new(Mp2023Hands::new(palm, hand));
            let mut run: BodyRun = match (blaze, cig) {
                (Some((det, lm)), _) => {
                    let mut stage = UpperBody::new(det, lm, hands);
                    let log = std::env::var_os("VT_LIMB_LOG").is_some();
                    Box::new(move |f| {
                        let out = stage.run(&f.bytes, f.width, f.height);
                        // The settings panel can change the thresholds while this runs.
                        let (body_conf, hand_conf) = (tuning_body.body_confidence(), tuning_body.hand_confidence());
                        let drawing = !focus_body.load(Ordering::Relaxed) && tuning_body.show_debug_drawing();
                        if let (Ok(o), true) = (&out, drawing) {
                            *overlay_out.lock().unwrap() = Some(debug_overlay::from_upper_body(o, body_conf, hand_conf));
                        }
                        // Each joint is judged on its own (visibility and inside the picture), so shoulders
                        // alone still drive the torso while arms and hands that are not seen keep their last pose.
                        // Only a pose the model doubts altogether is dropped.
                        let pose = out.and_then(|o| {
                            if o.presence < POSE_PRESENCE_MIN {
                                return Err(format!("pose presence {:.2} below {POSE_PRESENCE_MIN}", o.presence));
                            }
                            Ok(to_avatar_space(&o, mirror, body_conf, hand_conf))
                        });
                        if let (true, Ok(p)) = (log, &pose) {
                            let dz = |a: &avatar::body::ArmPose| a.wrist.zip(a.shoulder).map(|(w, s)| w.z - s.z);
                            eprintln!("blazelog wristdz(avatar L,R) {:?} {:?}", dz(&p.left), dz(&p.right));
                        }
                        pose
                    })
                }
                (None, Some(cig)) => {
                    let mut stage = limb_body::LimbBody::new(cig, hands, mirror);
                    Box::new(move |f| {
                        let pose = stage.run(&f.bytes, f.width, f.height, *face.lock().unwrap());
                        if !focus_body.load(Ordering::Relaxed) && tuning_body.show_debug_drawing() {
                            *overlay_out.lock().unwrap() = Some(stage.overlay.clone());
                        }
                        pose
                    })
                }
                (None, None) => unreachable!("one limb path is always selected"),
            };
            let mut times = BodyTimes::default();
            while !stop.load(Ordering::SeqCst) {
                let Some(frame) = body_in.wait_take(Duration::from_millis(50)) else { continue };
                let started = Instant::now();
                match run(&frame) {
                    Ok(pose) => *share.lock().unwrap() = Some(pose),
                    Err(_) => {
                        times.errors += 1;
                        *share.lock().unwrap() = None;
                    }
                }
                times.ms.push(started.elapsed().as_secs_f32() * 1000.0);
            }
            times
        }))
    } else {
        None
    };

    let inference = {
        let preview = Arc::clone(&preview);
        let preview_ms = Arc::clone(&preview_ms);
        let face_box = Arc::clone(&face_box);
        let overlay_in = args.overlay.then(|| Arc::clone(&body_overlay));
        let body_in = args.body.then(|| Arc::clone(&body_slot));
        let focus_hook = Arc::clone(&focus);
        let tuning_hook = Arc::clone(&tuning);
        spawn_inference_hooked(
            tracker,
            Arc::clone(&slot),
            Arc::clone(&share),
            Arc::clone(&pstats),
            Arc::clone(&stop),
            move |frame, tracker| {
                let started = Instant::now();
                if let Some(body_in) = &body_in {
                    body_in.put(frame.clone());
                }
                // The settings panel can change the face threshold while this runs.
                tracker.set_min_presence(tuning_hook.face_confidence());
                let landmarks = tracker.last_landmarks();
                *face_box.lock().unwrap() = landmarks.and_then(face_rect);
                if focus_hook.load(Ordering::Relaxed) || !tuning_hook.show_preview() {
                    return;
                }
                let mut built = build_preview(frame, landmarks);
                if let Some(shared) = overlay_in.as_ref().filter(|_| tuning_hook.show_debug_drawing()) {
                    let mut shapes = shared.lock().unwrap().clone().unwrap_or_default();
                    if let Some(lm) = landmarks {
                        let e = debug_overlay::eyes(lm);
                        shapes.lines.extend(e.lines);
                        shapes.points.extend(e.points);
                    }
                    debug_overlay::draw(&mut built.rgba, built.width as usize, built.height as usize, &shapes);
                }
                *preview.lock().unwrap() = Some(built);
                if let Ok(mut v) = preview_ms.lock()
                    && v.len() < 100_000
                {
                    v.push(started.elapsed().as_secs_f32() * 1000.0);
                }
            },
        )
    };

    // Monitor: once a second, sample every metric, print a line, update the window title.
    let run_start = Instant::now();
    let monitor = {
        let (stop, pstats, slot, samples, stage, hud) = (
            Arc::clone(&stop),
            Arc::clone(&pstats),
            Arc::clone(&slot),
            Arc::clone(&samples),
            Arc::clone(&stage),
            Arc::clone(&hud),
        );
        std::thread::spawn(move || {
            let mut rows: Vec<Second> = Vec::new();
            let (mut last_tracked, mut last_dropped, mut last_frames, mut last_lat) = (0u64, 0u64, 0u64, 0usize);
            println!("{:>4} {:>7} {:>7} {:>7} {:>9} {:>7} {:>8}", "t(s)", "cam", "track", "render", "lat(ms)", "cpu%", "ram(MB)");
            while !stop.load(Ordering::SeqCst) {
                let cpu = CpuSampler::start().ok();
                std::thread::sleep(Duration::from_secs(1));
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let tracked = pstats.frames_tracked.load(Ordering::SeqCst);
                let dropped = slot.dropped();
                let frames = samples.lock().map(|s| s.frames).unwrap_or(0);
                let lat_ms = {
                    let lats = pstats.latencies.lock().unwrap();
                    let slice = ms(&lats[last_lat.min(lats.len())..]);
                    last_lat = lats.len();
                    pct(&slice, 50.0).unwrap_or(0.0) as f64
                };
                let row = Second {
                    t: run_start.elapsed().as_secs_f64(),
                    camera_fps: ((tracked - last_tracked) + (dropped - last_dropped)) as f64,
                    tracking_fps: (tracked - last_tracked) as f64,
                    render_fps: (frames - last_frames) as f64,
                    latency_ms: lat_ms,
                    cpu_core_pct: cpu.as_ref().and_then(|c| c.percent().ok()).unwrap_or(0.0),
                    working_set_mb: process_memory().map(|m| m.working_set_mb).unwrap_or(0.0),
                };
                (last_tracked, last_dropped, last_frames) = (tracked, dropped, frames);
                let total = stage.lock().map(|s| pct(&s.total_ms, 50.0).unwrap_or(0.0)).unwrap_or(0.0);
                let line = format!(
                    "{:>4.0} {:>7.1} {:>7.1} {:>7.1} {:>9.1} {:>7.0} {:>8.0}",
                    row.t, row.camera_fps, row.tracking_fps, row.render_fps, row.latency_ms, row.cpu_core_pct, row.working_set_mb
                );
                println!("{line}");
                *hud.lock().unwrap() = format!(
                    "LIVE {:.0}s | cam {:.0} fps | track {:.0} fps | render {:.0} fps | latency {:.1} ms (tracker {:.1}) | CPU {:.0}% of a core | RAM {:.0} MB",
                    row.t, row.camera_fps, row.tracking_fps, row.render_fps, row.latency_ms, total, row.cpu_core_pct, row.working_set_mb
                );
                rows.push(row);
            }
            rows
        })
    };

    let cpu_all = CpuSampler::start().ok();
    let outcome = avatar::window::run(AvatarConfig {
        vrm_path: std::fs::canonicalize(&args.vrm).unwrap_or(args.vrm.clone()),
        map_path: args.map.clone(),
        tracking: Arc::clone(&share),
        live: Some(LiveConfig {
            preview: Arc::clone(&preview),
            hud: Arc::clone(&hud),
            seconds: args.seconds,
            samples: Arc::clone(&samples),
            body: args.body.then(|| Arc::clone(&body_share)),
            focus: Arc::clone(&focus),
            tuning: Arc::clone(&tuning),
        }),
    });
    let elapsed = run_start.elapsed();
    let avg_cpu = cpu_all.as_ref().and_then(|c| c.percent().ok());
    let memory = process_memory().ok();

    stop.store(true, Ordering::SeqCst);
    capture.stop();
    let _ = inference.join();
    let body_times = body_thread.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    let rows = monitor.join().unwrap_or_default();
    let summary: RunSummary = match outcome {
        Ok(s) => s,
        Err(e) => fail(e),
    };

    // ---- report ----
    let latencies = pstats.latencies.lock().map(|g| g.clone()).unwrap_or_default();
    let tracked = pstats.frames_tracked.load(Ordering::SeqCst);
    let st = stage.lock().map(|g| g.clone()).unwrap_or_default();
    let rs = samples.lock().map(|s| (s.frames, s.frame_ms.clone(), s.handoff_ms.clone(), s.upload_ms.clone())).unwrap_or_default();
    let render_secs: f64 = rs.1.iter().map(|x| *x as f64).sum::<f64>() / 1000.0;
    let render_fps = if render_secs > 0.0 { Some(rs.0 as f64 / render_secs) } else { None };
    let mut report = Report::from_samples(
        &latencies,
        Some(f64::from(capture.delivered_fps())),
        Some(tracked as f64 / elapsed.as_secs_f64()),
        render_fps,
    );
    if let Some(m) = memory {
        report.peak_working_set_mb = Some(m.peak_working_set_mb);
        report.peak_private_mb = Some(m.peak_private_mb);
    }
    if let Some(d) = summary.memory {
        report.dxgi_local_mb = Some(d.local_mb);
        report.dxgi_nonlocal_mb = Some(d.non_local_mb);
    }
    report.onnx_threads = Some(onnx_threads as u32);
    report.avg_cpu_percent = avg_cpu;
    report.cpu_model = cpu_model();
    report.ram_mb = Some(total_ram_mb() as f64);
    report.power_state = match power_state() {
        PowerState::PluggedIn => "plugged in",
        PowerState::OnBattery => "on battery",
        PowerState::Unknown => "unknown",
    }
    .to_string();

    let mut text = String::new();
    let _ = writeln!(text, "=== LIVE RUN: {:.1} s wall, {} tracked frames, {} dropped by the single-slot buffer ===", elapsed.as_secs_f64(), tracked, slot.dropped());
    text += &report.to_text();
    text += "\n--- stage timings per tracked frame (ms) ---\n";
    text += &row("detector (scheduled, not every frame)", &st.detect_ms);
    text += &row("landmark model", &st.landmarks_ms);
    text += &row("blendshape model", &st.blend_ms);
    text += &row("head pose fit", &st.fit_ms);
    text += &row("mirror + filters", &st.filter_ms);
    text += &row("tracker total", &st.total_ms);
    let pm = preview_ms.lock().map(|g| g.clone()).unwrap_or_default();
    text += &row("preview build (dots, flip, RGBA)", &pm);
    if args.body {
        let bm = &body_times.ms;
        let _ = writeln!(text, "body stage median/p95 ms: {:.1} / {:.1}", pct(bm, 50.0).unwrap_or(0.0), pct(bm, 95.0).unwrap_or(0.0));
        let _ = writeln!(text, "body fps: {:.1}", bm.len() as f64 / elapsed.as_secs_f64());
        let _ = writeln!(text, "body errors: {}", body_times.errors);
    } else {
        text += "body: off (run with --body)\n";
    }
    text += "\n--- latency path (ms) ---\n";
    text += &row("camera arrival -> tracker ready", &ms(&latencies));
    text += &row("tracker ready -> applied in renderer", &rs.2);
    text += &row("preview texture upload (render thread)", &rs.3);
    text += &row("render frame interval", &rs.1);
    if let (Some(a), Some(b), Some(f)) = (median(&latencies), pct(&rs.2, 50.0), pct(&rs.1, 50.0)) {
        let _ = writeln!(
            text,
            "estimated camera-to-pixels (median): {:.1} ms tracker + {:.1} ms hand-off + about {:.1} ms to the next frame (estimate; the display and camera exposure are not included)",
            a.as_secs_f32() * 1000.0,
            b,
            f / 2.0
        );
    }
    if let Some(p) = p95(&latencies) {
        let _ = writeln!(text, "p95 tracking latency: {:.1} ms", p.as_secs_f32() * 1000.0);
    }
    text += "\n--- per second ---\n";
    let col = |f: &dyn Fn(&Second) -> f64| rows.iter().map(f).map(|x| x as f32).collect::<Vec<f32>>();
    text += &row("camera fps", &col(&|r| r.camera_fps));
    text += &row("tracking fps", &col(&|r| r.tracking_fps));
    text += &row("render fps", &col(&|r| r.render_fps));
    text += &row("CPU % of one core (process)", &col(&|r| r.cpu_core_pct));
    text += &row("working set MB", &col(&|r| r.working_set_mb));
    let _ = writeln!(text, "logical cores: {}", std::thread::available_parallelism().map_or(1, |n| n.get()));
    println!("\n{text}");

    if std::fs::create_dir_all(&args.out).is_ok() {
        let _ = std::fs::write(args.out.join("live_report.txt"), &text);
        let mut csv = String::from("t_s,camera_fps,tracking_fps,render_fps,latency_ms,cpu_percent_of_one_core,working_set_mb\n");
        for r in &rows {
            let _ = writeln!(csv, "{:.1},{},{},{},{:.2},{:.1},{:.1}", r.t, r.camera_fps, r.tracking_fps, r.render_fps, r.latency_ms, r.cpu_core_pct, r.working_set_mb);
        }
        let _ = std::fs::write(args.out.join("live_stats.csv"), csv);
        println!("saved {} and live_stats.csv", args.out.join("live_report.txt").display());
    }
}
