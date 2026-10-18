//! 146-point blendshape wrapper around `serving_default_input_points:0` / `StatefulPartitionedCall:0`.
//!
//! ASSUMPTION (validated by the first real run, task 4.7): the caller passes full-frame pixel
//! coordinates, x in `0..frame_width` and y in `0..frame_height`, z ignored. Crop-relative or
//! normalized coordinates would silently produce wrong scores, so do not pass them.

use std::fmt;

use ort::session::Session;
use ort::value::Tensor;

const INPUT_NAME: &str = "serving_default_input_points:0";
const OUTPUT_NAME: &str = "StatefulPartitionedCall:0";
const LANDMARK_COUNT: usize = 478;
const POINT_COUNT: usize = 146;
const OUTPUT_COUNT: usize = 52;

/// Subset indices (MediaPipe order), compiled in from `data/landmark_subset_146.json`.
/// Parsed by hand: `infer` has no serde_json dependency.
pub const SUBSET: [usize; POINT_COUNT] = parse_subset(include_str!("../../../data/landmark_subset_146.json"));

const fn parse_subset(text: &str) -> [usize; POINT_COUNT] {
	let bytes = text.as_bytes();
	let mut out = [0usize; POINT_COUNT];
	let mut n = 0;
	let mut cur = 0usize;
	let mut in_num = false;
	let mut i = 0;
	while i < bytes.len() {
		let b = bytes[i];
		if b >= b'0' && b <= b'9' {
			cur = cur * 10 + (b - b'0') as usize;
			in_num = true;
		} else if in_num {
			out[n] = cur;
			n += 1;
			cur = 0;
			in_num = false;
		}
		i += 1;
	}
	if in_num {
		out[n] = cur;
		n += 1;
	}
	assert!(n == POINT_COUNT, "landmark subset must hold 146 indices");
	out
}

/// Errors from the blendshape wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlendshapeError {
	WrongLandmarkCount { expected: usize, got: usize },
	WrongOutputShape { expected: usize, got: usize },
	Ort(String),
}

impl fmt::Display for BlendshapeError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::WrongLandmarkCount { expected, got } => {
				write!(f, "expected {expected} landmarks, got {got}")
			}
			Self::WrongOutputShape { expected, got } => {
				write!(f, "expected {expected} blendshape outputs, got {got}")
			}
			Self::Ort(msg) => write!(f, "ONNX Runtime error: {msg}"),
		}
	}
}

impl std::error::Error for BlendshapeError {}

/// Select the 146 subset points from 478 image-pixel landmarks, dropping z.
/// Returns 292 floats laid out as x0, y0, x1, y1, ...
pub fn pick_subset(landmarks: &[[f32; 3]]) -> Result<Vec<f32>, BlendshapeError> {
	if landmarks.len() != LANDMARK_COUNT {
		return Err(BlendshapeError::WrongLandmarkCount { expected: LANDMARK_COUNT, got: landmarks.len() });
	}
	let mut out = Vec::with_capacity(POINT_COUNT * 2);
	for &i in SUBSET.iter() {
		let p = landmarks[i];
		out.push(p[0]);
		out.push(p[1]);
	}
	Ok(out)
}

/// Check the 52 raw scores and clamp them to 0..=1. NaN becomes 0.0.
/// The output is in MEDIAPIPE_OUTPUT_NAMES order (`_neutral` first).
pub fn decode_output(raw: &[f32]) -> Result<[f32; OUTPUT_COUNT], BlendshapeError> {
	if raw.len() != OUTPUT_COUNT {
		return Err(BlendshapeError::WrongOutputShape { expected: OUTPUT_COUNT, got: raw.len() });
	}
	let mut out = [0.0; OUTPUT_COUNT];
	for (o, &v) in out.iter_mut().zip(raw) {
		*o = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
	}
	Ok(out)
}

/// Blendshape ONNX session. CPU only; built by `session::build_session`.
pub struct BlendshapeModel {
	session: Session,
}

impl BlendshapeModel {
	pub fn new(session: Session) -> Self {
		Self { session }
	}

	/// Run the model and return the 52 raw outputs before any clamping (MEDIAPIPE_OUTPUT_NAMES order).
	pub fn run_raw(&mut self, landmarks_image_px: &[[f32; 3]]) -> Result<Vec<f32>, BlendshapeError> {
		let points = pick_subset(landmarks_image_px)?;
		let input = Tensor::from_array(([1usize, POINT_COUNT, 2], points))
			.map_err(|e| BlendshapeError::Ort(e.to_string()))?;
		let outputs = self
			.session
			.run(ort::inputs![INPUT_NAME => input])
			.map_err(|e| BlendshapeError::Ort(e.to_string()))?;
		let (_, data) = outputs[OUTPUT_NAME]
			.try_extract_tensor::<f32>()
			.map_err(|e| BlendshapeError::Ort(e.to_string()))?;
		Ok(data.to_vec())
	}

	/// Run the model on 478 image-pixel landmarks (see module doc for the coordinate assumption).
	/// Returns 52 ARKit-ordered weights via `pose::pad51::from_mediapipe`.
	pub fn run(&mut self, landmarks_image_px: &[[f32; 3]]) -> Result<[f32; OUTPUT_COUNT], BlendshapeError> {
		let raw = decode_output(&self.run_raw(landmarks_image_px)?)?;
		Ok(pose::pad51::from_mediapipe(&raw))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use core::names::{index_of, MEDIAPIPE_OUTPUT_NAMES};
	use std::collections::HashSet;

	fn fake_landmarks(n: usize) -> Vec<[f32; 3]> {
		(0..n).map(|i| [i as f32, 2.0 * i as f32, 99.0]).collect()
	}

	#[test]
	fn blendshapes_subset_has_146_unique_indices_below_478() {
		assert_eq!(SUBSET.len(), 146);
		let unique: HashSet<usize> = SUBSET.iter().copied().collect();
		assert_eq!(unique.len(), 146);
		assert!(SUBSET.iter().all(|&i| i < 478));
	}

	#[test]
	fn blendshapes_pick_subset_returns_xy_of_each_subset_index() {
		let out = pick_subset(&fake_landmarks(478)).unwrap();
		assert_eq!(out.len(), 292);
		for (k, &idx) in SUBSET.iter().enumerate() {
			assert_eq!(out[2 * k], idx as f32, "x of subset {k}");
			assert_eq!(out[2 * k + 1], 2.0 * idx as f32, "y of subset {k}");
		}
		assert!(!out.contains(&99.0), "z must be dropped");
	}

	#[test]
	fn blendshapes_pick_subset_rejects_wrong_count() {
		let err = pick_subset(&fake_landmarks(477)).unwrap_err();
		assert_eq!(err, BlendshapeError::WrongLandmarkCount { expected: 478, got: 477 });
	}

	#[test]
	fn blendshapes_decode_output_rejects_wrong_length() {
		let err = decode_output(&[0.0; 51]).unwrap_err();
		assert_eq!(err, BlendshapeError::WrongOutputShape { expected: 52, got: 51 });
	}

	#[test]
	fn blendshapes_decode_output_clamps_and_maps_nan_to_zero() {
		let mut raw = [0.5f32; 52];
		raw[0] = f32::NAN;
		raw[1] = -0.3;
		raw[2] = 1.7;
		let out = decode_output(&raw).unwrap();
		assert_eq!(out[0], 0.0);
		assert_eq!(out[1], 0.0);
		assert_eq!(out[2], 1.0);
		assert_eq!(out[3], 0.5);
	}

	#[test]
	fn blendshapes_output_maps_jaw_open_to_arkit_index_and_drops_neutral() {
		let jaw = MEDIAPIPE_OUTPUT_NAMES.iter().position(|n| *n == "jawOpen").unwrap();
		assert_eq!(jaw, 25);
		let mut raw = [0.0f32; 52];
		raw[jaw] = 0.7;
		raw[0] = 0.9; // _neutral: must never appear in the ARKit output
		let out = pose::pad51::from_mediapipe(&decode_output(&raw).unwrap());
		assert_eq!(out[index_of("jawOpen").unwrap()], 0.7);
		assert!(!out.contains(&0.9));
		assert!(index_of("_neutral").is_none());
	}
}
