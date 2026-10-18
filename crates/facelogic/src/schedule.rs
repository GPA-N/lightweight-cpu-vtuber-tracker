/// Decides on which frames the face detector runs.
pub struct DetectionScheduler {
    pub interval: u32,
    pub presence_threshold: f32,
    frame_index: u32,
    has_face: bool,
    low_presence: bool,
    last_detect: Option<u32>,
}

impl Default for DetectionScheduler {
    fn default() -> Self {
        Self {
            interval: 5,
            presence_threshold: 0.5,
            frame_index: 0,
            has_face: false,
            low_presence: false,
            last_detect: None,
        }
    }
}

impl DetectionScheduler {
    /// Call once per frame before tracking. True means run detection.
    pub fn should_detect(&mut self) -> bool {
        let index = self.frame_index;
        self.frame_index += 1;
        let due = match self.last_detect {
            None => true,
            Some(last) => index - last >= self.interval,
        };
        let run = due || !self.has_face || self.low_presence;
        if run {
            self.last_detect = Some(index);
        }
        run
    }

    /// Call once per frame after tracking with the face state and landmark presence.
    pub fn report(&mut self, face_held: bool, presence: Option<f32>) {
        self.has_face = face_held;
        self.low_presence = presence.is_some_and(|p| p < self.presence_threshold);
    }
}

#[cfg(test)]
mod tests {
    use super::DetectionScheduler;

    // One frame: ask for detection, then report tracking result.
    fn frame(s: &mut DetectionScheduler, face: bool, presence: Option<f32>) -> bool {
        let detect = s.should_detect();
        s.report(face, presence);
        detect
    }

    #[test]
    fn schedule_frame_zero_detects() {
        let mut s = DetectionScheduler::default();
        assert!(frame(&mut s, true, Some(0.9)));
    }

    #[test]
    fn schedule_frames_one_to_four_skip_with_good_face() {
        let mut s = DetectionScheduler::default();
        frame(&mut s, true, Some(0.9));
        for _ in 1..=4 {
            assert!(!frame(&mut s, true, Some(0.9)));
        }
    }

    #[test]
    fn schedule_frame_five_detects_again() {
        let mut s = DetectionScheduler::default();
        frame(&mut s, true, Some(0.9));
        for _ in 1..=4 {
            frame(&mut s, true, Some(0.9));
        }
        assert!(frame(&mut s, true, Some(0.9)));
    }

    #[test]
    fn schedule_low_presence_forces_next_detection() {
        let mut s = DetectionScheduler::default();
        frame(&mut s, true, Some(0.9)); // frame 0
        frame(&mut s, true, Some(0.9)); // frame 1
        frame(&mut s, true, Some(0.9)); // frame 2
        assert!(!frame(&mut s, true, Some(0.3))); // frame 3, low presence reported
        assert!(frame(&mut s, true, Some(0.9))); // frame 4 detects
    }

    #[test]
    fn schedule_lost_face_detects_until_face_held() {
        let mut s = DetectionScheduler::default();
        frame(&mut s, true, Some(0.9)); // frame 0
        assert!(!frame(&mut s, false, None)); // frame 1 lost, reported after its detect call
        assert!(frame(&mut s, false, None)); // frame 2 detects while lost
        assert!(frame(&mut s, false, None)); // frame 3 still detects
        assert!(frame(&mut s, true, Some(0.9))); // frame 4 detects, face found
        assert!(!frame(&mut s, true, Some(0.9))); // frame 5 back to skipping
    }
}
