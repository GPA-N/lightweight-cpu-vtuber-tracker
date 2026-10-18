//! Upper-body tracking data for a still image or one camera frame.
//! Plain data only: the models live in `infer`, the retarget in `avatar`.

/// Number of BlazePose body landmarks (0..=32). Hips are 23 and 24 and must not be used for desk framing.
pub const POSE_POINTS: usize = 33;
/// Number of hand landmarks per hand.
pub const HAND_POINTS: usize = 21;

/// One body landmark in source-image pixels. `z` uses the same scale as `x` (MediaPipe convention,
/// smaller is closer to the camera). `visibility` is a probability in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PoseLandmark {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub visibility: f32,
    /// Metric world landmark (metres, hip-centred origin, x right, y down, z away from the camera),
    /// rotated back into the image frame. Its scale is consistent across axes, so limb directions
    /// from it keep correct foreshortening. Not used for hips.
    pub world: [f32; 3],
}

/// Which arm of the photographed person a hand belongs to (person's own left or right, MediaPipe naming).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectSide {
    Left,
    Right,
}

/// 21 hand landmarks in source-image pixels (x, y) plus relative depth (z, same scale as x).
#[derive(Debug, Clone, PartialEq)]
pub struct HandLandmarks {
    pub points: [[f32; 3]; HAND_POINTS],
    /// Hand presence probability from the hand model, 0..=1.
    pub score: f32,
    /// Raw handedness output of the hand model (probability-like, convention unverified).
    pub handedness: f32,
    /// Arm whose pose wrist this hand was cropped around. Assigned geometrically, not from `handedness`.
    pub side: SubjectSide,
}

/// Result of the upper-body stage on one image.
#[derive(Debug, Clone, PartialEq)]
pub struct UpperBodyOutput {
    pub width: u32,
    pub height: u32,
    pub pose: [PoseLandmark; POSE_POINTS],
    /// Pose presence probability.
    pub presence: f32,
    /// Hand model results (score may be below threshold: the consumer decides).
    pub hands: Vec<HandLandmarks>,
    /// Label of the hand model that produced hands, e.g. PROVISIONAL (2023 port).
    pub hand_model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_landmark_is_zero() {
        assert_eq!(PoseLandmark::default().visibility, 0.0);
        assert_eq!(POSE_POINTS, 33);
        assert_eq!(HAND_POINTS, 21);
    }
}


