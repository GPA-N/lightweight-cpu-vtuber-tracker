//! InstantHMR: a RepViT regressor that turns one 224x224 person crop into 70 body keypoints in 2D
//! and 3D (the MHR70 layout, see `JOINT_NAMES`). The model file is the one from
//! `momolesang/InstantHMR` cut down to the two joint outputs (`scripts/fetch_instanthmr.ps1`); its
//! weights are under the SAM license and are not part of this repository.
//!
//! Inputs: `image` (1, 3, 224, 224), RGB, 0..1, ImageNet mean and std, and `cliff_cond` (1, 3), the
//! person box seen by the camera. Outputs: `joints_2d` (70, 2) in the crop, -1..1, and `joints_3d`
//! (70, 3) in metres, body-centred, x right, y down, z away from the camera.

use std::path::Path;

use crate::person_box::PersonBox;
use crate::engine::{load_model, Engine, ModelRun, NamedInput, NamedOutput};
use crate::upper_body::Image;

pub const JOINTS: usize = 70;
pub const INPUT: usize = 224;
/// The crop is a square this much wider than the person box's long side (the model was trained so).
pub const CROP_EXPAND: f32 = 1.2;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Indices in the MHR70 layout. "Left" is the person's own left, as in COCO. A finger's `third`
/// joint is its knuckle (the one nearest the wrist).
pub mod joint {
    pub const NOSE: usize = 0;
    pub const L_EAR: usize = 3;
    pub const R_EAR: usize = 4;
    pub const L_SHOULDER: usize = 5;
    pub const R_SHOULDER: usize = 6;
    pub const L_ELBOW: usize = 7;
    pub const R_ELBOW: usize = 8;
    pub const R_INDEX_KNUCKLE: usize = 28;
    pub const R_PINKY_KNUCKLE: usize = 40;
    pub const R_WRIST: usize = 41;
    pub const L_INDEX_KNUCKLE: usize = 49;
    pub const L_PINKY_KNUCKLE: usize = 61;
    pub const L_WRIST: usize = 62;
}

/// The square region of the frame that goes into the model, in frame pixels (may reach outside the frame).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crop {
    pub x0: f32,
    pub y0: f32,
    pub size: f32,
}

/// A square crop centred on the person box.
pub fn square_crop(person: PersonBox) -> Crop {
    let size = (person.w.max(person.h) * CROP_EXPAND).max(1.0);
    Crop { x0: person.x + person.w / 2.0 - size / 2.0, y0: person.y + person.h / 2.0 - size / 2.0, size }
}

/// The person box as the model's camera condition: the box centre in -1..1 and its long side over
/// the long side of the frame.
pub fn cliff_cond(person: PersonBox, frame_w: f32, frame_h: f32) -> [f32; 3] {
    let (cx, cy) = (person.x + person.w / 2.0, person.y + person.h / 2.0);
    [2.0 * cx / frame_w - 1.0, 2.0 * cy / frame_h - 1.0, person.w.max(person.h) / frame_w.max(frame_h)]
}

/// Bilinear resize of the crop to 224x224, normalised, planar (CHW). Pixels outside the frame are black.
pub fn preprocess(img: &Image, crop: Crop) -> Vec<f32> {
    let plane = INPUT * INPUT;
    let mut out = vec![0.0f32; 3 * plane];
    let scale = crop.size / INPUT as f32;
    let at = |x: i64, y: i64, c: usize| -> f32 {
        if x < 0 || y < 0 || x >= img.w as i64 || y >= img.h as i64 {
            0.0
        } else {
            img.rgb[(y as usize * img.w + x as usize) * 3 + c] as f32 / 255.0
        }
    };
    for oy in 0..INPUT {
        let sy = crop.y0 + (oy as f32 + 0.5) * scale - 0.5;
        let (y0, fy) = (sy.floor() as i64, sy - sy.floor());
        for ox in 0..INPUT {
            let sx = crop.x0 + (ox as f32 + 0.5) * scale - 0.5;
            let (x0, fx) = (sx.floor() as i64, sx - sx.floor());
            for c in 0..3 {
                let top = at(x0, y0, c) * (1.0 - fx) + at(x0 + 1, y0, c) * fx;
                let bottom = at(x0, y0 + 1, c) * (1.0 - fx) + at(x0 + 1, y0 + 1, c) * fx;
                let v = top * (1.0 - fy) + bottom * fy;
                out[c * plane + oy * INPUT + ox] = (v - MEAN[c]) / STD[c];
            }
        }
    }
    out
}

/// Joints of one person: 2D in frame pixels, 3D in metres (see the module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct Joints {
    pub xy: [[f32; 2]; JOINTS],
    pub xyz: [[f32; 3]; JOINTS],
}

/// Reads the two joint outputs (found by their shape, so the order and names do not matter) and maps the
/// 2D ones from the crop back to the frame.
pub fn decode(outs: &[NamedOutput], crop: Crop) -> Result<Joints, String> {
    let find = |dim: usize| {
        outs.iter()
            .find(|o| o.shape.iter().product::<usize>() == JOINTS * dim && o.shape.last() == Some(&dim))
            .ok_or_else(|| format!("no output of {JOINTS}x{dim} values"))
    };
    let (two, three) = (find(2)?, find(3)?);
    let scale = crop.size / INPUT as f32;
    let mut joints = Joints { xy: [[0.0; 2]; JOINTS], xyz: [[0.0; 3]; JOINTS] };
    for i in 0..JOINTS {
        for k in 0..2 {
            let crop_px = (two.data[i * 2 + k] + 1.0) * 0.5 * INPUT as f32;
            joints.xy[i][k] = crop_px * scale + if k == 0 { crop.x0 } else { crop.y0 };
        }
        joints.xyz[i].copy_from_slice(&three.data[i * 3..i * 3 + 3]);
    }
    Ok(joints)
}

pub struct InstantHmr {
    model: Box<dyn ModelRun + Send>,
}

impl InstantHmr {
    pub fn load(engine: Engine, path: &Path, threads: usize) -> Result<Self, String> {
        Ok(Self { model: load_model(engine, path, threads).map_err(|e| e.to_string())? })
    }

    /// Runs the model on the crop around `person` (a box in frame pixels).
    pub fn run(&mut self, img: &Image, person: PersonBox) -> Result<Joints, String> {
        if img.rgb.len() != img.w * img.h * 3 || img.w == 0 || img.h == 0 {
            return Err("image buffer does not match its size".into());
        }
        let crop = square_crop(person);
        let image = preprocess(img, crop);
        let cond = cliff_cond(person, img.w as f32, img.h as f32);
        let outs = self
            .model
            .run_two(
                NamedInput { name: "image", data: &image, shape: &[1, 3, INPUT, INPUT] },
                NamedInput { name: "cliff_cond", data: &cond, shape: &[1, 3] },
            )
            .map_err(|e| e.to_string())?;
        decode(&outs, crop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(shape: &[usize], f: impl Fn(usize) -> f32) -> NamedOutput {
        NamedOutput { name: "x".into(), shape: shape.to_vec(), data: (0..shape.iter().product()).map(f).collect() }
    }

    #[test]
    fn the_crop_is_a_square_around_the_box_centre_and_a_fifth_wider() {
        let c = square_crop(PersonBox { x: 100.0, y: 50.0, w: 200.0, h: 100.0 });
        assert!((c.size - 240.0).abs() < 1e-3);
        assert!((c.x0 + c.size / 2.0 - 200.0).abs() < 1e-3 && (c.y0 + c.size / 2.0 - 100.0).abs() < 1e-3);
    }

    #[test]
    fn the_camera_condition_is_centre_and_relative_size() {
        let b = PersonBox { x: 0.0, y: 0.0, w: 640.0, h: 240.0 };
        let c = cliff_cond(b, 1280.0, 720.0);
        assert_eq!(c, [2.0 * 320.0 / 1280.0 - 1.0, 2.0 * 120.0 / 720.0 - 1.0, 0.5]);
    }

    #[test]
    fn a_constant_image_gives_the_normalised_constant_and_black_outside_the_frame() {
        let rgb = vec![255u8; 8 * 8 * 3];
        let img = Image { rgb: &rgb, w: 8, h: 8 };
        // Crop exactly the frame, then one entirely to the left of it.
        let inside = preprocess(&img, Crop { x0: 0.0, y0: 0.0, size: 8.0 });
        let plane = INPUT * INPUT;
        assert!((inside[plane / 2 + INPUT / 2] - (1.0 - MEAN[0]) / STD[0]).abs() < 1e-4);
        assert!((inside[plane + plane / 2 + INPUT / 2] - (1.0 - MEAN[1]) / STD[1]).abs() < 1e-4);
        let outside = preprocess(&img, Crop { x0: -100.0, y0: 0.0, size: 8.0 });
        assert!((outside[0] - (0.0 - MEAN[0]) / STD[0]).abs() < 1e-4);
        assert_eq!(inside.len(), 3 * plane);
    }

    #[test]
    fn two_d_joints_map_from_the_crop_to_the_frame() {
        let crop = Crop { x0: 100.0, y0: 200.0, size: 448.0 };
        let two = out(&[1, JOINTS, 2], |i| if i % 2 == 0 { -1.0 } else { 0.0 });
        let three = out(&[1, JOINTS, 3], |i| i as f32);
        let j = decode(&[three, two], crop).unwrap();
        // x = -1 is the crop's left edge, y = 0 its middle.
        assert!((j.xy[5][0] - 100.0).abs() < 1e-3 && (j.xy[5][1] - (200.0 + 224.0)).abs() < 1e-3, "{:?}", j.xy[5]);
        assert_eq!(j.xyz[1], [3.0, 4.0, 5.0]);
    }

    #[test]
    fn a_missing_output_is_an_error() {
        let crop = Crop { x0: 0.0, y0: 0.0, size: 224.0 };
        assert!(decode(&[out(&[1, JOINTS, 2], |_| 0.0)], crop).is_err());
    }

    /// Runs the real model when it is installed: a 3D body of human size must come out of a photo.
    #[test]
    fn the_real_model_gives_a_human_sized_body() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let model = root.join("models/upper_body/instanthmr_joints.onnx");
        let photo = root.join("fixtures/private/upper_body_photos/thumbs_up.png");
        if !model.exists() || !photo.exists() {
            eprintln!("SKIP instant_hmr: model or photo missing");
            return;
        }
        let rgb = image::open(photo).unwrap().to_rgb8();
        let img = Image { rgb: rgb.as_raw(), w: rgb.width() as usize, h: rgb.height() as usize };
        let mut hmr = InstantHmr::load(Engine::Ort, &model, 4).unwrap();
        let person = PersonBox { x: 0.0, y: 0.0, w: img.w as f32, h: img.h as f32 };
        let j = hmr.run(&img, person).unwrap();
        let d = |a: usize, b: usize| {
            let (p, q) = (j.xyz[a], j.xyz[b]);
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
        };
        let shoulders = d(joint::L_SHOULDER, joint::R_SHOULDER);
        assert!((0.25..0.6).contains(&shoulders), "shoulder width {shoulders} m");
        let forearm = d(joint::L_ELBOW, joint::L_WRIST);
        assert!((0.12..0.4).contains(&forearm), "forearm {forearm} m");
        assert!(j.xy[joint::L_SHOULDER][0] != j.xy[joint::R_SHOULDER][0]);
    }
}
