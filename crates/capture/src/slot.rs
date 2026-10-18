use std::sync::{Condvar, Mutex};
use std::time::Duration;

use core::frame::Frame;

#[derive(Default)]
struct SlotState {
    frame: Option<Frame>,
    dropped: u64,
}

/// Single-slot newest-frame buffer shared by a producer and a consumer thread.
#[derive(Default)]
pub struct FrameSlot {
    state: Mutex<SlotState>,
    ready: Condvar,
}

impl FrameSlot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `frame`, overwriting (and counting) any frame still waiting.
    pub fn put(&self, frame: Frame) {
        let mut state = self.state.lock().unwrap();
        if state.frame.replace(frame).is_some() {
            state.dropped += 1;
        }
        self.ready.notify_one();
    }

    /// Returns the newest frame and clears the slot.
    pub fn take(&self) -> Option<Frame> {
        self.state.lock().unwrap().frame.take()
    }

    /// Like `take`, but waits up to `timeout` for a frame to arrive.
    pub fn wait_take(&self, timeout: Duration) -> Option<Frame> {
        let (mut state, _) = self
            .ready
            .wait_timeout_while(self.state.lock().unwrap(), timeout, |s| s.frame.is_none())
            .unwrap();
        state.frame.take()
    }

    /// Number of frames overwritten before they were taken.
    pub fn dropped(&self) -> u64 {
        self.state.lock().unwrap().dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::frame::PixelFormat;
    use std::sync::Arc;
    use std::thread;
    use std::time::Instant;

    /// Frame whose sequence id is carried in `width`.
    fn frame(seq: u32) -> Frame {
        Frame {
            width: seq,
            height: 1,
            format: PixelFormat::Rgb24,
            bytes: Vec::new(),
            arrived: Instant::now(),
        }
    }

    #[test]
    fn slow_consumer_gets_newest_frame() {
        let slot = FrameSlot::new();
        for seq in 1..=5 {
            slot.put(frame(seq));
        }
        assert_eq!(slot.take().map(|f| f.width), Some(5));
        assert!(slot.take().is_none());
        assert_eq!(slot.dropped(), 4);
    }

    #[test]
    fn slot_never_holds_more_than_one_frame() {
        let slot = FrameSlot::new();
        slot.put(frame(1));
        slot.put(frame(2));
        assert_eq!(slot.take().map(|f| f.width), Some(2));
        assert!(slot.take().is_none());
    }

    #[test]
    fn thousand_writes_ten_reads_keep_order() {
        let slot = FrameSlot::new();
        let mut last_taken = 0;
        let mut takes = 0u64;
        for seq in 1..=1000u32 {
            slot.put(frame(seq));
            if seq % 100 == 0 {
                let got = slot.take().expect("a frame is waiting").width;
                assert!(got > last_taken, "order broken: {got} after {last_taken}");
                last_taken = got;
                takes += 1;
            }
        }
        assert_eq!(takes, 10);
        assert_eq!(last_taken, 1000);
        assert!(slot.take().is_none());
        assert_eq!(slot.dropped(), 1000 - takes);
    }

    #[test]
    fn wait_take_times_out_when_empty() {
        let slot = FrameSlot::new();
        let start = Instant::now();
        assert!(slot.wait_take(Duration::from_millis(20)).is_none());
        assert!(start.elapsed() >= Duration::from_millis(20));
    }

    #[test]
    fn wait_take_wakes_on_put_from_another_thread() {
        let slot = Arc::new(FrameSlot::new());
        let producer = Arc::clone(&slot);
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            producer.put(frame(7));
        });
        let got = slot.wait_take(Duration::from_secs(5)).map(|f| f.width);
        handle.join().unwrap();
        assert_eq!(got, Some(7));
    }
}
