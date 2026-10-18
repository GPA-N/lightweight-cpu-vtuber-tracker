use std::path::{Path, PathBuf};
use std::time::Instant;

use core::frame::{Frame, PixelFormat};
use image::{Rgb, RgbImage};
use infer::blendshapes::BlendshapeModel;
use infer::detector::Detector;
use infer::landmarks::LandmarkModel;
use infer::session::build_session;
use infer::tracker::Tracker;
use ort::session::Session;
use ort::value::Tensor;
use timing::memory::process_memory;

const UPPER: &str = "models/upper_body";
const FACE_MODELS: &str = "models";
const FRAMES: &str = "fixtures/private/golden_take";
const OUT: &str = "fixtures/private/out/ub_bench";
const POSE_DETECTOR: &str = "pose_detector.onnx";
const POSE_LANDMARKS: &str = "pose_landmarks.onnx";
const HANDPOSE: &str = "handpose_estimation_mediapipe_2023feb.onnx";
const UPPER_BODY_MODELS: [&str; 4] = [
    POSE_DETECTOR,
    POSE_LANDMARKS,
    "palm_detection_mediapipe_2023feb.onnx",
    HANDPOSE,
];
const INPUT: &str = "input_1";
const WARMUP: usize = 5;
const BODY_POINTS: usize = 33;
const POSE_POINTS: usize = 39;
const HAND_POINTS: usize = 21;
const DRAW_FRAMES: [usize; 3] = [0, 100, 200];

/// Loads each upper-body ONNX through ort and prints its I/O names, dtypes and shapes.
/// A load failure prints the exact error and the run continues to the next model.
pub fn inspect(dir: &Path) -> Result<(), String> {
    for name in UPPER_BODY_MODELS {
        println!("== {name}");
        match build_session(&dir.join(name), 1) {
            Ok((session, _)) => {
                for outlet in session.inputs() {
                    println!("input  {} {:?}", outlet.name(), outlet.dtype());
                }
                for outlet in session.outputs() {
                    println!("output {} {:?}", outlet.name(), outlet.dtype());
                }
            }
            Err(e) => println!("load error: {e}"),
        }
    }
    Ok(())
}

/// Entry point for the body-tracking bench flags. None means the args are not ours.
pub fn dispatch(args: &[String]) -> Option<i32> {
    let result = match args.first()?.as_str() {
        "--body-inspect" => inspect(Path::new(UPPER)),
        "--body-bench" => bench(&args[1..]),
        "--body-photos" => photos(&args[1..]),
        _ => return None,
    };
    Some(match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    })
}

#[derive(Clone, Copy)]
struct Pt {
    x: f32,
    y: f32,
    vis: f32,
}

/// Source-pixel rectangle that is resampled to the model input.
#[derive(Clone, Copy)]
struct Rect {
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
}

fn letterbox(w: u32, h: u32) -> Rect {
    let s = w.max(h) as f32;
    Rect {
        x0: (w as f32 - s) / 2.0,
        y0: (h as f32 - s) / 2.0,
        w: s,
        h: s,
    }
}

fn around(cx: f32, cy: f32, side: f32) -> Rect {
    Rect {
        x0: cx - side / 2.0,
        y0: cy - side / 2.0,
        w: side,
        h: side,
    }
}

fn sig(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn mean(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}

/// Nearest-neighbour resample of a rect to a square NHWC RGB tensor.
/// Pixels outside the source are black. `signed` maps to [-1, 1], else [0, 1].
fn sample(src: &RgbImage, r: Rect, out: usize, signed: bool) -> Vec<f32> {
    let (sw, sh) = src.dimensions();
    let mut v = Vec::with_capacity(out * out * 3);
    for oy in 0..out {
        let sy = r.y0 + (oy as f32 + 0.5) * r.h / out as f32;
        for ox in 0..out {
            let sx = r.x0 + (ox as f32 + 0.5) * r.w / out as f32;
            let inside = sx >= 0.0 && sy >= 0.0 && (sx as u32) < sw && (sy as u32) < sh;
            let px = if inside {
                src.get_pixel(sx as u32, sy as u32).0
            } else {
                [0, 0, 0]
            };
            for c in px {
                v.push(if signed {
                    c as f32 / 127.5 - 1.0
                } else {
                    c as f32 / 255.0
                });
            }
        }
    }
    v
}

/// Runs one model. Outputs are copied out before the next run.
fn run(
    session: &mut Session,
    data: Vec<f32>,
    shape: Vec<i64>,
    outs: &[&str],
) -> Result<Vec<Vec<f32>>, String> {
    let input = Tensor::<f32>::from_array((shape, data)).map_err(|e| e.to_string())?;
    let outputs = session
        .run(ort::inputs![INPUT => input])
        .map_err(|e| e.to_string())?;
    outs.iter()
        .map(|name| {
            outputs[*name]
                .try_extract_tensor::<f32>()
                .map(|(_, d)| d.to_vec())
                .map_err(|e| e.to_string())
        })
        .collect()
}

/// Upper-body box from the previous frame's visible body points, with 25 percent margin.
fn body_box(prev: &[Pt]) -> Option<Rect> {
    let vis: Vec<&Pt> = prev[..BODY_POINTS].iter().filter(|p| p.vis > 0.5).collect();
    if vis.is_empty() {
        return None;
    }
    let x_min = vis.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    let x_max = vis.iter().map(|p| p.x).fold(f32::MIN, f32::max);
    let y_min = vis.iter().map(|p| p.y).fold(f32::MAX, f32::min);
    let y_max = vis.iter().map(|p| p.y).fold(f32::MIN, f32::max);
    let side = (1.25 * (x_max - x_min).max(y_max - y_min)).max(64.0);
    Some(around((x_min + x_max) / 2.0, (y_min + y_max) / 2.0, side))
}

struct PoseOut {
    pts: Vec<Pt>,
    presence: f32,
}

/// Detector runs for timing only: its boxes are not decoded (no anchor decode yet).
/// Landmarks run on a body-box crop, or on the letterboxed full frame when no previous pose.
fn run_pose(
    det: &mut Session,
    lm: &mut Session,
    img: &RgbImage,
    prev: Option<&[Pt]>,
) -> Result<PoseOut, String> {
    let (w, h) = img.dimensions();
    let det_in = sample(img, letterbox(w, h), 224, true);
    run(det, det_in, vec![1, 224, 224, 3], &[])?;
    let rect = prev.and_then(body_box).unwrap_or_else(|| letterbox(w, h));
    let out = run(
        lm,
        sample(img, rect, 256, false),
        vec![1, 256, 256, 3],
        &["Identity", "Identity_1"],
    )?;
    let v = &out[0];
    let pts = (0..POSE_POINTS)
        .map(|i| Pt {
            x: rect.x0 + v[i * 5] * rect.w / 256.0,
            y: rect.y0 + v[i * 5 + 1] * rect.h / 256.0,
            vis: sig(v[i * 5 + 3]),
        })
        .collect();
    Ok(PoseOut {
        pts,
        presence: sig(out[1][0]),
    })
}

struct HandOut {
    pts: Vec<Pt>,
    score_raw: f32,
    score: f32,
    handed_raw: f32,
    handed: f32,
}

/// Handpose on one rect. Output landmarks are in the 224 input pixel space.
fn run_hand(hand: &mut Session, img: &RgbImage, rect: Rect) -> Result<HandOut, String> {
    let out = run(
        hand,
        sample(img, rect, 224, false),
        vec![1, 224, 224, 3],
        &["Identity", "Identity_1", "Identity_2"],
    )?;
    let v = &out[0];
    let pts = (0..HAND_POINTS)
        .map(|i| Pt {
            x: rect.x0 + v[i * 3] * rect.w / 224.0,
            y: rect.y0 + v[i * 3 + 1] * rect.h / 224.0,
            vis: 1.0,
        })
        .collect();
    Ok(HandOut {
        pts,
        score_raw: out[1][0],
        score: sig(out[1][0]),
        handed_raw: out[2][0],
        handed: sig(out[2][0]),
    })
}

/// Mean wrist-to-fingertip distance over mean wrist-to-knuckle distance.
/// Open palm is high, fist is low. A proxy, not a calibrated curl angle.
fn extension(pts: &[Pt]) -> f32 {
    let d = |i: usize| ((pts[i].x - pts[0].x).powi(2) + (pts[i].y - pts[0].y).powi(2)).sqrt();
    let tips: Vec<f32> = [4, 8, 12, 16, 20].iter().map(|&i| d(i)).collect();
    let mcps: Vec<f32> = [5, 9, 13, 17].iter().map(|&i| d(i)).collect();
    mean(&tips) / mean(&mcps).max(1e-6)
}

fn load(dir: &Path, name: &str, threads: usize) -> Result<(Session, usize), String> {
    build_session(&dir.join(name), threads).map_err(|e| format!("{name}: {e}"))
}

struct PoseModels {
    det: Session,
    lm: Session,
}

type FaceTracker = Tracker<Detector, LandmarkModel, BlendshapeModel>;

fn load_face(threads: usize) -> Result<FaceTracker, String> {
    let dir = Path::new(FACE_MODELS);
    let detect = load(dir, "face_detector.onnx", threads)?.0;
    let landmarks = load(dir, "face_landmarks.onnx", threads)?.0;
    let blend = load(dir, "face_blendshapes.onnx", threads)?.0;
    let mut tracker = Tracker::new(
        Detector::new(detect),
        LandmarkModel::new(landmarks),
        BlendshapeModel::new(blend),
    );
    tracker.set_mirror(true);
    Ok(tracker)
}

struct Ctx {
    pose: Option<PoseModels>,
    hand: Option<Session>,
    face: Option<FaceTracker>,
    prev: Option<Vec<Pt>>,
}

struct Sample {
    body: Option<Vec<Pt>>,
    hands: Vec<HandOut>,
}

/// One frame through the configured stages. Returns the timed milliseconds.
/// The Frame copy for the face tracker is built before the timer starts.
fn process(ctx: &mut Ctx, img: &RgbImage) -> Result<(f64, Sample), String> {
    let (w, h) = img.dimensions();
    let frame = ctx.face.as_ref().map(|_| Frame {
        width: w,
        height: h,
        format: PixelFormat::Rgb24,
        bytes: img.as_raw().clone(),
        arrived: Instant::now(),
    });
    let wrists = ctx.prev.as_ref().map(|p| [p[15], p[16]]);
    let mut sample = Sample {
        body: None,
        hands: Vec::new(),
    };
    let t0 = Instant::now();
    if let Some(p) = ctx.pose.as_mut() {
        let out = run_pose(&mut p.det, &mut p.lm, img, ctx.prev.as_deref())?;
        ctx.prev = Some(out.pts.clone());
        sample.body = Some(out.pts);
    }
    if let Some(hs) = ctx.hand.as_mut() {
        let rects: Vec<Rect> = if ctx.pose.is_none() {
            vec![Rect {
                x0: 0.0,
                y0: 0.0,
                w: w as f32,
                h: h as f32,
            }]
        } else {
            match wrists {
                Some(ws) => {
                    let side = (0.4 * w.max(h) as f32).max(64.0);
                    ws.iter().map(|p| around(p.x, p.y, side)).collect()
                }
                None => vec![letterbox(w, h)],
            }
        };
        for r in rects {
            sample.hands.push(run_hand(hs, img, r)?);
        }
    }
    if let (Some(f), Some(fr)) = (ctx.face.as_mut(), frame.as_ref()) {
        f.step(fr)?;
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    Ok((ms, sample))
}

fn draw_dot(img: &mut RgbImage, x: f32, y: f32, color: [u8; 3]) {
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            let px = x as i32 + dx;
            let py = y as i32 + dy;
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                img.put_pixel(px as u32, py as u32, Rgb(color));
            }
        }
    }
}

/// Body points with vis > 0.5 in cyan. Hand points in green when the score passes 0.5, red otherwise.
fn draw_sample(img: &mut RgbImage, s: &Sample) {
    if let Some(body) = &s.body {
        for p in body[..BODY_POINTS].iter().filter(|p| p.vis > 0.5) {
            draw_dot(img, p.x, p.y, [0, 255, 255]);
        }
    }
    for hand in &s.hands {
        let color = if hand.score > 0.5 { [0, 255, 0] } else { [255, 0, 0] };
        for p in &hand.pts {
            draw_dot(img, p.x, p.y, color);
        }
    }
}

fn decode(path: &Path) -> Result<RgbImage, String> {
    image::open(path)
        .map(|i| i.to_rgb8())
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((p * sorted.len() as f64).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[idx]
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 0 {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

/// `--body-bench pose|hand|full THREADS`. Prints one RESULT line.
/// Warm-up: the first WARMUP frames run untimed. Then every golden frame is timed once.
fn bench(args: &[String]) -> Result<(), String> {
    let config = args.first().map(String::as_str).unwrap_or("");
    if !matches!(config, "pose" | "hand" | "full") {
        return Err("usage: --body-bench pose|hand|full THREADS".to_string());
    }
    let threads: usize = args
        .get(1)
        .and_then(|t| t.parse().ok())
        .ok_or("usage: --body-bench pose|hand|full THREADS")?;
    let upper = Path::new(UPPER);
    let use_pose = config != "hand";
    let use_hand = config != "pose";
    let use_face = config == "full";

    let mut actual = 0usize;
    let pose = if use_pose {
        let (det, t) = load(upper, POSE_DETECTOR, threads)?;
        let (lm, _) = load(upper, POSE_LANDMARKS, threads)?;
        actual = t;
        Some(PoseModels { det, lm })
    } else {
        None
    };
    let hand = if use_hand {
        let (s, t) = load(upper, HANDPOSE, threads)?;
        if actual == 0 {
            actual = t;
        }
        Some(s)
    } else {
        None
    };
    let face = if use_face {
        Some(load_face(threads)?)
    } else {
        None
    };
    let mut ctx = Ctx {
        pose,
        hand,
        face,
        prev: None,
    };

    let mut frames: Vec<PathBuf> = std::fs::read_dir(FRAMES)
        .map_err(|e| format!("{FRAMES}: {e}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n.starts_with('f') && n.ends_with(".jpg"))
        })
        .collect();
    frames.sort();

    for path in frames.iter().take(WARMUP) {
        let img = decode(path)?;
        process(&mut ctx, &img)?;
    }
    ctx.prev = None;

    let mut times = Vec::with_capacity(frames.len());
    let mut hip = Vec::new();
    let mut wrist = Vec::new();
    let mut body_visible = Vec::new();
    let mut hand_scores = Vec::new();
    let mut drawn: Vec<(usize, RgbImage, Sample)> = Vec::new();
    for (i, path) in frames.iter().enumerate() {
        let img = decode(path)?;
        let (ms, s) = process(&mut ctx, &img)?;
        times.push(ms);
        if let Some(b) = &s.body {
            hip.push((b[23].vis + b[24].vis) / 2.0);
            wrist.push((b[15].vis + b[16].vis) / 2.0);
            body_visible.push(b[..BODY_POINTS].iter().filter(|p| p.vis > 0.5).count() as f32);
        }
        hand_scores.extend(s.hands.iter().map(|h| h.score));
        if DRAW_FRAMES.contains(&i) {
            drawn.push((i, img, s));
        }
    }

    std::fs::create_dir_all(OUT).map_err(|e| e.to_string())?;
    for (i, mut img, s) in drawn {
        draw_sample(&mut img, &s);
        let path = Path::new(OUT).join(format!("{config}_frame{i:03}.png"));
        img.save(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("drew {}", path.display());
    }

    let total_s: f64 = times.iter().sum::<f64>() / 1000.0;
    let mut sorted = times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mem = process_memory().map_err(|e| e.to_string())?;
    println!(
        "RESULT config={config} threads={actual} frames={} median_ms={:.2} p95_ms={:.2} fps={:.1} peak_ws_mb={:.0} private_mb={:.0} body_visible_mean={:.1} hip_vis={:.3} wrist_vis={:.3} hand_score_mean={:.3}",
        times.len(),
        median(&sorted),
        percentile(&sorted, 0.95),
        times.len() as f64 / total_s,
        mem.peak_working_set_mb,
        mem.peak_private_mb,
        mean(&body_visible),
        mean(&hip),
        mean(&wrist),
        mean(&hand_scores),
    );
    Ok(())
}

/// `--body-photos PATH...`. Pose on the letterboxed photo, then handpose on a crop around each
/// wrist (both wrists, regardless of visibility). Prints per-photo facts and writes a PNG.
fn photos(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("usage: --body-photos PATH...".to_string());
    }
    let upper = Path::new(UPPER);
    let (det_s, _) = load(upper, POSE_DETECTOR, 1)?;
    let (lm_s, _) = load(upper, POSE_LANDMARKS, 1)?;
    let (hand_s, _) = load(upper, HANDPOSE, 1)?;
    let mut pose = PoseModels { det: det_s, lm: lm_s };
    let mut hand = hand_s;
    std::fs::create_dir_all(OUT).map_err(|e| e.to_string())?;

    for arg in args {
        let path = Path::new(arg);
        let img = decode(path)?;
        let (w, h) = img.dimensions();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("photo");
        let pose_out = run_pose(&mut pose.det, &mut pose.lm, &img, None)?;
        println!("== {stem} {w}x{h}");
        println!("pose presence={:.3}", pose_out.presence);
        for (name, i) in [
            ("L shoulder", 11),
            ("R shoulder", 12),
            ("L elbow", 13),
            ("R elbow", 14),
            ("L wrist", 15),
            ("R wrist", 16),
            ("L hip", 23),
            ("R hip", 24),
        ] {
            let p = pose_out.pts[i];
            println!(
                "  {name:<11} px=({:.0},{:.0}) vis={:.3}",
                p.x, p.y, p.vis
            );
        }
        let side = (0.4 * w.max(h) as f32).max(64.0);
        let mut sample = Sample {
            body: Some(pose_out.pts.clone()),
            hands: Vec::new(),
        };
        for (name, i) in [("left-wrist crop", 15), ("right-wrist crop", 16)] {
            let p = pose_out.pts[i];
            let hd = run_hand(&mut hand, &img, around(p.x, p.y, side))?;
            println!(
                "  {name}: score={:.3} (raw {:.2}) handed={:.3} (raw {:.2}) extension={:.2}",
                hd.score,
                hd.score_raw,
                hd.handed,
                hd.handed_raw,
                extension(&hd.pts)
            );
            sample.hands.push(hd);
        }
        let mut canvas = img.clone();
        draw_sample(&mut canvas, &sample);
        let out = Path::new(OUT).join(format!("photo_{stem}.png"));
        canvas.save(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("  drew {}", out.display());
    }
    Ok(())
}
