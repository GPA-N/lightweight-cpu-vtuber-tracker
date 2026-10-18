//! Parity of the Rust CIGPose-m stage with the Python reference on the four user photos.
//!
//! Run in release for meaningful timings:
//! `cargo test --release -p infer --test cigpose_parity -- --nocapture`
//!
//! Needs the private photos, the reference JSON and `models/upper_body/cigpose-m_coco_256x192.onnx`;
//! it skips with a message when any is missing. Engine from `VT_ENGINE` (default auto); for
//! OpenVINO the DLL folder defaults to the throwaway Python venv (`VT_OPENVINO_LIBS` overrides).

use std::path::{Path, PathBuf};
use std::time::Instant;

use infer::cigpose::{CigPose, LimbStage, PersonBox, KEYPOINTS};
use infer::engine::resolve_engine;
use infer::upper_body::Image;

const TOLERANCE_PX: f32 = 2.0;
const PHOTOS: [&str; 4] = ["hands_15", "hands_16", "hands_17", "hands_19"];

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn add_openvino_to_path() {
	let libs = std::env::var_os("VT_OPENVINO_LIBS").map(PathBuf::from).unwrap_or_else(|| {
		root().join("fixtures/private/venv_rt/Lib/site-packages/openvino/libs")
	});
	let mut paths = vec![libs];
	paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
	// SAFETY: called once at the start of the only test in this binary, before other threads exist.
	unsafe { std::env::set_var("PATH", std::env::join_paths(paths).unwrap()) };
}

#[test]
fn cigpose_matches_python_reference_on_user_photos() {
	let model = root().join("models/upper_body/cigpose-m_coco_256x192.onnx");
	let reference = root().join("fixtures/private/out/shootout/shootout/results/l2_cigpose_m_coco.json");
	let photos = root().join("fixtures/private/upper_body_photos");
	if !model.exists() || !reference.exists() || !photos.join("hands_15.png").exists() {
		eprintln!("SKIP cigpose_parity: private fixtures or model missing ({model:?}, {reference:?}, {photos:?})");
		return;
	}
	add_openvino_to_path();
	let engine = resolve_engine();
	eprintln!("engine: {engine:?}");
	let mut stage = CigPose::load(engine, &model, 4).expect("load cigpose");
	let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&reference).unwrap()).unwrap();

	let mut worst_overall = 0.0f32;
	for name in PHOTOS {
		let person = &json["images"][name]["people"][0];
		let b: Vec<f32> = person["box"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect();
		let want: Vec<[f32; 2]> = person["kp"]
			.as_array()
			.unwrap()
			.iter()
			.map(|p| [p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32])
			.collect();
		let rgb = image::open(photos.join(format!("{name}.png"))).unwrap().to_rgb8();
		let (w, h) = (rgb.width() as usize, rgb.height() as usize);
		let img = Image { rgb: rgb.as_raw(), w, h };
		let boxed = PersonBox { x: b[0], y: b[1], w: b[2] - b[0], h: b[3] - b[1] };

		let got = stage.run(&img, boxed).expect("run");
		assert_eq!(got.len(), KEYPOINTS);
		let (mut worst, mut at) = (0.0f32, 0);
		for (i, (g, w)) in got.iter().zip(&want).enumerate() {
			let err = (g.x - w[0]).abs().max((g.y - w[1]).abs());
			if err > worst {
				(worst, at) = (err, i);
			}
		}
		eprintln!("{name}: max px error {worst:.3} (joint {at})");
		worst_overall = worst_overall.max(worst);
		assert!(worst <= TOLERANCE_PX, "{name}: joint {at} off by {worst:.2} px");
	}
	eprintln!("max px error over all photos: {worst_overall:.3}");

	// Timing: full call (crop, resize, normalise, run, decode) on hands_17, after warm-up.
	let rgb = image::open(photos.join("hands_17.png")).unwrap().to_rgb8();
	let img = Image { rgb: rgb.as_raw(), w: rgb.width() as usize, h: rgb.height() as usize };
	let boxed = PersonBox { x: 27.69, y: 97.09, w: 524.13 - 27.69, h: 361.56 - 97.09 };
	for _ in 0..5 {
		stage.run(&img, boxed).unwrap();
	}
	let mut ms: Vec<f64> = (0..40)
		.map(|_| {
			let t = Instant::now();
			stage.run(&img, boxed).unwrap();
			t.elapsed().as_secs_f64() * 1000.0
		})
		.collect();
	ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
	eprintln!("median ms per call: {:.2} (min {:.2}, max {:.2})", ms[ms.len() / 2], ms[0], ms[ms.len() - 1]);
}
