use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use core::frame::{Frame, PixelFormat};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;

use crate::auto_brightness::AutoBrightness;
use crate::convert;
use crate::devices::{classify_open_error, CameraError};
use crate::slot::FrameSlot;

/// One undecoded frame as a source hands it over, before RGB conversion.
#[derive(Debug, Clone)]
pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub bytes: Vec<u8>,
}

/// Blocking source of camera frames. Real cameras and test fakes both implement it.
pub trait FrameSource {
    fn next_frame(&mut self) -> Result<RawFrame, CameraError>;
}

/// Handle to a running capture thread.
pub struct CaptureThread {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<CameraError>>>,
    fps: Arc<AtomicU32>,
}

impl CaptureThread {
    /// Waits for the thread to exit on its own (source end or fatal error).
    pub fn join(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    /// Asks the thread to stop after its current frame, then waits for it to exit.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.join();
    }

    /// The fatal error that ended the thread, if any.
    pub fn error(&self) -> Option<CameraError> {
        self.error.lock().unwrap().clone()
    }

    /// Frames per second delivered to the slot, averaged since the thread started.
    pub fn delivered_fps(&self) -> u32 {
        self.fps.load(Ordering::SeqCst)
    }
}

/// Starts a thread that runs `open` to get a source (cameras are not `Send`, so
/// the source is created on the thread), then pulls frames from it, converts them
/// to RGB, and writes each into `slot` with its arrival time. Stops on an open,
/// source, or conversion error (stored in `error`), or when `stop` is called.
pub fn spawn_capture<F>(open: F, slot: Arc<FrameSlot>) -> CaptureThread
where
    F: FnOnce() -> Result<Box<dyn FrameSource>, CameraError> + Send + 'static,
{
    spawn_capture_with(open, slot, Arc::new(AtomicBool::new(false)))
}

/// Like [spawn_capture], with automatic brightness (crate::auto_brightness) applied to every frame while
/// auto_brightness is true. The flag can be flipped while the thread runs.
pub fn spawn_capture_with<F>(open: F, slot: Arc<FrameSlot>, auto_brightness: Arc<AtomicBool>) -> CaptureThread
where
    F: FnOnce() -> Result<Box<dyn FrameSource>, CameraError> + Send + 'static,
{
    let stop = Arc::new(AtomicBool::new(false));
    let error = Arc::new(Mutex::new(None));
    let fps = Arc::new(AtomicU32::new(0));

    let join = {
        let stop = Arc::clone(&stop);
        let error = Arc::clone(&error);
        let fps = Arc::clone(&fps);
        thread::spawn(move || {
            let mut source = match open() {
                Ok(source) => source,
                Err(e) => {
                    *error.lock().unwrap() = Some(e);
                    return;
                }
            };
            let started = Instant::now();
            let mut delivered: u64 = 0;
            let mut brightness = AutoBrightness::default();
            while !stop.load(Ordering::SeqCst) {
                let fatal = match source.next_frame() {
                    Ok(raw) => match convert::to_rgb_owned(raw.format, raw.width, raw.height, raw.bytes) {
                        Ok(mut bytes) => {
                            if auto_brightness.load(Ordering::Relaxed) {
                                brightness.process(&mut bytes, raw.width, raw.height);
                            }
                            slot.put(Frame {
                                width: raw.width,
                                height: raw.height,
                                format: PixelFormat::Rgb24,
                                bytes,
                                arrived: Instant::now(),
                            });
                            delivered += 1;
                            let secs = started.elapsed().as_secs_f64();
                            fps.store((delivered as f64 / secs) as u32, Ordering::SeqCst);
                            None
                        }
                        Err(e) => Some(CameraError::Other(e.to_string())),
                    },
                    Err(e) => Some(e),
                };
                if let Some(e) = fatal {
                    *error.lock().unwrap() = Some(e);
                    break;
                }
            }
        })
    };

    CaptureThread {
        stop,
        join: Some(join),
        error,
        fps,
    }
}

/// Frame format of a camera mode, as far as the mode picker cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeFormat {
    Mjpeg,
    Yuyv,
    Nv12,
    Other,
}

/// One camera mode the picker can choose from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeInfo {
    pub format: ModeFormat,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// Preference rank of a format when two modes tie: MJPEG first, then YUYV, then NV12.
/// Other is never picked, so it only needs a rank to keep the match total.
fn format_rank(format: ModeFormat) -> i64 {
    match format {
        ModeFormat::Mjpeg => 0,
        ModeFormat::Yuyv => 1,
        ModeFormat::Nv12 => 2,
        ModeFormat::Other => 3,
    }
}

/// Picks the mode to open. Only modes no larger than `want_w` x `want_h` and not `Other`
/// count. If some reach `want_fps`, the nearest resolution wins, then MJPEG, YUYV, NV12,
/// then the higher fps. If none reach it, the highest fps wins, then the nearest
/// resolution, then format. `None` when no mode fits.
pub fn pick_mode(modes: &[ModeInfo], want_w: u32, want_h: u32, want_fps: u32) -> Option<ModeInfo> {
    modes
        .iter()
        .filter(|m| m.format != ModeFormat::Other && m.width <= want_w && m.height <= want_h)
        .copied()
        .min_by_key(|m| {
            let gap = i64::from(want_w - m.width) + i64::from(want_h - m.height);
            let fps = i64::from(m.fps);
            if m.fps >= want_fps {
                (0, gap, format_rank(m.format), -fps)
            } else {
                (1, -fps, gap, format_rank(m.format))
            }
        })
}

/// Lists the camera's modes for MJPEG, YUYV and NV12. Empty when the camera cannot be
/// queried. Each format is listed on its own: the combined format list is capped at two.
fn list_modes(index: u32) -> Vec<ModeInfo> {
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
    let Ok(mut camera) = Camera::new(CameraIndex::Index(index), requested) else {
        return Vec::new();
    };
    let mut modes = Vec::new();
    for (fcc, format) in [
        (FrameFormat::MJPEG, ModeFormat::Mjpeg),
        (FrameFormat::YUYV, ModeFormat::Yuyv),
        (FrameFormat::NV12, ModeFormat::Nv12),
    ] {
        let Ok(by_resolution) = camera.compatible_list_by_resolution(fcc) else {
            continue;
        };
        for (res, fps_list) in by_resolution {
            for fps in fps_list {
                modes.push(ModeInfo { format, width: res.width(), height: res.height(), fps });
            }
        }
    }
    modes
}

/// A real webcam opened through Media Foundation.
pub struct NokhwaSource {
    camera: Camera,
    /// Fps of the mode picked by `pick_mode`; `None` when the closest-mode loop opened the camera.
    picked_fps: Option<u32>,
    first_frame: Option<Instant>,
    frames: u32,
    checked: bool,
}

impl NokhwaSource {
    fn new(camera: Camera, picked_fps: Option<u32>) -> Self {
        Self { camera, picked_fps, first_frame: None, frames: 0, checked: false }
    }

    /// Opens camera `index` with the mode `pick_mode` chooses for `width` x `height` at `fps`
    /// (listed first, requested exactly). Falls back to the closest-mode loop when listing
    /// finds nothing or the exact request fails.
    pub fn open(index: u32, width: u32, height: u32, fps: u32) -> Result<Self, CameraError> {
        if let Some(mode) = pick_mode(&list_modes(index), width, height, fps) {
            if let Some(source) = Self::open_exact(index, mode) {
                return Ok(source);
            }
        }
        Self::open_closest(index, width, height, fps)
    }

    fn open_exact(index: u32, mode: ModeInfo) -> Option<Self> {
        let fcc = match mode.format {
            ModeFormat::Mjpeg => FrameFormat::MJPEG,
            ModeFormat::Yuyv => FrameFormat::YUYV,
            ModeFormat::Nv12 => FrameFormat::NV12,
            ModeFormat::Other => return None,
        };
        let wanted = CameraFormat::new(Resolution::new(mode.width, mode.height), fcc, mode.fps);
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(wanted));
        let mut camera = Camera::new(CameraIndex::Index(index), requested).ok()?;
        camera.open_stream().ok()?;
        Some(Self::new(camera, Some(mode.fps)))
    }

    /// Opens camera `index`, asking for the closest mode to `width` x `height` at `fps`.
    /// MJPEG is tried first, then the raw formats: some cameras (found on a real laptop)
    /// reject an MJPEG request with "Failed to fulfill requested format".
    fn open_closest(index: u32, width: u32, height: u32, fps: u32) -> Result<Self, CameraError> {
        let mut last = None;
        for fcc in [FrameFormat::MJPEG, FrameFormat::YUYV, FrameFormat::NV12] {
            let wanted = CameraFormat::new(Resolution::new(width, height), fcc, fps);
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(wanted));
            let attempt = Camera::new(CameraIndex::Index(index), requested).and_then(|mut camera| {
                camera.open_stream()?;
                Ok(camera)
            });
            match attempt {
                Ok(camera) => return Ok(Self::new(camera, None)),
                Err(e) => last = Some(e.to_string()),
            }
        }
        Err(classify_open_error(&last.unwrap_or_default(), index, ""))
    }

    /// Counts frames and, once 2 s of frames have arrived, warns if the delivered fps is
    /// below 0.7 of the picked fps. Checked once; no retry.
    fn note_frame(&mut self) {
        if self.checked {
            return;
        }
        let first = *self.first_frame.get_or_insert_with(Instant::now);
        self.frames += 1;
        let secs = first.elapsed().as_secs_f64();
        if secs < 2.0 {
            return;
        }
        self.checked = true;
        if let Some(picked) = self.picked_fps {
            let delivered = f64::from(self.frames) / secs;
            if delivered < 0.7 * f64::from(picked) {
                eprintln!(
                    "warning: camera delivers {delivered:.1} fps, below 0.7 of the picked {picked} fps"
                );
            }
        }
    }
}

impl FrameSource for NokhwaSource {
    fn next_frame(&mut self) -> Result<RawFrame, CameraError> {
        let buffer = self
            .camera
            .frame()
            .map_err(|e| CameraError::Other(e.to_string()))?;
        self.note_frame();
        let res = buffer.resolution();
        let (width, height) = (res.width(), res.height());
        let (format, bytes) = match buffer.source_frame_format() {
            FrameFormat::RAWRGB => (PixelFormat::Rgb24, buffer.buffer().to_vec()),
            FrameFormat::RAWBGR => (PixelFormat::Bgr24, buffer.buffer().to_vec()),
            FrameFormat::NV12 => (PixelFormat::Nv12, buffer.buffer().to_vec()),
            FrameFormat::YUYV => (PixelFormat::Yuy2, buffer.buffer().to_vec()),
            FrameFormat::MJPEG => {
                let image = buffer
                    .decode_image::<RgbFormat>()
                    .map_err(|e| CameraError::Other(e.to_string()))?;
                (PixelFormat::Rgb24, image.into_raw())
            }
            other => {
                return Err(CameraError::Other(format!(
                    "unsupported camera frame format: {other:?}"
                )))
            }
        };
        Ok(RawFrame {
            width,
            height,
            format,
            bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    /// Fake source: yields `count` frames as fast as possible, then fails with `end`.
    struct BurstSource {
        seq: u32,
        count: u32,
    }

    impl FrameSource for BurstSource {
        fn next_frame(&mut self) -> Result<RawFrame, CameraError> {
            if self.seq >= self.count {
                return Err(CameraError::Other("end".to_string()));
            }
            self.seq += 1;
            Ok(raw(self.seq))
        }
    }

    /// Fake source: fails on frame `fail_at` (1-based) with `error`.
    struct FailingSource {
        seq: u32,
        fail_at: u32,
    }

    impl FrameSource for FailingSource {
        fn next_frame(&mut self) -> Result<RawFrame, CameraError> {
            self.seq += 1;
            if self.seq == self.fail_at {
                return Err(CameraError::Other("device unplugged".to_string()));
            }
            Ok(raw(self.seq))
        }
    }

    /// Fake source: endless 1x1 frames on a fixed schedule, `interval` apart.
    struct PacedSource {
        seq: u32,
        interval: Duration,
        next_at: Option<Instant>,
    }

    impl FrameSource for PacedSource {
        fn next_frame(&mut self) -> Result<RawFrame, CameraError> {
            let due = *self.next_at.get_or_insert_with(Instant::now);
            let now = Instant::now();
            if due > now {
                thread::sleep(due - now);
            }
            self.next_at = Some(due + self.interval);
            self.seq += 1;
            Ok(raw(self.seq))
        }
    }

    /// 1x1 RGB frame whose sequence id is carried in `width`.
    fn raw(seq: u32) -> RawFrame {
        RawFrame {
            width: seq,
            height: 1,
            format: PixelFormat::Rgb24,
            bytes: vec![0; (seq * 3) as usize],
        }
    }

    #[test]
    fn three_hundred_frames_keep_only_the_newest() {
        let slot = Arc::new(FrameSlot::new());
        let mut handle = spawn_capture(
            move || Ok(Box::new(BurstSource { seq: 0, count: 300 }) as Box<dyn FrameSource>),
            Arc::clone(&slot),
        );
        handle.join();
        let newest = slot.take().expect("a frame is waiting");
        assert_eq!(newest.width, 300);
        assert_eq!(slot.dropped(), 299);
    }

    #[test]
    fn source_error_stops_thread_and_is_stored() {
        let slot = Arc::new(FrameSlot::new());
        let mut handle = spawn_capture(
            move || Ok(Box::new(FailingSource { seq: 0, fail_at: 5 }) as Box<dyn FrameSource>),
            Arc::clone(&slot),
        );
        handle.join();
        assert_eq!(
            handle.error(),
            Some(CameraError::Other("device unplugged".to_string()))
        );
        assert_eq!(slot.take().map(|f| f.width), Some(4));
    }

    #[test]
    fn stop_flag_ends_an_endless_source() {
        let slot = Arc::new(FrameSlot::new());
        let mut handle = spawn_capture(
            move || {
                Ok(Box::new(PacedSource {
                    seq: 0,
                    interval: Duration::from_millis(1),
                    next_at: None,
                }) as Box<dyn FrameSource>)
            },
            Arc::clone(&slot),
        );
        thread::sleep(Duration::from_millis(50));
        handle.stop();
        assert_eq!(handle.error(), None);
        assert!(slot.take().is_some());
    }

    #[test]
    fn delivered_fps_matches_a_hundred_fps_source_within_thirty_percent() {
        let slot = Arc::new(FrameSlot::new());
        let mut handle = spawn_capture(
            move || {
                Ok(Box::new(PacedSource {
                    seq: 0,
                    interval: Duration::from_millis(10),
                    next_at: None,
                }) as Box<dyn FrameSource>)
            },
            Arc::clone(&slot),
        );
        thread::sleep(Duration::from_secs(2));
        let fps = handle.delivered_fps();
        handle.stop();
        assert!(
            (70..=130).contains(&fps),
            "delivered fps {fps} outside 100 +-30%"
        );
    }

    fn mode(format: ModeFormat, width: u32, height: u32, fps: u32) -> ModeInfo {
        ModeInfo { format, width, height, fps }
    }

    #[test]
    fn pick_mode_skips_a_mode_that_is_too_slow_for_the_nearer_one() {
        let modes = [
            mode(ModeFormat::Mjpeg, 1280, 720, 10),
            mode(ModeFormat::Mjpeg, 640, 480, 30),
        ];
        assert_eq!(
            pick_mode(&modes, 1280, 720, 30),
            Some(mode(ModeFormat::Mjpeg, 640, 480, 30))
        );
    }

    #[test]
    fn pick_mode_uses_yuyv_when_it_is_the_only_format() {
        let modes = [mode(ModeFormat::Yuyv, 640, 480, 30)];
        assert_eq!(pick_mode(&modes, 1280, 720, 30), Some(mode(ModeFormat::Yuyv, 640, 480, 30)));
    }

    #[test]
    fn pick_mode_prefers_mjpeg_over_yuyv_at_the_same_size() {
        let modes = [
            mode(ModeFormat::Yuyv, 640, 480, 30),
            mode(ModeFormat::Mjpeg, 640, 480, 30),
        ];
        assert_eq!(pick_mode(&modes, 640, 480, 30), Some(mode(ModeFormat::Mjpeg, 640, 480, 30)));
    }

    #[test]
    fn pick_mode_is_none_for_an_empty_list() {
        assert_eq!(pick_mode(&[], 1280, 720, 30), None);
    }

    #[test]
    fn pick_mode_is_none_when_every_mode_is_larger_than_requested() {
        let modes = [
            mode(ModeFormat::Mjpeg, 1280, 720, 30),
            mode(ModeFormat::Yuyv, 1920, 1080, 30),
        ];
        assert_eq!(pick_mode(&modes, 640, 480, 30), None);
    }

    #[test]
    fn pick_mode_ignores_other_formats() {
        let modes = [mode(ModeFormat::Other, 640, 480, 30)];
        assert_eq!(pick_mode(&modes, 1280, 720, 30), None);
    }

    #[test]
    fn pick_mode_falls_back_to_highest_fps_when_none_reach_the_wanted_fps() {
        let modes = [
            mode(ModeFormat::Mjpeg, 640, 480, 15),
            mode(ModeFormat::Mjpeg, 320, 240, 24),
        ];
        assert_eq!(pick_mode(&modes, 640, 480, 30), Some(mode(ModeFormat::Mjpeg, 320, 240, 24)));
    }
}
