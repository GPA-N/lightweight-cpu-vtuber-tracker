//! RF-DETR person detector, the stage InstantHMR is built to run behind (`instanthmr/detector.py`,
//! `instanthmr/pipeline.py` in github.com/mohamdev/InstantHMR). The nano variant is exported with
//! `rfdetr`'s own `export()`: input `input` (1, 3, 384, 384) RGB, 0..1, ImageNet mean and std, the whole
//! frame squashed to the square; outputs `dets` (1, 300, 4) boxes as normalised centre x, centre y,
//! width, height, and `labels` (1, 300, 91) class logits (COCO ids, person is 1).
//!
//! The detector is slow next to the body model, so it runs on its own thread on every
//! [`DETECTOR_STRIDE`]th frame and the body model reuses the last box in between, as the maker's
//! pipeline does with `detector_stride`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crate::engine::{load_model, Engine, ModelRun, NamedOutput};
use crate::person_box::PersonBox;

/// RF-DETR nano's input side.
pub const INPUT: usize = 384;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];
/// COCO class id of a person in RF-DETR's 1-indexed labels (`detector.py` COCO_PERSON_CLASS_ID).
const PERSON_CLASS: usize = 1;
/// Confidence threshold (`pipeline.py` det_confidence).
pub const MIN_CONFIDENCE: f32 = 0.5;
/// A reused box is grown by this share of its size on each side (`pipeline.py` `_expand_bbox(expand=0.1)`).
pub const REUSE_EXPAND: f32 = 0.1;
/// The detector runs on every this-many-th frame the body model sees. The maker's phone app runs its
/// detector with `K 30`; its README advises stride 2 to 3 on CPU.
pub const DETECTOR_STRIDE: u64 = 30;

/// Squashes the whole frame to `INPUT` x `INPUT` (bilinear), normalised, planar (CHW).
pub fn preprocess(rgb: &[u8], w: usize, h: usize) -> Vec<f32> {
    let plane = INPUT * INPUT;
    let mut out = vec![0.0f32; 3 * plane];
    let axis = |len: usize| -> Vec<(usize, usize, f32)> {
        let scale = len as f32 / INPUT as f32;
        (0..INPUT)
            .map(|o| {
                let s = ((o as f32 + 0.5) * scale - 0.5).clamp(0.0, (len - 1) as f32);
                let i0 = s.floor() as usize;
                (i0, (i0 + 1).min(len - 1), s - i0 as f32)
            })
            .collect()
    };
    let (rows, cols) = (axis(h), axis(w));
    for (oy, &(y0, y1, fy)) in rows.iter().enumerate() {
        for (ox, &(x0, x1, fx)) in cols.iter().enumerate() {
            for c in 0..3 {
                let at = |x: usize, y: usize| rgb[(y * w + x) * 3 + c] as f32 / 255.0;
                let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
                let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
                let v = top * (1.0 - fy) + bottom * fy;
                out[c * plane + oy * INPUT + ox] = (v - MEAN[c]) / STD[c];
            }
        }
    }
    out
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// The most confident person at or above `MIN_CONFIDENCE`, in frame pixels. `None` when nobody is found.
pub fn decode(outs: &[NamedOutput], frame_w: f32, frame_h: f32) -> Result<Option<PersonBox>, String> {
    let dets = outs.iter().find(|o| o.shape.last() == Some(&4)).ok_or("no boxes output")?;
    let labels = outs.iter().find(|o| !std::ptr::eq(*o, dets) && o.shape.len() == 3).ok_or("no labels output")?;
    let classes = *labels.shape.last().unwrap_or(&0);
    if classes <= PERSON_CLASS {
        return Err("labels output has no person class".into());
    }
    let mut best: Option<(f32, usize)> = None;
    for q in 0..labels.data.len() / classes {
        let score = sigmoid(labels.data[q * classes + PERSON_CLASS]);
        if score >= MIN_CONFIDENCE && best.map_or(true, |(s, _)| score > s) {
            best = Some((score, q));
        }
    }
    Ok(best.and_then(|(_, q)| {
        let d = &dets.data[q * 4..q * 4 + 4];
        let (x0, y0) = (((d[0] - d[2] / 2.0) * frame_w).max(0.0), ((d[1] - d[3] / 2.0) * frame_h).max(0.0));
        let (x1, y1) = (((d[0] + d[2] / 2.0) * frame_w).min(frame_w), ((d[1] + d[3] / 2.0) * frame_h).min(frame_h));
        (x1 > x0 && y1 > y0).then_some(PersonBox { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    }))
}

/// `b` grown by `REUSE_EXPAND` of its size on each side and clamped to the frame.
pub fn expand(b: PersonBox, frame_w: f32, frame_h: f32) -> PersonBox {
    let (x0, y0) = ((b.x - b.w * REUSE_EXPAND).max(0.0), (b.y - b.h * REUSE_EXPAND).max(0.0));
    let (x1, y1) = ((b.x + b.w * (1.0 + REUSE_EXPAND)).min(frame_w), (b.y + b.h * (1.0 + REUSE_EXPAND)).min(frame_h));
    PersonBox { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

/// Runs the model on one frame.
pub struct PersonDetector {
    model: Box<dyn ModelRun + Send>,
}

impl PersonDetector {
    pub fn load(engine: Engine, path: &Path, threads: usize) -> Result<Self, String> {
        Ok(Self { model: load_model(engine, path, threads).map_err(|e| e.to_string())? })
    }

    pub fn detect(&mut self, rgb: &[u8], w: usize, h: usize) -> Result<Option<PersonBox>, String> {
        if w == 0 || h == 0 || rgb.len() != w * h * 3 {
            return Err("image buffer does not match its size".into());
        }
        let outs = self.model.run(&preprocess(rgb, w, h), &[1, 3, INPUT, INPUT]).map_err(|e| e.to_string())?;
        decode(&outs, w as f32, h as f32)
    }
}

struct Job {
    rgb: Vec<u8>,
    w: usize,
    h: usize,
}

#[derive(Default)]
struct Shared {
    job: Mutex<Option<Job>>,
    wake: Condvar,
    /// Last box found and a counter that grows with every new one.
    found: Mutex<Option<(PersonBox, u64)>>,
    stop: AtomicBool,
}

/// A [`PersonDetector`] on its own thread. `submit` never waits: a frame offered while the detector
/// is busy is dropped.
pub struct DetectorThread {
    shared: Arc<Shared>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl DetectorThread {
    pub fn start(mut detector: PersonDetector) -> Self {
        let shared = Arc::new(Shared::default());
        let s = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("person-detector".into())
            .spawn(move || loop {
                let job = {
                    let mut slot = s.job.lock().unwrap_or_else(|e| e.into_inner());
                    while slot.is_none() && !s.stop.load(Ordering::Relaxed) {
                        slot = s.wake.wait(slot).unwrap_or_else(|e| e.into_inner());
                    }
                    if s.stop.load(Ordering::Relaxed) {
                        return;
                    }
                    slot.take()
                };
                let Some(job) = job else { continue };
                if let Ok(Some(b)) = detector.detect(&job.rgb, job.w, job.h) {
                    let mut found = s.found.lock().unwrap_or_else(|e| e.into_inner());
                    let n = found.map_or(0, |(_, n)| n) + 1;
                    *found = Some((b, n));
                }
            })
            .ok();
        Self { shared, worker }
    }

    /// Offers a frame. Returns false when the detector is still busy with the previous one.
    pub fn submit(&self, rgb: &[u8], w: usize, h: usize) -> bool {
        let Ok(mut slot) = self.shared.job.try_lock() else { return false };
        if slot.is_some() {
            return false;
        }
        *slot = Some(Job { rgb: rgb.to_vec(), w, h });
        self.shared.wake.notify_one();
        true
    }

    /// The last box found and its number; the number changes when a newer detection arrives.
    pub fn latest(&self) -> Option<(PersonBox, u64)> {
        *self.shared.found.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for DetectorThread {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(name: &str, shape: &[usize], data: Vec<f32>) -> NamedOutput {
        NamedOutput { name: name.into(), shape: shape.to_vec(), data }
    }

    #[test]
    fn the_most_confident_person_comes_back_in_frame_pixels() {
        let mut labels = vec![-10.0f32; 2 * 91];
        labels[PERSON_CLASS] = 0.0; // query 0: sigmoid(0) = 0.5, at the threshold
        labels[91 + PERSON_CLASS] = 4.0; // query 1: 0.98
        let boxes = vec![0.5, 0.5, 0.2, 0.2, 0.5, 0.5, 0.5, 1.0];
        let b = decode(&[out("dets", &[1, 2, 4], boxes), out("labels", &[1, 2, 91], labels)], 1000.0, 500.0).unwrap().unwrap();
        assert_eq!(b, PersonBox { x: 250.0, y: 0.0, w: 500.0, h: 500.0 });
    }

    #[test]
    fn nobody_above_the_threshold_gives_none() {
        let labels = vec![-3.0f32; 91];
        let r = decode(&[out("dets", &[1, 1, 4], vec![0.5; 4]), out("labels", &[1, 1, 91], labels)], 100.0, 100.0).unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn preprocess_gives_the_normalised_constant_for_a_constant_frame() {
        let t = preprocess(&vec![255u8; 8 * 6 * 3], 8, 6);
        assert_eq!(t.len(), 3 * INPUT * INPUT);
        assert!((t[0] - (1.0 - MEAN[0]) / STD[0]).abs() < 1e-5);
        assert!((t[2 * INPUT * INPUT] - (1.0 - MEAN[2]) / STD[2]).abs() < 1e-5);
    }

    #[test]
    fn a_reused_box_grows_a_tenth_each_side_and_stays_in_the_frame() {
        let e = expand(PersonBox { x: 100.0, y: 0.0, w: 100.0, h: 100.0 }, 1000.0, 1000.0);
        assert_eq!(e, PersonBox { x: 90.0, y: 0.0, w: 120.0, h: 110.0 });
    }
}
