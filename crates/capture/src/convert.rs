use core::frame::PixelFormat;
use std::fmt;

/// Failure converting a camera buffer to RGB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    WrongLength {
        format: PixelFormat,
        width: u32,
        height: u32,
        expected: usize,
        got: usize,
    },
    OddDimension {
        format: PixelFormat,
        width: u32,
        height: u32,
    },
    Unsupported(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::WrongLength {
                format,
                width,
                height,
                expected,
                got,
            } => write!(
                f,
                "{format:?} {width}x{height} needs {expected} bytes, got {got}"
            ),
            ConvertError::OddDimension {
                format,
                width,
                height,
            } => write!(f, "{format:?} {width}x{height} needs even dimensions"),
            ConvertError::Unsupported(name) => write!(f, "unsupported pixel format: {name}"),
        }
    }
}

impl std::error::Error for ConvertError {}

/// BT.601 limited-range YUV to RGB, integer fixed point (coefficients scaled by 256).
fn yuv_to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let c = y as i32 - 16;
    let d = u as i32 - 128;
    let e = v as i32 - 128;
    let r = (298 * c + 409 * e + 128) >> 8;
    let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
    let b = (298 * c + 516 * d + 128) >> 8;
    [clamp(r), clamp(g), clamp(b)]
}

fn clamp(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

/// Converts a camera buffer to tightly packed RGB bytes (`w*h*3`, row-major).
///
/// XRGB is read as little-endian B,G,R,X per pixel (Windows XRGB/BGRX); the X byte is dropped.
/// NV12 and YUY2 use BT.601 limited-range coefficients.
pub fn to_rgb(
    format: PixelFormat,
    width: u32,
    height: u32,
    data: &[u8],
) -> Result<Vec<u8>, ConvertError> {
    let (w, h) = (width as usize, height as usize);
    let expected = match format {
        PixelFormat::Rgb24 | PixelFormat::Bgr24 => w * h * 3,
        PixelFormat::Xrgb => w * h * 4,
        PixelFormat::Nv12 => {
            if !width.is_multiple_of(2) || !height.is_multiple_of(2) {
                return Err(ConvertError::OddDimension {
                    format,
                    width,
                    height,
                });
            }
            w * h * 3 / 2
        }
        PixelFormat::Yuy2 => {
            if !width.is_multiple_of(2) {
                return Err(ConvertError::OddDimension {
                    format,
                    width,
                    height,
                });
            }
            w * h * 2
        }
    };
    if data.len() != expected {
        return Err(ConvertError::WrongLength {
            format,
            width,
            height,
            expected,
            got: data.len(),
        });
    }

    let rgb = match format {
        PixelFormat::Rgb24 => data.to_vec(),
        PixelFormat::Bgr24 | PixelFormat::Xrgb => {
            // Xrgb has a padding byte after each B,G,R triple; it is dropped.
            let bpp = if matches!(format, PixelFormat::Bgr24) { 3 } else { 4 };
            data.chunks_exact(bpp)
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect()
        }
        PixelFormat::Nv12 => {
            let (y_plane, uv_plane) = data.split_at(w * h);
            let mut out = Vec::with_capacity(w * h * 3);
            for row in 0..h {
                for col in 0..w {
                    // One interleaved U,V pair covers a 2x2 block.
                    let uv = (row / 2) * w + (col / 2) * 2;
                    out.extend_from_slice(&yuv_to_rgb(
                        y_plane[row * w + col],
                        uv_plane[uv],
                        uv_plane[uv + 1],
                    ));
                }
            }
            out
        }
        PixelFormat::Yuy2 => {
            let mut out = Vec::with_capacity(w * h * 3);
            // Each 4-byte group Y0 U Y1 V covers two horizontal pixels.
            for px in data.chunks_exact(4) {
                out.extend_from_slice(&yuv_to_rgb(px[0], px[1], px[3]));
                out.extend_from_slice(&yuv_to_rgb(px[2], px[1], px[3]));
            }
            out
        }
    };
    Ok(rgb)
}

/// Like `to_rgb`, but takes the buffer by value. RGB24 input is returned as-is, without a copy.
pub fn to_rgb_owned(
    format: PixelFormat,
    width: u32,
    height: u32,
    data: Vec<u8>,
) -> Result<Vec<u8>, ConvertError> {
    match format {
        // A wrong length falls through to `to_rgb`, which returns the same WrongLength error.
        PixelFormat::Rgb24 if data.len() == width as usize * height as usize * 3 => Ok(data),
        _ => to_rgb(format, width, height, &data),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts each channel of `got` is within +-2 of `want`.
    fn assert_near(got: &[u8], want: &[u8]) {
        assert_eq!(got.len(), want.len(), "length");
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            assert!(
                (*g as i32 - *w as i32).abs() <= 2,
                "byte {i}: got {g}, want {w} (got {got:?})"
            );
        }
    }

    #[test]
    fn rgb24_is_identity() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let out = to_rgb(PixelFormat::Rgb24, 2, 2, &data).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn rgb24_owned_reuses_buffer() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let ptr = data.as_ptr();
        let out = to_rgb_owned(PixelFormat::Rgb24, 2, 2, data).unwrap();
        assert_eq!(out.as_ptr(), ptr);
        assert_eq!(out, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    }

    #[test]
    fn rgb24_owned_rejects_wrong_length() {
        let err = to_rgb_owned(PixelFormat::Rgb24, 2, 2, vec![0; 11]).unwrap_err();
        assert!(matches!(err, ConvertError::WrongLength { expected: 12, got: 11, .. }));
    }

    #[test]
    fn bgr24_swaps_channels() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let out = to_rgb(PixelFormat::Bgr24, 2, 2, &data).unwrap();
        assert_eq!(out, [3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]);
    }

    #[test]
    fn xrgb_drops_padding_byte() {
        // Little-endian B,G,R,X per pixel.
        let data = [10, 20, 30, 255, 40, 50, 60, 0, 70, 80, 90, 1, 100, 110, 120, 2];
        let out = to_rgb(PixelFormat::Xrgb, 2, 2, &data).unwrap();
        assert_eq!(out, [30, 20, 10, 60, 50, 40, 90, 80, 70, 120, 110, 100]);
    }

    #[test]
    fn nv12_white_black_red() {
        // 2x2: Y plane (4 bytes) then interleaved UV (one U,V pair for the block).
        // Pixel order: white, black / red, black-row-two pair shares UV.
        // Use 2x2 with UV=128,128 for white/black, then a second 2x2 block for red.
        let white_black = [235, 16, 235, 16, 128, 128];
        let out = to_rgb(PixelFormat::Nv12, 2, 2, &white_black).unwrap();
        assert_near(
            &out,
            &[255, 255, 255, 0, 0, 0, 255, 255, 255, 0, 0, 0],
        );

        let red = [81, 81, 81, 81, 90, 240];
        let out = to_rgb(PixelFormat::Nv12, 2, 2, &red).unwrap();
        assert_near(&out, &[255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0]);
    }

    #[test]
    fn yuy2_white_black_red() {
        // Each row: Y0 U Y1 V covers two horizontal pixels.
        let data = [
            235, 128, 16, 128, // row 0: white, black
            81, 90, 81, 240, // row 1: red, red
        ];
        let out = to_rgb(PixelFormat::Yuy2, 2, 2, &data).unwrap();
        assert_near(
            &out,
            &[255, 255, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0],
        );
    }

    #[test]
    fn wrong_length_is_rejected() {
        let err = to_rgb(PixelFormat::Rgb24, 2, 2, &[0; 11]).unwrap_err();
        assert_eq!(
            err,
            ConvertError::WrongLength {
                format: PixelFormat::Rgb24,
                width: 2,
                height: 2,
                expected: 12,
                got: 11,
            }
        );
        assert!(err.to_string().contains("Rgb24"));
        assert!(err.to_string().contains("2x2"));
    }

    #[test]
    fn odd_dimension_is_rejected() {
        let err = to_rgb(PixelFormat::Nv12, 3, 2, &[0; 9]).unwrap_err();
        assert!(matches!(err, ConvertError::OddDimension { width: 3, height: 2, .. }));
        let err = to_rgb(PixelFormat::Yuy2, 3, 2, &[0; 12]).unwrap_err();
        assert!(matches!(err, ConvertError::OddDimension { width: 3, height: 2, .. }));
    }
}
