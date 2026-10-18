//! Types shared between the live app and the avatar window: the camera preview shown beside
//! the avatar, the heads-up text, and the render-side timing samples.

use std::sync::{Arc, Mutex};

use crate::body::BodyPose;

/// Upper bound on stored render samples, so a long run cannot grow memory without limit.
pub const MAX_RENDER_SAMPLES: usize = 100_000;

/// One camera frame as RGBA, already mirrored and drawn over with the tracked landmarks.
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Newest preview. The inference thread writes it; the render thread takes it.
pub type PreviewShare = Arc<Mutex<Option<Preview>>>;

/// Newest upper-body pose. The body thread writes it (None when no person is in view); the render thread reads it.
pub type BodyShare = Arc<Mutex<Option<BodyPose>>>;

/// Timing measured inside the render thread.
#[derive(Default)]
pub struct RenderSamples {
    /// Total render frames drawn.
    pub frames: u64,
    /// Frame interval in milliseconds, one entry per frame.
    pub frame_ms: Vec<f32>,
    /// Time from "tracker output ready" to "applied in the render thread", in milliseconds.
    pub handoff_ms: Vec<f32>,
    /// Time spent copying the preview into the GPU texture, in milliseconds.
    pub upload_ms: Vec<f32>,
}

impl RenderSamples {
    pub fn push_frame(&mut self, ms: f32) {
        self.frames += 1;
        if self.frame_ms.len() < MAX_RENDER_SAMPLES {
            self.frame_ms.push(ms);
        }
    }

    pub fn push_handoff(&mut self, ms: f32) {
        if self.handoff_ms.len() < MAX_RENDER_SAMPLES {
            self.handoff_ms.push(ms);
        }
    }

    pub fn push_upload(&mut self, ms: f32) {
        if self.upload_ms.len() < MAX_RENDER_SAMPLES {
            self.upload_ms.push(ms);
        }
    }
}

/// Settings for the live window: the preview panel, a window-title HUD, a time limit, and samples.
pub struct LiveConfig {
    pub preview: PreviewShare,
    /// Text the window title shows; the monitor thread rewrites it about once a second.
    pub hud: Arc<Mutex<String>>,
    /// The window closes by itself after this many seconds.
    pub seconds: f32,
    pub samples: Arc<Mutex<RenderSamples>>,
    /// Live upper-body pose. `None` leaves the body to the default rest pose.
    pub body: Option<BodyShare>,
    /// Focus mode: set means the preview panel, its upload and the debug drawing are off. The
    /// window toggles it with the F key; the tracker threads read it to skip the preview work.
    pub focus: Arc<std::sync::atomic::AtomicBool>,
    /// Confidence thresholds and preview switches the tracker threads read while running; the
    /// settings panel writes them.
    pub tuning: Arc<crate::settings::LiveTuning>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_capped_but_frames_keep_counting() {
        let mut s = RenderSamples::default();
        for _ in 0..(MAX_RENDER_SAMPLES + 10) {
            s.push_frame(16.0);
        }
        assert_eq!(s.frame_ms.len(), MAX_RENDER_SAMPLES);
        assert_eq!(s.frames, (MAX_RENDER_SAMPLES + 10) as u64);
    }
}
