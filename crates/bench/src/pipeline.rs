// Task 6.2 (the bench binary's run loop) is the first user of these items, so until
// it lands the bin build sees them as dead code.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use capture::slot::FrameSlot;
use core::frame::{Frame, TrackingOutput};
use infer::tracker::{BlendRun, FaceDetect, LandmarkRun, Tracker};

/// Upper bound on stored latency samples, so a long run cannot grow memory without limit.
const MAX_LATENCY_SAMPLES: usize = 100_000;

/// One inference step: turns a frame into tracking output.
pub trait TrackStep {
    fn step(&mut self, frame: &Frame) -> Result<TrackingOutput, String>;

    /// Full-frame pixel landmarks from the last step, for callers that draw them. None by default.
    fn last_landmarks(&self) -> Option<Vec<[f32; 3]>> {
        None
    }
}

impl<D: FaceDetect, L: LandmarkRun, B: BlendRun> TrackStep for Tracker<D, L, B> {
    fn step(&mut self, frame: &Frame) -> Result<TrackingOutput, String> {
        // Inherent `Tracker::step` (not this trait method).
        Tracker::step(self, frame)
    }

    fn last_landmarks(&self) -> Option<Vec<[f32; 3]>> {
        Tracker::last_landmarks(self).map(|l| l.to_vec())
    }
}

/// Latest tracking output, overwritten by the inference thread and read by the consumer.
pub type TrackingShare = Arc<Mutex<Option<TrackingOutput>>>;

/// Counters shared between the inference thread and the reporter.
pub struct PipelineStats {
    pub latencies: Mutex<Vec<Duration>>,
    pub frames_tracked: AtomicU64,
    /// Number of frames where the tracker returned an error (model failure etc.).
    pub errors: AtomicU64,
    /// Message of the most recent tracker error, so a failing model is visible in the report.
    pub last_error: Mutex<Option<String>>,
    /// Ms blocked in `wait_take` per loop iteration (near 0 when a frame was already waiting).
    pub wait_ms: Mutex<Vec<f32>>,
    /// Ms of each `tracker.step` call.
    pub step_ms: Mutex<Vec<f32>>,
    /// Ms of each `hook` call.
    pub hook_ms: Mutex<Vec<f32>>,
}

impl PipelineStats {
    pub fn new() -> Self {
        Self {
            latencies: Mutex::new(Vec::new()),
            frames_tracked: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            last_error: Mutex::new(None),
            wait_ms: Mutex::new(Vec::new()),
            step_ms: Mutex::new(Vec::new()),
            hook_ms: Mutex::new(Vec::new()),
        }
    }
}

impl Default for PipelineStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Starts the inference thread. It takes the newest frame from `slot`, runs `tracker`
/// on it, publishes the result to `out`, and records arrival-to-ready latency in `stats`.
/// It exits once `stop` is set.
pub fn spawn_inference(
    tracker: impl TrackStep + Send + 'static,
    slot: Arc<FrameSlot>,
    out: TrackingShare,
    stats: Arc<PipelineStats>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    spawn_inference_hooked(tracker, slot, out, stats, stop, |_, _| {})
}

/// Same as `spawn_inference`, and calls `hook` with each frame and the tracker right after the
/// output is published, so latency is not affected by the hook's work.
pub fn spawn_inference_hooked<T: TrackStep + Send + 'static>(
    mut tracker: T,
    slot: Arc<FrameSlot>,
    out: TrackingShare,
    stats: Arc<PipelineStats>,
    stop: Arc<AtomicBool>,
    mut hook: impl FnMut(&Frame, &mut T) + Send + 'static,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let waiting = Instant::now();
            let taken = slot.wait_take(Duration::from_millis(50));
            push_capped(&stats.wait_ms, elapsed_ms(waiting));
            let Some(frame) = taken else {
                continue;
            };
            // A tracker error is counted and kept, and published as "no face" so the
            // consumer always has a valid output.
            let stepping = Instant::now();
            let output = match tracker.step(&frame) {
                Ok(output) => output,
                Err(message) => {
                    stats.errors.fetch_add(1, Ordering::SeqCst);
                    *stats.last_error.lock().unwrap() = Some(message);
                    TrackingOutput {
                        weights: [0.0; 52],
                        head_rotation: Default::default(),
                        face_found: false,
                        ready: Instant::now(),
                    }
                }
            };
            push_capped(&stats.step_ms, elapsed_ms(stepping));
            let latency = output
                .ready
                .checked_duration_since(frame.arrived)
                .unwrap_or_default();
            record_latency(&stats, latency);
            *out.lock().unwrap() = Some(output);
            stats.frames_tracked.fetch_add(1, Ordering::SeqCst);
            let hooking = Instant::now();
            hook(&frame, &mut tracker);
            push_capped(&stats.hook_ms, elapsed_ms(hooking));
        }
    })
}

fn record_latency(stats: &PipelineStats, latency: Duration) {
    let mut latencies = stats.latencies.lock().unwrap();
    if latencies.len() < MAX_LATENCY_SAMPLES {
        latencies.push(latency);
    }
}

fn elapsed_ms(since: Instant) -> f32 {
    since.elapsed().as_secs_f32() * 1000.0
}

fn push_capped(samples: &Mutex<Vec<f32>>, ms: f32) {
    let mut v = samples.lock().unwrap();
    if v.len() < MAX_LATENCY_SAMPLES {
        v.push(ms);
    }
}

/// Frames tracked per second over `elapsed`; 0.0 when no time has passed.
pub fn tracking_fps(stats: &PipelineStats, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 {
        return 0.0;
    }
    stats.frames_tracked.load(Ordering::SeqCst) as f64 / secs
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::frame::PixelFormat;
    use std::thread;
    use std::time::{Duration, Instant};

    /// Fake tracker: sleeps `cost` per frame, records the sequence id it saw
    /// (carried in `width`), and encodes that id in `weights[0]`.
    struct FakeTracker {
        cost: Duration,
        seen: Arc<Mutex<Vec<u32>>>,
    }

    impl TrackStep for FakeTracker {
        fn step(&mut self, frame: &Frame) -> Result<TrackingOutput, String> {
            thread::sleep(self.cost);
            self.seen.lock().unwrap().push(frame.width);
            let mut weights = [0.0; 52];
            weights[0] = frame.width as f32;
            Ok(TrackingOutput {
                weights,
                head_rotation: Default::default(),
                face_found: true,
                ready: Instant::now(),
            })
        }
    }

    fn frame(seq: u32, arrived: Instant) -> Frame {
        Frame {
            width: seq,
            height: 1,
            format: PixelFormat::Rgb24,
            bytes: Vec::new(),
            arrived,
        }
    }

    #[test]
    fn pipeline_meets_latency_drop_and_count_targets() {
        let slot = Arc::new(FrameSlot::new());
        let out: TrackingShare = Arc::new(Mutex::new(None));
        let stats = Arc::new(PipelineStats::new());
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(Mutex::new(Vec::new()));

        let handle = spawn_inference(
            FakeTracker { cost: Duration::from_millis(5), seen: Arc::clone(&seen) },
            Arc::clone(&slot),
            Arc::clone(&out),
            Arc::clone(&stats),
            Arc::clone(&stop),
        );

        // Fake source: 1000 frames, one every 3.3 ms, on a fixed schedule.
        let producer = {
            let slot = Arc::clone(&slot);
            thread::spawn(move || {
                let start = Instant::now();
                for seq in 1..=1000u32 {
                    let due = start + Duration::from_micros(3300 * seq as u64);
                    let now = Instant::now();
                    if due > now {
                        thread::sleep(due - now);
                    }
                    slot.put(frame(seq, Instant::now()));
                }
            })
        };
        producer.join().unwrap();
        // Let the consumer take the last frame before stopping.
        thread::sleep(Duration::from_millis(100));
        stop.store(true, Ordering::SeqCst);
        let joined = Instant::now();
        handle.join().unwrap();
        assert!(joined.elapsed() < Duration::from_secs(1), "thread did not stop promptly");

        let latencies = stats.latencies.lock().unwrap().clone();
        let tracked = stats.frames_tracked.load(Ordering::SeqCst);
        assert_eq!(latencies.len() as u64, tracked);
        assert!(latencies.iter().all(|d| *d < Duration::from_millis(100)), "a latency exceeded 100 ms");

        let mut sorted = latencies.clone();
        sorted.sort();
        let median = sorted[sorted.len() / 2];
        assert!(median < Duration::from_millis(30), "median {median:?} not below 30 ms");

        let first = median_of(&latencies[..100]);
        let last = median_of(&latencies[latencies.len() - 100..]);
        assert!(
            last <= first + Duration::from_millis(20),
            "latency grew over the run: first {first:?}, last {last:?}"
        );

        let dropped = slot.dropped();
        assert!(dropped > 0, "no frames were dropped");
        let accounted = tracked + dropped;
        assert!((998..=1002).contains(&accounted), "tracked + dropped = {accounted}");

        let seen = seen.lock().unwrap();
        let last_seen = *seen.last().expect("some frames tracked");
        let published = out.lock().unwrap().clone().expect("an output was published");
        assert_eq!(published.weights[0] as u32, last_seen);
    }

    fn median_of(values: &[Duration]) -> Duration {
        let mut v = values.to_vec();
        v.sort();
        v[v.len() / 2]
    }

    #[test]
    fn latency_samples_are_capped() {
        let stats = PipelineStats::new();
        for _ in 0..(MAX_LATENCY_SAMPLES + 5) {
            record_latency(&stats, Duration::from_millis(1));
        }
        assert_eq!(stats.latencies.lock().unwrap().len(), MAX_LATENCY_SAMPLES);
    }

    #[test]
    fn tracking_fps_divides_frames_by_elapsed_seconds() {
        let stats = PipelineStats::new();
        stats.frames_tracked.store(500, Ordering::SeqCst);
        assert_eq!(tracking_fps(&stats, Duration::from_secs(2)), 250.0);
        assert_eq!(tracking_fps(&stats, Duration::ZERO), 0.0);
    }

    /// A failing tracker is counted and its message kept, and the published output is "no face".
    struct FailingTracker;

    impl TrackStep for FailingTracker {
        fn step(&mut self, _frame: &Frame) -> Result<TrackingOutput, String> {
            Err("model run failed".to_string())
        }
    }

    #[test]
    fn tracker_errors_are_counted_and_published_as_no_face() {
        let slot = Arc::new(FrameSlot::new());
        let out: TrackingShare = Arc::new(Mutex::new(None));
        let stats = Arc::new(PipelineStats::new());
        let stop = Arc::new(AtomicBool::new(false));
        let handle = spawn_inference(
            FailingTracker,
            Arc::clone(&slot),
            Arc::clone(&out),
            Arc::clone(&stats),
            Arc::clone(&stop),
        );
        slot.put(frame(1, Instant::now()));
        let start = Instant::now();
        while stats.errors.load(Ordering::SeqCst) == 0 && start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(5));
        }
        stop.store(true, Ordering::SeqCst);
        handle.join().unwrap();
        assert_eq!(stats.errors.load(Ordering::SeqCst), 1);
        assert_eq!(stats.last_error.lock().unwrap().as_deref(), Some("model run failed"));
        assert!(!out.lock().unwrap().as_ref().unwrap().face_found);
    }
}
