use std::time::Instant;

use nalgebra::UnitQuaternion;

use crate::body::UpperBodyOutput;

/// Pixel layout of a captured camera frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb24,
    Bgr24,
    Nv12,
    Yuy2,
    Xrgb,
}

/// One camera frame as delivered by the capture crate.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub bytes: Vec<u8>,
    pub arrived: Instant,
}

/// Result of one tracking pass.
#[derive(Debug, Clone)]
pub struct TrackingOutput {
    pub weights: [f32; 52],
    pub head_rotation: UnitQuaternion<f32>,
    pub face_found: bool,
    pub ready: Instant,
}

/// Result of one upper-body tracking pass. Absent when the body stage is off.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BodyOutput {
    pub body: Option<UpperBodyOutput>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn body_output_defaults_to_no_body() {
        let out = BodyOutput::default();
        assert_eq!(out.body, None);
    }

    #[test]
    fn frame_and_tracking_output_expose_every_field() {
        let arrived = Instant::now();
        let frame = Frame {
            width: 2,
            height: 1,
            format: PixelFormat::Rgb24,
            bytes: vec![1, 2, 3, 4, 5, 6],
            arrived,
        };
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.format, PixelFormat::Rgb24);
        assert_eq!(frame.bytes.len(), 6);
        assert_eq!(frame.arrived, arrived);

        let ready = Instant::now();
        let output = TrackingOutput {
            weights: [0.5; 52],
            head_rotation: nalgebra::UnitQuaternion::identity(),
            face_found: true,
            ready,
        };
        assert_eq!(output.weights.len(), 52);
        assert_eq!(output.weights[51], 0.5);
        assert_eq!(output.head_rotation, nalgebra::UnitQuaternion::identity());
        assert!(output.face_found);
        assert_eq!(output.ready, ready);

        let cloned = output.clone();
        assert!(cloned.face_found);
        let _ = format!("{:?}", frame.clone());
        let _ = format!("{:?}", PixelFormat::Xrgb);
    }

    #[test]
    fn pixel_format_covers_capture_formats() {
        let formats = [
            PixelFormat::Rgb24,
            PixelFormat::Bgr24,
            PixelFormat::Nv12,
            PixelFormat::Yuy2,
            PixelFormat::Xrgb,
        ];
        assert_eq!(formats.len(), 5);
        assert_ne!(PixelFormat::Nv12, PixelFormat::Yuy2);
    }
}
