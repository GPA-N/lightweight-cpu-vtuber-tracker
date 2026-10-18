use facelogic::crop::{crop_to_frame, RotatedRect};
use ort::session::{Session, SessionOutputs};
use ort::value::Tensor;

const INPUT_SIZE: usize = 256;
const NUM_POINTS: usize = 478;
const INPUT_NAME: &str = "input_12";

/// Errors from the landmark wrapper.
#[derive(Debug, Clone, PartialEq)]
pub enum LandmarkError {
    WrongInputLength { expected: usize, got: usize },
    WrongOutputShape { name: &'static str, expected: usize, got: usize },
    Ort(String),
}

/// The three raw scalar outputs, kept for the first-run print.
#[derive(Debug, Clone, PartialEq)]
pub struct RawOutputs {
    /// `Identity_1`: presence logit.
    pub logit: f32,
    /// `Identity_2`: presence probability (the graph's Sigmoid output).
    pub probability: f32,
    /// sigmoid(`Identity_1`).
    pub sigmoid_logit: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LandmarkOutput {
    /// 478 points in crop pixels 0..256; z in the model's raw units.
    pub points: Vec<[f32; 3]>,
    /// sigmoid(`Identity_1`). Measured on real photos: `Identity_2` reads about 0 on clear faces
    /// while the logit is +13 to +23, so `Identity_2` is not the presence probability.
    /// The caller applies the 0.5 threshold.
    pub presence: f32,
    pub raw_outputs: RawOutputs,
}

fn ort_err(e: ort::Error) -> LandmarkError {
    LandmarkError::Ort(e.to_string())
}

/// The warped crop tensor must be 256x256x3 floats (NHWC, RGB, 0..1).
pub fn check_input(tensor: &[f32]) -> Result<(), LandmarkError> {
    let expected = INPUT_SIZE * INPUT_SIZE * 3;
    if tensor.len() != expected {
        return Err(LandmarkError::WrongInputLength { expected, got: tensor.len() });
    }
    Ok(())
}

fn check_len(name: &'static str, data: &[f32], expected: usize) -> Result<(), LandmarkError> {
    if data.len() != expected {
        return Err(LandmarkError::WrongOutputShape { name, expected, got: data.len() });
    }
    Ok(())
}

/// Split the raw outputs into points and presence. Threshold is applied by the caller.
pub fn decode_outputs(
    identity: &[f32],
    id1: &[f32],
    id2: &[f32],
) -> Result<LandmarkOutput, LandmarkError> {
    check_len("Identity", identity, NUM_POINTS * 3)?;
    check_len("Identity_1", id1, 1)?;
    check_len("Identity_2", id2, 1)?;
    let points = identity.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    let logit = id1[0];
    Ok(LandmarkOutput {
        points,
        presence: 1.0 / (1.0 + (-logit).exp()),
        raw_outputs: RawOutputs {
            logit,
            probability: id2[0],
            sigmoid_logit: 1.0 / (1.0 + (-logit).exp()),
        },
    })
}

/// Map crop points to frame pixels. z scales by `rect.width / 256`.
pub fn landmarks_to_frame(points: &[[f32; 3]], rect: &RotatedRect) -> Vec<[f32; 3]> {
    points
        .iter()
        .map(|p| {
            let (x, y) = crop_to_frame(p[0], p[1], rect, INPUT_SIZE);
            [x, y, p[2] / INPUT_SIZE as f32 * rect.width]
        })
        .collect()
}

fn extract(outputs: &SessionOutputs<'_>, name: &str) -> Result<Vec<f32>, LandmarkError> {
    let (_, data) = outputs[name].try_extract_tensor::<f32>().map_err(ort_err)?;
    Ok(data.to_vec())
}

pub struct LandmarkModel {
    session: Session,
}

impl LandmarkModel {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    /// `tensor` is the warped crop, NHWC 1x256x256x3 RGB in 0..1.
    pub fn run(&mut self, tensor: &[f32]) -> Result<LandmarkOutput, LandmarkError> {
        check_input(tensor)?;
        let shape = vec![1i64, INPUT_SIZE as i64, INPUT_SIZE as i64, 3];
        let input = Tensor::<f32>::from_array((shape, tensor.to_vec())).map_err(ort_err)?;
        let outputs = self
            .session
            .run(ort::inputs![INPUT_NAME => input])
            .map_err(ort_err)?;
        let identity = extract(&outputs, "Identity")?;
        let id1 = extract(&outputs, "Identity_1")?;
        let id2 = extract(&outputs, "Identity_2")?;
        decode_outputs(&identity, &id1, &id2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis_rect() -> RotatedRect {
        RotatedRect {
            cx: 320.0,
            cy: 240.0,
            width: 192.0,
            height: 192.0,
            angle: 0.0,
        }
    }

    #[test]
    fn check_input_accepts_right_length() {
        assert!(check_input(&vec![0.0; 256 * 256 * 3]).is_ok());
    }

    #[test]
    fn check_input_rejects_other_lengths() {
        match check_input(&vec![0.0; 256 * 256]) {
            Err(LandmarkError::WrongInputLength { expected, got }) => {
                assert_eq!(expected, 256 * 256 * 3);
                assert_eq!(got, 256 * 256);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(check_input(&[]).is_err());
    }

    #[test]
    fn decode_splits_triples_in_order_and_keeps_raw() {
        let identity: Vec<f32> = (0..1434).map(|i| i as f32).collect();
        let out = decode_outputs(&identity, &[0.25], &[0.75]).unwrap();
        assert_eq!(out.points.len(), 478);
        assert_eq!(out.points[0], [0.0, 1.0, 2.0]);
        assert_eq!(out.points[477], [1431.0, 1432.0, 1433.0]);
        assert_eq!(out.presence, out.raw_outputs.sigmoid_logit);
        assert_eq!(out.raw_outputs.logit, 0.25);
        assert_eq!(out.raw_outputs.probability, 0.75);
        let expected = 1.0 / (1.0 + (-0.25f32).exp());
        assert!((out.raw_outputs.sigmoid_logit - expected).abs() < 1e-6);
    }

    #[test]
    fn decode_rejects_wrong_output_lengths() {
        let ok: Vec<f32> = vec![0.0; 1434];
        match decode_outputs(&ok[..1433], &[0.0], &[0.0]) {
            Err(LandmarkError::WrongOutputShape { name, expected, got }) => {
                assert_eq!(name, "Identity");
                assert_eq!(expected, 1434);
                assert_eq!(got, 1433);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            decode_outputs(&ok, &[], &[0.0]),
            Err(LandmarkError::WrongOutputShape { name: "Identity_1", .. })
        ));
        assert!(matches!(
            decode_outputs(&ok, &[0.0], &[0.0, 0.0]),
            Err(LandmarkError::WrongOutputShape { name: "Identity_2", .. })
        ));
    }

    #[test]
    fn landmarks_to_frame_maps_centre_and_depth() {
        let points = [[128.0, 128.0, 256.0]];
        let out = landmarks_to_frame(&points, &axis_rect());
        assert!((out[0][0] - 320.0).abs() < 1e-3, "x {}", out[0][0]);
        assert!((out[0][1] - 240.0).abs() < 1e-3, "y {}", out[0][1]);
        assert!((out[0][2] - 192.0).abs() < 1e-3, "z {}", out[0][2]);
    }
}
