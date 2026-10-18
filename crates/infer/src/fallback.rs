//! NOT USABLE FOR THE SPIKE: see docs/notes/model-fallback.md; private evaluation only.
//!
//! Wrapper for `karie666666/face-au-arkit-blendshapes` (input `image` [batch,3,224,224] NCHW,
//! output `blendshape` [batch,51]). Unknowns are kept explicit:
//! - ASSUMPTION: ImageNet mean/std and RGB channel order, taken from the README PyTorch path.
//!   NOT FOUND: whether the ONNX graph expects this normalization, and the channel order.
//! - ASSUMPTION: output index i is `pose::pad51::FALLBACK_NAMES[i]`. NOT FOUND: any code proving it.
//! - NOT FOUND: a license for the derived weights (see the note, section 8).

use std::fmt;

use ort::session::Session;
use ort::value::Tensor;

/// Model input side length (`input_size: 224`, README and config.json).
pub const INPUT_SIZE: usize = 224;
/// Number of raw outputs of the `blendshape` tensor.
pub const RAW_LEN: usize = 51;
/// ASSUMPTION: ImageNet channel means (RGB), from the README PyTorch path only.
pub const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
/// ASSUMPTION: ImageNet channel stds (RGB), from the README PyTorch path only.
pub const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Errors from the fallback wrapper.
#[derive(Debug, Clone, PartialEq)]
pub enum FallbackError {
	/// `rgb.len()` is not `width * height * 3`.
	WrongInputLength,
	/// The output tensor does not hold `expected` values.
	WrongOutputShape { expected: usize, got: usize },
	/// ONNX Runtime error, as text.
	Ort(String),
}

impl fmt::Display for FallbackError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::WrongInputLength => write!(f, "rgb length must be width * height * 3"),
			Self::WrongOutputShape { expected, got } => {
				write!(f, "fallback output has {got} values, expected {expected}")
			}
			Self::Ort(e) => write!(f, "onnx runtime error: {e}"),
		}
	}
}

impl std::error::Error for FallbackError {}

/// Check that `rgb` holds exactly `width * height` RGB pixels and both sides are non-zero.
pub fn check_rgb_len(rgb: &[u8], width: u32, height: u32) -> Result<(), FallbackError> {
	let want = width as usize * height as usize * 3;
	if width == 0 || height == 0 || rgb.len() != want {
		return Err(FallbackError::WrongInputLength);
	}
	Ok(())
}

/// ASSUMPTION: resize RGB8 `rgb` (`width` x `height`) to 224x224 with plain bilinear sampling,
/// scale to 0..1, normalize with [`MEAN`]/[`STD`], and lay out as NCHW (`[3, 224, 224]` flattened).
/// Caller must pass a valid buffer (see [`check_rgb_len`]).
pub fn preprocess(rgb: &[u8], width: u32, height: u32) -> Vec<f32> {
	let (w, h) = (width as usize, height as usize);
	let plane = INPUT_SIZE * INPUT_SIZE;
	let mut out = vec![0.0f32; 3 * plane];
	let scale_x = w as f32 / INPUT_SIZE as f32;
	let scale_y = h as f32 / INPUT_SIZE as f32;
	for y in 0..INPUT_SIZE {
		let sy = ((y as f32 + 0.5) * scale_y - 0.5).clamp(0.0, (h - 1) as f32);
		let (y0, fy) = (sy.floor() as usize, sy.fract());
		let y1 = (y0 + 1).min(h - 1);
		for x in 0..INPUT_SIZE {
			let sx = ((x as f32 + 0.5) * scale_x - 0.5).clamp(0.0, (w - 1) as f32);
			let (x0, fx) = (sx.floor() as usize, sx.fract());
			let x1 = (x0 + 1).min(w - 1);
			for c in 0..3 {
				let px = |xx: usize, yy: usize| rgb[(yy * w + xx) * 3 + c] as f32 / 255.0;
				let top = px(x0, y0) * (1.0 - fx) + px(x1, y0) * fx;
				let bottom = px(x0, y1) * (1.0 - fx) + px(x1, y1) * fx;
				let v = top * (1.0 - fy) + bottom * fy;
				out[c * plane + y * INPUT_SIZE + x] = (v - MEAN[c]) / STD[c];
			}
		}
	}
	out
}

/// Raw 51 scores to 52 ARKit weights: length check, NaN to 0, clamp to 0..=1, then
/// `pose::pad51::from_fallback`. `tongueOut` is always 0.0.
pub fn decode_output(raw: &[f32]) -> Result<[f32; 52], FallbackError> {
	if raw.len() != RAW_LEN {
		return Err(FallbackError::WrongOutputShape { expected: RAW_LEN, got: raw.len() });
	}
	let mut scores = [0.0f32; RAW_LEN];
	for (dst, &v) in scores.iter_mut().zip(raw) {
		*dst = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
	}
	Ok(pose::pad51::from_fallback(&scores))
}

/// Fallback model session. Not wired into the default pipeline.
pub struct FallbackModel {
	session: Session,
}

impl FallbackModel {
	/// Wrap an already-built session (input `image`, output `blendshape`).
	pub fn new(session: Session) -> Self {
		Self { session }
	}

	/// Run one frame: preprocess, infer, decode. Returns 52 ARKit weights.
	pub fn run(&mut self, rgb: &[u8], width: u32, height: u32) -> Result<[f32; 52], FallbackError> {
		check_rgb_len(rgb, width, height)?;
		let data = preprocess(rgb, width, height);
		let ort_err = |e: ort::Error| FallbackError::Ort(e.to_string());
		let input = Tensor::from_array((vec![1i64, 3, INPUT_SIZE as i64, INPUT_SIZE as i64], data))
			.map_err(ort_err)?;
		let outputs = self.session.run(ort::inputs!["image" => input]).map_err(ort_err)?;
		let (_, values) = outputs["blendshape"].try_extract_tensor::<f32>().map_err(ort_err)?;
		decode_output(values)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const GREY: u8 = 128;

	fn grey_rgb(width: u32, height: u32) -> Vec<u8> {
		vec![GREY; (width * height * 3) as usize]
	}

	#[test]
	fn preprocess_output_length_is_nchw() {
		let out = preprocess(&grey_rgb(640, 480), 640, 480);
		assert_eq!(out.len(), 3 * 224 * 224);
	}

	#[test]
	fn constant_grey_normalizes_per_channel() {
		let out = preprocess(&grey_rgb(50, 37), 50, 37);
		let plane = INPUT_SIZE * INPUT_SIZE;
		let v = GREY as f32 / 255.0;
		for c in 0..3 {
			let expected = (v - MEAN[c]) / STD[c];
			for &x in &out[c * plane..(c + 1) * plane] {
				assert!((x - expected).abs() < 1e-5, "channel {c}: {x} vs {expected}");
			}
		}
	}

	#[test]
	fn red_pixel_fills_first_plane_in_rgb_order() {
		// ASSUMPTION under test: RGB order, so pure red lands in plane 0.
		let out = preprocess(&[255, 0, 0], 1, 1);
		let plane = INPUT_SIZE * INPUT_SIZE;
		assert!((out[0] - (1.0 - MEAN[0]) / STD[0]).abs() < 1e-5);
		assert!((out[plane] - (0.0 - MEAN[1]) / STD[1]).abs() < 1e-5);
		assert!((out[2 * plane] - (0.0 - MEAN[2]) / STD[2]).abs() < 1e-5);
	}

	#[test]
	fn check_rgb_len_rejects_bad_buffers() {
		assert_eq!(check_rgb_len(&grey_rgb(4, 3), 4, 3), Ok(()));
		assert_eq!(check_rgb_len(&grey_rgb(4, 3)[..35], 4, 3), Err(FallbackError::WrongInputLength));
		assert_eq!(check_rgb_len(&[], 0, 0), Err(FallbackError::WrongInputLength));
	}

	#[test]
	fn decode_output_maps_distinct_values_by_name() {
		let mut raw = [0.0f32; 51];
		for (i, v) in raw.iter_mut().enumerate() {
			*v = (i as f32 + 1.0) / 100.0;
		}
		let out = decode_output(&raw).expect("51 values");
		assert_eq!(out.len(), 52);
		assert_eq!(out[core::names::index_of("tongueOut").unwrap()], 0.0);
		for (j, name) in pose::pad51::FALLBACK_NAMES.iter().enumerate() {
			let idx = core::names::index_of(name).unwrap();
			assert_eq!(out[idx], raw[j], "{name}");
		}
	}

	#[test]
	fn decode_output_rejects_wrong_length() {
		assert_eq!(
			decode_output(&[0.5; 52]),
			Err(FallbackError::WrongOutputShape { expected: 51, got: 52 })
		);
		assert_eq!(
			decode_output(&[]),
			Err(FallbackError::WrongOutputShape { expected: 51, got: 0 })
		);
	}

	#[test]
	fn decode_output_handles_nan_and_out_of_range() {
		let mut raw = [0.5f32; 51];
		raw[0] = f32::NAN;
		raw[1] = -3.0;
		raw[2] = 7.0;
		let out = decode_output(&raw).expect("51 values");
		assert_eq!(out[core::names::index_of(pose::pad51::FALLBACK_NAMES[0]).unwrap()], 0.0);
		assert_eq!(out[core::names::index_of(pose::pad51::FALLBACK_NAMES[1]).unwrap()], 0.0);
		assert_eq!(out[core::names::index_of(pose::pad51::FALLBACK_NAMES[2]).unwrap()], 1.0);
		assert!(out.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
	}
}
