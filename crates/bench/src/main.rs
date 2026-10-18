mod body_bench;
mod check;
mod pipeline;

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use capture::slot::FrameSlot;
use capture::thread::{spawn_capture, FrameSource, NokhwaSource};
use infer::blendshapes::BlendshapeModel;
use infer::detector::Detector;
use infer::landmarks::LandmarkModel;
use infer::session::{build_session, default_threads};
use infer::tracker::Tracker;
use pipeline::{spawn_inference, tracking_fps, PipelineStats, TrackingShare};
use timing::cpu::CpuSampler;
use timing::hardware::{cpu_model, power_state, total_ram_mb, PowerState};
use timing::memory::process_memory;
use timing::report::Report;

const USAGE: &str = "usage: bench [--seconds N] [--camera INDEX] [--threads N] [--models DIR] [--no-mirror] [--check-image PATH] [--help]";
const CAMERA_WIDTH: u32 = 640;
const CAMERA_HEIGHT: u32 = 480;
const CAMERA_FPS: u32 = 30;
const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, PartialEq)]
struct Options {
    seconds: u64,
    camera: u32,
    threads: usize,
    models: PathBuf,
    mirror: bool,
    check_image: Option<PathBuf>,
}

#[derive(Debug, PartialEq)]
enum Command {
    Help,
    Run(Options),
}

fn value<'a>(args: &mut std::slice::Iter<'a, String>, flag: &str) -> Result<&'a str, String> {
    args.next()
        .map(|s| s.as_str())
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn number<T: FromStr>(text: &str, flag: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("invalid value for {flag}: {text}"))
}

fn parse_args(args: &[String], default_threads: usize) -> Result<Command, String> {
    let mut opts = Options {
        seconds: 60,
        camera: 0,
        threads: default_threads,
        models: PathBuf::from("models"),
        mirror: true,
        check_image: None,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" => return Ok(Command::Help),
            "--no-mirror" => opts.mirror = false,
            "--seconds" => {
                let seconds: u64 = number(value(&mut iter, arg)?, arg)?;
                if seconds == 0 {
                    return Err(format!("invalid value for {arg}: 0"));
                }
                opts.seconds = seconds;
            }
            "--camera" => opts.camera = number(value(&mut iter, arg)?, arg)?,
            "--threads" => opts.threads = number(value(&mut iter, arg)?, arg)?,
            "--models" => opts.models = PathBuf::from(value(&mut iter, arg)?),
            "--check-image" => opts.check_image = Some(PathBuf::from(value(&mut iter, arg)?)),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Command::Run(opts))
}

/// Run-level facts that are not in the pipeline counters.
struct Env {
    onnx_threads: u32,
    avg_cpu_percent: Option<f64>,
    peak_working_set_mb: Option<f64>,
    peak_private_mb: Option<f64>,
    cpu_model: String,
    ram_mb: Option<f64>,
    power_state: String,
}

fn build_report(
    stats: &PipelineStats,
    elapsed: Duration,
    delivered_camera_fps: Option<f64>,
    env: &Env,
) -> Report {
    let latencies = stats.latencies.lock().unwrap().clone();
    let mut report = Report::from_samples(
        &latencies,
        delivered_camera_fps,
        Some(tracking_fps(stats, elapsed)),
        None,
    );
    report.onnx_threads = Some(env.onnx_threads);
    report.avg_cpu_percent = env.avg_cpu_percent;
    report.peak_working_set_mb = env.peak_working_set_mb;
    report.peak_private_mb = env.peak_private_mb;
    report.cpu_model = env.cpu_model.clone();
    report.ram_mb = env.ram_mb;
    report.power_state = env.power_state.clone();
    report
}

fn power_label(state: PowerState) -> &'static str {
    match state {
        PowerState::PluggedIn => "plugged in",
        PowerState::OnBattery => "on battery",
        PowerState::Unknown => "unknown",
    }
}

fn run_live(opts: &Options) -> Result<(), String> {
    let model = |name: &str| opts.models.join(name);
    let (detect, threads) =
        build_session(&model("face_detector.onnx"), opts.threads).map_err(|e| e.to_string())?;
    let (landmarks, _) =
        build_session(&model("face_landmarks.onnx"), opts.threads).map_err(|e| e.to_string())?;
    let (blend, _) =
        build_session(&model("face_blendshapes.onnx"), opts.threads).map_err(|e| e.to_string())?;
    let mut tracker = Tracker::new(
        Detector::new(detect),
        LandmarkModel::new(landmarks),
        BlendshapeModel::new(blend),
    );
    tracker.set_mirror(opts.mirror);

    let sampler = CpuSampler::start().map_err(|e| e.to_string())?;
    let slot = Arc::new(FrameSlot::new());
    let camera = opts.camera;
    let mut capture = spawn_capture(
        move || {
            NokhwaSource::open(camera, CAMERA_WIDTH, CAMERA_HEIGHT, CAMERA_FPS)
                .map(|source| Box::new(source) as Box<dyn FrameSource>)
        },
        Arc::clone(&slot),
    );
    let out: TrackingShare = Arc::new(Mutex::new(None));
    let stats = Arc::new(PipelineStats::new());
    let stop = Arc::new(AtomicBool::new(false));
    let inference = spawn_inference(
        tracker,
        Arc::clone(&slot),
        out,
        Arc::clone(&stats),
        Arc::clone(&stop),
    );

    let start = Instant::now();
    let deadline = start + Duration::from_secs(opts.seconds);
    while Instant::now() < deadline && capture.error().is_none() {
        thread::sleep(POLL_INTERVAL);
    }
    let elapsed = start.elapsed();
    let capture_error = capture.error();
    let avg_cpu_percent = sampler.percent().ok();
    let memory = process_memory().ok();
    capture.stop();
    stop.store(true, Ordering::SeqCst);
    inference
        .join()
        .map_err(|_| "inference thread panicked".to_string())?;

    let env = Env {
        onnx_threads: threads as u32,
        avg_cpu_percent,
        peak_working_set_mb: memory.as_ref().map(|m| m.peak_working_set_mb),
        peak_private_mb: memory.as_ref().map(|m| m.peak_private_mb),
        cpu_model: cpu_model(),
        ram_mb: Some(total_ram_mb() as f64),
        power_state: power_label(power_state()).to_string(),
    };
    let report = build_report(&stats, elapsed, Some(capture.delivered_fps() as f64), &env);
    println!("{}", report.to_text());
    let errors = stats.errors.load(Ordering::SeqCst);
    if errors > 0 {
        let last = stats.last_error.lock().unwrap().clone().unwrap_or_default();
        println!("tracker errors: {errors}, last: {last}");
    }
    match capture_error {
        Some(e) => Err(e.to_string()),
        None => Ok(()),
    }
}

fn run(opts: &Options) -> Result<(), String> {
    if let Some(image) = &opts.check_image {
        if let Some(n) = std::env::var("BENCH_REPEAT").ok().and_then(|v| v.parse::<usize>().ok()) {
            return check::run_image_timing(Path::new(image), &opts.models, opts.threads, n);
        }
        return check::run_image_check(Path::new(image), &opts.models, opts.threads);
    }
    if let (Ok(secs), Ok(dir)) = (std::env::var("BENCH_RECORD"), std::env::var("BENCH_OUT")) {
        let secs = secs.parse::<f64>().map_err(|e| e.to_string())?;
        return check::run_record(&opts.models, opts.threads, opts.camera, secs, Path::new(&dir));
    }
    run_live(opts)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = body_bench::dispatch(&args) {
        std::process::exit(code);
    }
    let code = match parse_args(&args, default_threads()) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            0
        }
        Ok(Command::Run(opts)) => match run(&opts) {
            Ok(()) => 0,
            Err(message) => {
                eprintln!("{message}");
                1
            }
        },
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn run_opts(list: &[&str]) -> Options {
        match parse_args(&args(list), 4) {
            Ok(Command::Run(opts)) => opts,
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn defaults_when_no_arguments() {
        assert_eq!(
            run_opts(&[]),
            Options {
                seconds: 60,
                camera: 0,
                threads: 4,
                models: PathBuf::from("models"),
                mirror: true,
                check_image: None,
            }
        );
    }

    #[test]
    fn every_flag_is_parsed() {
        assert_eq!(
            run_opts(&[
                "--seconds", "5", "--camera", "2", "--threads", "3", "--models", "m",
                "--no-mirror", "--check-image", "i.png",
            ]),
            Options {
                seconds: 5,
                camera: 2,
                threads: 3,
                models: PathBuf::from("m"),
                mirror: false,
                check_image: Some(PathBuf::from("i.png")),
            }
        );
    }

    #[test]
    fn help_is_recognised_anywhere() {
        assert_eq!(parse_args(&args(&["--help"]), 4), Ok(Command::Help));
        assert_eq!(parse_args(&args(&["--seconds", "5", "--help"]), 4), Ok(Command::Help));
    }

    #[test]
    fn unknown_argument_is_an_error() {
        assert_eq!(
            parse_args(&args(&["--bogus"]), 4),
            Err("unknown argument: --bogus".to_string())
        );
    }

    #[test]
    fn missing_value_is_an_error() {
        assert_eq!(
            parse_args(&args(&["--seconds"]), 4),
            Err("missing value for --seconds".to_string())
        );
    }

    #[test]
    fn non_numeric_value_is_an_error() {
        assert_eq!(
            parse_args(&args(&["--camera", "x"]), 4),
            Err("invalid value for --camera: x".to_string())
        );
    }

    #[test]
    fn zero_seconds_is_an_error() {
        assert_eq!(
            parse_args(&args(&["--seconds", "0"]), 4),
            Err("invalid value for --seconds: 0".to_string())
        );
    }

    #[test]
    fn report_is_assembled_from_stats() {
        let stats = PipelineStats::new();
        *stats.latencies.lock().unwrap() =
            [10u64, 20, 30].iter().map(|&m| Duration::from_millis(m)).collect();
        stats.frames_tracked.store(60, Ordering::SeqCst);
        let env = Env {
            onnx_threads: 4,
            avg_cpu_percent: Some(12.5),
            peak_working_set_mb: Some(500.0),
            peak_private_mb: Some(400.0),
            cpu_model: "Test CPU".to_string(),
            ram_mb: Some(16384.0),
            power_state: "plugged in".to_string(),
        };
        let report = build_report(&stats, Duration::from_secs(2), Some(30.0), &env);
        assert_eq!(report.median_latency_ms, Some(20.0));
        assert_eq!(report.tracking_fps, Some(30.0));
        assert_eq!(report.delivered_camera_fps, Some(30.0));
        assert_eq!(report.onnx_threads, Some(4));
        assert_eq!(report.cpu_model, "Test CPU");
        let text = report.to_text();
        assert!(text.contains("median tracking latency < 50 ms: PASS"), "{text}");
        assert!(text.contains("peak working set <= 1024 MB: PASS"), "{text}");
    }
}
