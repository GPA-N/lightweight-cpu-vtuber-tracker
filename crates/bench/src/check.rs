//! One-shot image check for task 4.7: runs the tracker stages on one still image
//! and prints first-run diagnostics. Verdict lines decide the Ok/Err result.
#![allow(clippy::items_after_test_module)]

use std::path::Path;
use std::time::Instant;

use core::frame::{Frame, PixelFormat};
use core::names::BLENDSHAPE_NAMES;
use facelogic::crop::{rect_from_detection, warp_to_tensor};
use infer::blendshapes::BlendshapeModel;
use infer::detector::{Detector, MIN_SCORE};
use infer::landmarks::{landmarks_to_frame, LandmarkModel};
use infer::session::build_session;
use pose::fit::{fit_head_rotation, yaw_pitch_roll};

const CROP_SIZE: usize = 256;
const LANDMARK_COUNT: usize = 478;
const NOSE: usize = 4;
const EYE_A: usize = 33;
const EYE_B: usize = 263;

/// Count points inside [0, w) x [0, h), full-frame pixels.
pub fn inside_count(points: &[[f32; 3]], w: f32, h: f32) -> usize {
    points
        .iter()
        .filter(|p| p[0] >= 0.0 && p[0] < w && p[1] >= 0.0 && p[1] < h)
        .count()
}

/// One CHECK line: values, expectation, verdict.
pub fn format_check(name: &str, values: &str, expected: &str, ok: bool) -> String {
    let verdict = if ok { "OK" } else { "UNEXPECTED" };
    format!("CHECK {name}: {values} -> EXPECTED {expected} -> {verdict}")
}

/// True when every weight is within 0.0..=1.0.
pub fn weights_in_range(weights: &[f32]) -> bool {
    weights.iter().all(|w| (0.0..=1.0).contains(w))
}

pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn yes_no(ok: bool) -> &'static str {
    if ok {
        "yes"
    } else {
        "no"
    }
}

fn min_max(values: impl Iterator<Item = f32>) -> (f32, f32) {
    values.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

fn infer_names() -> &'static [&'static str; 52] {
    &core::names::MEDIAPIPE_OUTPUT_NAMES
}

pub fn run_image_check(image: &Path, models_dir: &Path, threads: usize) -> Result<(), String> {
    let rgb = image::open(image)
        .map_err(|e| format!("open image: {e}"))?
        .to_rgb8();
    let (w, h) = rgb.dimensions();
    let frame = Frame {
        width: w,
        height: h,
        format: PixelFormat::Rgb24,
        bytes: rgb.into_raw(),
        arrived: Instant::now(),
    };

    let (det_session, det_threads) = build_session(&models_dir.join("face_detector.onnx"), threads)
        .map_err(|e| format!("face_detector.onnx: {e}"))?;
    let (lm_session, _) = build_session(&models_dir.join("face_landmarks.onnx"), threads)
        .map_err(|e| format!("face_landmarks.onnx: {e}"))?;
    let (bs_session, _) = build_session(&models_dir.join("face_blendshapes.onnx"), threads)
        .map_err(|e| format!("face_blendshapes.onnx: {e}"))?;
    println!("sessions loaded, threads {det_threads}");

    // Detector.
    let mut detector = Detector::new(det_session);
    let dets = detector.detect(&frame).map_err(|e| e.to_string())?;
    let Some(det) = dets.first() else {
        return Err("NO FACE DETECTED".to_string());
    };
    println!("detector: {} detection(s), scores {:?}", dets.len(), dets.iter().map(|d| d.score).collect::<Vec<_>>());
    println!(
        "{}",
        format_check(
            "detector score",
            &format!("{:.4}", det.score),
            &format!(">= {MIN_SCORE}"),
            det.score >= MIN_SCORE,
        )
    );

    // Crop and landmarks.
    let rect = rect_from_detection(det.box_norm, det.keypoints[0], det.keypoints[1], w, h);
    let tensor = warp_to_tensor(&frame.bytes, w, h, &rect, CROP_SIZE);
    let (tmin, tmax) = min_max(tensor.iter().copied());
    println!(
        "{}",
        format_check(
            "input range",
            &format!("min {tmin:.4} max {tmax:.4} (frame/255, no switch)"),
            "0..1",
            tmin >= 0.0 && tmax <= 1.0,
        )
    );

    let mut landmarks = LandmarkModel::new(lm_session);
    let out = landmarks.run(&tensor).map_err(|e| format!("{e:?}"))?;
    let raw = &out.raw_outputs;
    println!(
        "landmark raw: logit {:.4} probability {:.4} sigmoid(logit) {:.4}",
        raw.logit, raw.probability, raw.sigmoid_logit
    );
    println!(
        "{}",
        format_check(
            "presence",
            &format!("probability {:.4} sigmoid(logit) {:.4}", raw.probability, sigmoid(raw.logit)),
            "probability equals sigmoid(logit) within 1e-3",
            (raw.probability - sigmoid(raw.logit)).abs() < 1e-3,
        )
    );
    let (xmin, xmax) = min_max(out.points.iter().map(|p| p[0]));
    let (ymin, ymax) = min_max(out.points.iter().map(|p| p[1]));
    let (zmin, zmax) = min_max(out.points.iter().map(|p| p[2]));
    println!("raw landmark min/max: x {xmin:.3}..{xmax:.3} y {ymin:.3}..{ymax:.3} z {zmin:.3}..{zmax:.3}");

    // Raw z sign and scale (first-run checks A1, A2) and x handedness.
    let p = &out.points;
    let mean_eye_z = (p[EYE_A][2] + p[EYE_B][2]) / 2.0;
    println!(
        "{}",
        format_check(
            "z sign",
            &format!("z4 {:.3} mean(z33,z263) {:.3}", p[NOSE][2], mean_eye_z),
            "z4 - mean(z33,z263) < 0",
            p[NOSE][2] - mean_eye_z < 0.0,
        )
    );
    let eye_dx = (p[EYE_B][0] - p[EYE_A][0]).abs();
    let nose_dz = (p[NOSE][2] - p[EYE_A][2]).abs();
    let ratio = eye_dx / nose_dz;
    println!(
        "{}",
        format_check(
            "z scale ratio",
            &format!("|x263-x33| {eye_dx:.3} |z4-z33| {nose_dz:.3} ratio {ratio:.3}"),
            "about 2.01 (1.5..2.5)",
            (1.5..=2.5).contains(&ratio),
        )
    );

    // Full-frame pixel coordinates for the blendshape model and the fit.
    let px = landmarks_to_frame(&out.points, &rect);
    if std::env::var_os("DUMP_LM").is_some() {
        for p in &px {
            println!("LM {} {}", p[0], p[1]);
        }
    }
    let (x33, x263) = (px[EYE_A], px[EYE_B]);
    let dist = ((x263[0] - x33[0]).powi(2) + (x263[1] - x33[1]).powi(2)).sqrt();
    println!("eye distance 33-263 in frame px: {dist:.2}");
    println!(
        "{}",
        format_check(
            "x handedness",
            &format!("x33 {:.2} x263 {:.2}", x33[0], x263[0]),
            "x33 < x263",
            x33[0] < x263[0],
        )
    );
    let inside = inside_count(&px, w as f32, h as f32);

    // Blendshapes.
    let mut blend = BlendshapeModel::new(bs_session);
    let weights = blend.run(&px).map_err(|e| e.to_string())?;
    if std::env::var_os("DUMP_LM").is_some() {
        let raw = blend.run_raw(&px).map_err(|e| e.to_string())?;
        let outside = raw.iter().filter(|v| !(0.0..=1.0).contains(*v)).count();
        println!("RAW outside 0..1: {outside} of {}", raw.len());
        for (name, v) in infer_names().iter().zip(&raw) {
            println!("RAW {name} {v:.4}");
        }
    }
    let mut ranked: Vec<usize> = (0..weights.len()).collect();
    ranked.sort_by(|&a, &b| weights[b].total_cmp(&weights[a]));
    println!("blendshapes top 10:");
    for &i in ranked.iter().take(10) {
        println!("  {:>2} {:<18} {:.4}", i, BLENDSHAPE_NAMES[i], weights[i]);
    }
    if std::env::var_os("DUMP_LM").is_some() {
        for (i, w) in weights.iter().enumerate() {
            println!("W {} {w:.4}", BLENDSHAPE_NAMES[i]);
        }
    }
    let (wmin, wmax) = min_max(weights.iter().copied());
    println!("blendshapes min/max: {wmin:.4} {wmax:.4}");

    if std::env::var_os("DUMP_LM").is_some() {
        for (name, v) in pose::fit::residual_variants(&px) {
            println!("RES {name}: {v:.4}");
        }
    }

    // Head pose. A FitError is reported but does not decide the verdict.
    match fit_head_rotation(&px) {
        Ok(fit) => {
            let (yaw, pitch, roll) = yaw_pitch_roll(&fit.rotation);
            println!(
                "{}",
                format_check(
                    "head fit",
                    &format!(
                        "yaw {yaw:.2} pitch {pitch:.2} roll {roll:.2} scale {:.4} residual {:.4}",
                        fit.scale, fit.residual
                    ),
                    "finite angles and residual",
                    [yaw, pitch, roll, fit.scale, fit.residual].iter().all(|v| v.is_finite()),
                )
            );
        }
        Err(e) => println!(
            "{}",
            format_check("head fit", &format!("{e:?}"), "a fit result", false)
        ),
    }

    // Verdicts.
    let lm_ok = inside == LANDMARK_COUNT;
    let bs_ok = weights_in_range(&weights);
    println!(
        "PASS {LANDMARK_COUNT} landmarks inside image bounds: {} (count inside {inside}/{LANDMARK_COUNT})",
        yes_no(lm_ok)
    );
    println!("PASS 52 blendshape weights within 0.0..1.0: {}", yes_no(bs_ok));
    if lm_ok && bs_ok {
        Ok(())
    } else {
        Err("image check failed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inside_count_counts_only_points_within_bounds() {
        let pts = [[0.0, 0.0, 9.0], [255.9, 255.9, 0.0], [256.0, 0.0, 0.0], [-0.1, 5.0, 0.0]];
        assert_eq!(inside_count(&pts, 256.0, 256.0), 2);
    }

    #[test]
    fn format_check_prints_verdict() {
        assert_eq!(
            format_check("n", "1.0", "< 2", true),
            "CHECK n: 1.0 -> EXPECTED < 2 -> OK"
        );
        assert_eq!(
            format_check("n", "3.0", "< 2", false),
            "CHECK n: 3.0 -> EXPECTED < 2 -> UNEXPECTED"
        );
    }

    #[test]
    fn weights_in_range_accepts_only_unit_interval() {
        assert!(weights_in_range(&[0.0, 0.5, 1.0]));
        assert!(!weights_in_range(&[0.2, 1.01]));
        assert!(!weights_in_range(&[-0.01]));
        assert!(!weights_in_range(&[f32::NAN]));
    }

    #[test]
    fn sigmoid_matches_known_values() {
        assert_eq!(sigmoid(0.0), 0.5);
        assert!((sigmoid(2.0) - 0.880_797).abs() < 1e-5);
        assert!(sigmoid(40.0) > 0.999_999);
        assert!(sigmoid(-40.0) < 1e-6);
    }
}

/// Timing run on one still image: runs the full tracker `repeat` times and prints latency,
/// memory and CPU. No camera, so it measures inference only.
pub fn run_image_timing(image: &Path, models_dir: &Path, threads: usize, repeat: usize) -> Result<(), String> {
    use infer::blendshapes::BlendshapeModel;
    use infer::detector::Detector;
    use infer::tracker::Tracker;
    use timing::cpu::CpuSampler;
    use timing::memory::process_memory;
    use timing::stats::{median, p95};

    let rgb = image::open(image).map_err(|e| format!("open image: {e}"))?.to_rgb8();
    let (w, h) = rgb.dimensions();
    let bytes = rgb.into_raw();
    let load = |name: &str| build_session(&models_dir.join(name), threads).map_err(|e| format!("{name}: {e}"));
    let (d, used) = load("face_detector.onnx")?;
    let (l, _) = load("face_landmarks.onnx")?;
    let (b, _) = load("face_blendshapes.onnx")?;
    let mut tracker = Tracker::new(Detector::new(d), LandmarkModel::new(l), BlendshapeModel::new(b));
    let mk = || Frame { width: w, height: h, format: PixelFormat::Rgb24, bytes: bytes.clone(), arrived: Instant::now() };
    for _ in 0..5 {
        tracker.step(&mk())?;
    }
    // BENCH_PACE_MS paces frames like a camera (e.g. 66 for 15 fps) so idle CPU (spinning) shows up.
    let pace = std::env::var("BENCH_PACE_MS").ok().and_then(|v| v.parse::<u64>().ok()).map(std::time::Duration::from_millis);
    let cpu = CpuSampler::start().map_err(|e| e.to_string())?;
    let mut lat = Vec::with_capacity(repeat);
    let start = Instant::now();
    for _ in 0..repeat {
        let frame = mk();
        let t0 = Instant::now();
        tracker.step(&frame)?;
        let took = t0.elapsed();
        lat.push(took);
        if let Some(p) = pace {
            std::thread::sleep(p.saturating_sub(took));
        }
    }
    let elapsed = start.elapsed();
    let mem = process_memory().map_err(|e| e.to_string())?;
    println!("image {w}x{h}, onnx threads {used}, {repeat} frames back to back");
    println!("latency median {:?}  p95 {:?}", median(&lat).unwrap_or_default(), p95(&lat).unwrap_or_default());
    println!("throughput {:.1} frames/s", repeat as f64 / elapsed.as_secs_f64());
    println!("peak working set {:.0} MB  peak private {:.0} MB", mem.peak_working_set_mb, mem.peak_private_mb);
    println!("avg CPU of this process while saturated: {:.0}% of one core ({:.0}% of all logical cores)", cpu.percent().unwrap_or(f64::NAN), cpu.percent_of_logical_cores().unwrap_or(f64::NAN));
    Ok(())
}

/// Records `seconds` of live camera into `out_dir`: one JPEG per frame (`f0000.jpg`), the 478
/// landmarks per frame (`lm.txt`), and the tracker output per frame (`track.txt`).
/// Offline helper for making a side-by-side video; not part of the measured pipeline.
pub fn run_record(models_dir: &Path, threads: usize, camera: u32, seconds: f64, out_dir: &Path) -> Result<(), String> {
    use std::io::Write;

    use capture::thread::{FrameSource, NokhwaSource};
    use infer::tracker::Tracker;

    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let load = |name: &str| build_session(&models_dir.join(name), threads).map_err(|e| format!("{name}: {e}"));
    let (d, _) = load("face_detector.onnx")?;
    let (l, _) = load("face_landmarks.onnx")?;
    let (b, _) = load("face_blendshapes.onnx")?;
    let mut tracker = Tracker::new(Detector::new(d), LandmarkModel::new(l), BlendshapeModel::new(b));
    let mut source = NokhwaSource::open(camera, 640, 480, 30).map_err(|e| e.to_string())?;
    let mut track = std::fs::File::create(out_dir.join("track.txt")).map_err(|e| e.to_string())?;
    let mut lm_file = std::fs::File::create(out_dir.join("lm.txt")).map_err(|e| e.to_string())?;

    let start = Instant::now();
    let mut i = 0usize;
    let mut found_count = 0usize;
    let (mut wait_ms, mut conv_ms) = (0.0f64, 0.0f64);
    let mut formats = std::collections::BTreeSet::new();
    while start.elapsed().as_secs_f64() < seconds {
        let t_wait = Instant::now();
        let raw = source.next_frame().map_err(|e| e.to_string())?;
        wait_ms += t_wait.elapsed().as_secs_f64() * 1000.0;
        formats.insert(format!("{:?} {}x{}", raw.format, raw.width, raw.height));
        let t_conv = Instant::now();
        let bytes = capture::convert::to_rgb(raw.format, raw.width, raw.height, &raw.bytes).map_err(|e| format!("{e:?}"))?;
        conv_ms += t_conv.elapsed().as_secs_f64() * 1000.0;
        let frame = Frame { width: raw.width, height: raw.height, format: PixelFormat::Rgb24, bytes, arrived: Instant::now() };
        let t0 = Instant::now();
        let out = tracker.step(&frame)?;
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        image::RgbImage::from_raw(frame.width, frame.height, frame.bytes.clone())
            .ok_or("bad frame size")?
            .save(out_dir.join(format!("f{i:04}.jpg")))
            .map_err(|e| e.to_string())?;
        let q = out.head_rotation.coords;
        let mut line = format!("{i} {:.3} {} {} {} {} {} {ms:.1}", start.elapsed().as_secs_f64(), out.face_found as u8, q[0], q[1], q[2], q[3]);
        for w in out.weights {
            line.push_str(&format!(" {w}"));
        }
        writeln!(track, "{line}").map_err(|e| e.to_string())?;
        let mut lm_line = format!("{i}");
        if out.face_found {
            found_count += 1;
            for p in tracker.last_landmarks().unwrap_or(&[]) {
                lm_line.push_str(&format!(" {:.1} {:.1}", p[0], p[1]));
            }
        }
        writeln!(lm_file, "{lm_line}").map_err(|e| e.to_string())?;
        i += 1;
    }
    println!("camera formats {formats:?}; per frame: next_frame wait {:.1} ms, to_rgb {:.1} ms", wait_ms / i as f64, conv_ms / i as f64);
    println!("recorded {i} frames in {:.1}s ({:.1} fps), face found in {found_count}", start.elapsed().as_secs_f64(), i as f64 / start.elapsed().as_secs_f64());
    Ok(())
}
