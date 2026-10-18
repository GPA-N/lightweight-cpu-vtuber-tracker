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
    // Source index and lerp weight for each output row and column, computed once instead of per pixel.
    let axis = |origin: f32| -> Vec<(i64, f32)> {
        (0..INPUT)
            .map(|o| {
                let s = origin + (o as f32 + 0.5) * scale - 0.5;
                (s.floor() as i64, s - s.floor())
            })
            .collect()
    };
    let rows = axis(crop.y0);
    let cols = axis(crop.x0);
    let at = |x: i64, y: i64, c: usize| -> f32 {
        if x < 0 || y < 0 || x >= img.w as i64 || y >= img.h as i64 {
            0.0
        } else {
            img.rgb[(y as usize * img.w + x as usize) * 3 + c] as f32 / 255.0
        }
    };
    for (oy, &(y0, fy)) in rows.iter().enumerate() {
        for (ox, &(x0, fx)) in cols.iter().enumerate() {
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
    use crate::engine::OrtModel;
    use crate::session::{build_session_with, default_threads, SessionOptions};
    use ort::session::builder::GraphOptimizationLevel;

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

    /// The per-pixel closure version of `preprocess`, kept here to check the fast version against it.
    fn preprocess_reference(img: &Image, crop: Crop) -> Vec<f32> {
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

    #[test]
    fn preprocess_matches_the_reference_on_a_random_image() {
        // Seeded LCG: a fixed pseudo-random image, so the test is repeatable.
        let mut seed: u32 = 12345;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8
        };
        let (w, h) = (37usize, 23usize);
        let rgb: Vec<u8> = (0..w * h * 3).map(|_| next()).collect();
        let img = Image { rgb: &rgb, w, h };
        // Inside the frame, partly outside on each side, and a crop larger than the frame.
        let crops = [
            Crop { x0: 0.0, y0: 0.0, size: 23.0 },
            Crop { x0: -5.3, y0: 10.7, size: 14.6 },
            Crop { x0: 20.4, y0: -7.9, size: 31.2 },
            Crop { x0: -40.0, y0: 3.0, size: 90.0 },
        ];
        for crop in crops {
            let fast = preprocess(&img, crop);
            let slow = preprocess_reference(&img, crop);
            assert_eq!(fast.len(), slow.len());
            let worst = fast.iter().zip(&slow).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(worst < 1e-5, "crop {crop:?}: max diff {worst}");
        }
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

    /// Session-option sweep for docs/notes/instanthmr.md, "Session options". Ignored by default.
    /// Driver (VT_SWEEP_SPEC unset): for each `label=spec` in VT_SWEEP_VARIANTS (`;`-separated, first
    /// is the reference) runs a fresh child process of this test binary and prints one `SWEEP|` row.
    /// Child (VT_SWEEP_SPEC set): 1 warm-up plus 50 timed runs per photo, writes to VT_SWEEP_OUT.
    /// Spec: comma list of `opt=0|1|2|3|all`, `mempat=0|1`, `arena=0|1`, `threads=N`.
    #[test]
    #[ignore]
    fn session_option_sweep() {
        match std::env::var("VT_SWEEP_SPEC") {
            Ok(spec) => sweep_child(&spec, &std::env::var("VT_SWEEP_OUT").expect("VT_SWEEP_OUT")),
            Err(_) => sweep_driver(&std::env::var("VT_SWEEP_VARIANTS").expect("VT_SWEEP_VARIANTS")),
        }
    }

    const SWEEP_RUNS: usize = 50;

    fn sweep_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Photos as (rgb, w, h), in file-name order.
    fn sweep_photos() -> Vec<(Vec<u8>, usize, usize)> {
        let dir = sweep_root().join("fixtures/private/upper_body_photos");
        let mut paths: Vec<_> = std::fs::read_dir(&dir).expect("photo dir").map(|e| e.unwrap().path()).collect();
        paths.sort();
        paths
            .iter()
            .map(|p| {
                let rgb = image::open(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())).to_rgb8();
                let (w, h) = (rgb.width() as usize, rgb.height() as usize);
                (rgb.into_raw(), w, h)
            })
            .collect()
    }

    fn parse_spec(spec: &str) -> (SessionOptions, usize) {
        let mut opts = SessionOptions::default();
        let mut threads = default_threads();
        for item in spec.split(',').filter(|s| !s.is_empty()) {
            let (k, v) = item.split_once('=').expect("key=value");
            match k {
                "opt" => {
                    opts.optimization = match v {
                        "0" => GraphOptimizationLevel::Disable,
                        "1" => GraphOptimizationLevel::Level1,
                        "2" => GraphOptimizationLevel::Level2,
                        "3" => GraphOptimizationLevel::Level3,
                        "all" => GraphOptimizationLevel::All,
                        _ => panic!("opt={v}"),
                    }
                }
                "mempat" => opts.memory_pattern = v == "1",
                "arena" => opts.cpu_arena = v == "1",
                "threads" => threads = v.parse().expect("threads=N"),
                _ => panic!("unknown key {k}"),
            }
        }
        (opts, threads)
    }

    /// Peak working set of this process in bytes (Windows: GetProcessMemoryInfo, declared here to avoid a dependency).
    #[cfg(windows)]
    fn peak_working_set_bytes() -> Option<usize> {
        #[repr(C)]
        struct Counters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }
        #[link(name = "psapi")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessMemoryInfo(process: *mut std::ffi::c_void, counters: *mut Counters, cb: u32) -> i32;
        }
        let mut c = Counters {
            cb: std::mem::size_of::<Counters>() as u32,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: the pseudo handle is always valid; `c` is a PROCESS_MEMORY_COUNTERS with `cb` set.
        let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
        (ok != 0).then_some(c.peak_working_set_size)
    }

    #[cfg(not(windows))]
    fn peak_working_set_bytes() -> Option<usize> {
        None
    }

    fn sweep_child(spec: &str, out: &str) {
        let model = sweep_root().join("models/upper_body/instanthmr_joints.onnx");
        assert!(model.exists(), "model missing: {}", model.display());
        let (opts, threads) = parse_spec(spec);
        let (session, _) = build_session_with(&model, threads, &opts).unwrap();
        let mut hmr = InstantHmr { model: Box::new(OrtModel::new(session)) };
        let photos = sweep_photos();
        let mut times = Vec::new();
        let mut first = String::new();
        for (rgb, w, h) in &photos {
            let img = Image { rgb, w: *w, h: *h };
            let person = PersonBox { x: 0.0, y: 0.0, w: *w as f32, h: *h as f32 };
            hmr.run(&img, person).unwrap();
            for run in 0..SWEEP_RUNS {
                let t = std::time::Instant::now();
                let j = hmr.run(&img, person).unwrap();
                times.push(t.elapsed().as_secs_f64() * 1e3);
                if run == 0 {
                    for p in j.xyz {
                        first.push_str(&format!("{} {} {} ", p[0], p[1], p[2]));
                    }
                    first.push('\n');
                }
            }
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = times.len();
        let median = times[n / 2];
        let p95 = times[((n as f64 * 0.95).ceil() as usize - 1).min(n - 1)];
        let peak = peak_working_set_bytes().unwrap_or(0);
        std::fs::write(out, format!("{peak} {median} {p95}\n{first}")).unwrap();
    }

    fn sweep_driver(list: &str) {
        let exe = std::env::current_exe().unwrap();
        let mut reference: Option<Vec<Vec<f32>>> = None;
        for item in list.split(';').filter(|s| !s.trim().is_empty()) {
            let (label, spec) = item.split_once('=').expect("label=spec");
            let out = std::env::temp_dir().join(format!("vt_sweep_{label}.txt"));
            let status = std::process::Command::new(&exe)
                .args(["--ignored", "--exact", "instant_hmr::tests::session_option_sweep", "--test-threads=1"])
                .env("VT_SWEEP_SPEC", spec)
                .env("VT_SWEEP_OUT", &out)
                .status()
                .unwrap();
            assert!(status.success(), "child {label} failed");
            let text = std::fs::read_to_string(&out).unwrap();
            let mut lines = text.lines();
            let head: Vec<f64> = lines.next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
            let photos: Vec<Vec<f32>> = lines
                .map(|l| l.split_whitespace().map(|v| v.parse().unwrap()).collect())
                .collect();
            let diff = match &reference {
                None => {
                    reference = Some(photos.clone());
                    0.0f32
                }
                Some(r) => r
                    .iter()
                    .zip(&photos)
                    .flat_map(|(a, b)| a.iter().zip(b).map(|(x, y)| (x - y).abs()))
                    .fold(0.0f32, f32::max),
            };
            println!(
                "SWEEP|{label}|{spec}|peak_mb={:.1}|median_ms={:.2}|p95_ms={:.2}|max_diff_m={diff:.6}",
                head[0] / 1048576.0,
                head[1],
                head[2]
            );
        }
    }
}
