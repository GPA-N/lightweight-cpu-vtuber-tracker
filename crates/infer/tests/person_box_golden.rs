//! Person-box detection rate on the 236 golden_take frames, and boxes on the four user photos
//! (OpenSpec task 11.4).
//!
//! `cargo test --release -p infer --test person_box_golden -- --nocapture`
//!
//! Face box: bounding box of the 478 face-tracker landmarks recorded per frame in
//! `golden_take/lm.txt` (frame pixels, written by `Tracker::last_landmarks`).
//! Truth for "usable": BlazePose `pose_landmarks` shoulders (11, 12) and elbows (13, 14) that are
//! visible (visibility >= 0.5) and inside the frame; the box must contain all of them.
//! Skips with a message when private fixtures or models are missing.

use std::path::{Path, PathBuf};

use facelogic::crop::RotatedRect;
use infer::cigpose::{CigPose, Keypoint, LimbStage, PersonBox, KEYPOINTS, L_ELBOW, L_SHOULDER, R_ELBOW, R_SHOULDER};
use infer::engine::{load_model, Engine};
use infer::person_box::{face_rect, from_face, from_face_with, PersonBoxTracker};
use infer::upper_body::{decode_detections, decode_pose, letterbox_rect, nms, pose_roi, ssd_anchors, warp_black, Image};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn contains(b: PersonBox, p: (f32, f32)) -> bool {
    p.0 >= b.x && p.0 <= b.x + b.w && p.1 >= b.y && p.1 <= b.y + b.h
}

struct Frame {
    rgb: Vec<u8>,
    w: usize,
    h: usize,
    /// None when the face tracker had no landmarks on this frame.
    face: Option<PersonBox>,
    /// BlazePose shoulders and elbows with visibility >= 0.5, clamped onto the frame.
    truth: Vec<(f32, f32)>,
}

fn out<'a>(o: &'a [infer::engine::NamedOutput], name: &str) -> &'a [f32] {
    &o.iter().find(|x| x.name == name).unwrap_or_else(|| panic!("no output {name}")).data
}

#[test]
fn person_box_detection_rate_on_golden_take() {
    let dir = root().join("fixtures/private/golden_take");
    let ub = root().join("models/upper_body");
    let cig = ub.join("cigpose-m_coco_256x192.onnx");
    if !dir.join("lm.txt").exists() || !cig.exists() || !ub.join("pose_landmarks.onnx").exists() {
        eprintln!("SKIP person_box_golden: private fixtures or models missing");
        return;
    }
    let mut det = load_model(Engine::Ort, &ub.join("pose_detector.onnx"), 4).unwrap();
    let mut lm = load_model(Engine::Ort, &ub.join("pose_landmarks.onnx"), 4).unwrap();
    let mut stage = CigPose::load(Engine::Ort, &cig, 4).unwrap();
    let anchors = ssd_anchors(224, &[8, 16, 32, 32, 32]);

    let lm_txt = std::fs::read_to_string(dir.join("lm.txt")).unwrap();
    let mut frames = Vec::new();
    let mut last_face: Option<PersonBox> = None; // held while the face tracker has no landmarks
    let mut held = 0;
    for (i, line) in lm_txt.lines().enumerate() {
        let v: Vec<f32> = line.split_whitespace().skip(1).map(|t| t.parse().unwrap()).collect();
        let pts: Vec<[f32; 3]> = v.chunks_exact(2).map(|c| [c[0], c[1], 0.0]).collect();
        let seen = face_rect(&pts);
        held += seen.is_none() as usize;
        let face = seen.or(last_face);
        last_face = face;
        let rgb = image::open(dir.join(format!("f{i:04}.jpg"))).unwrap().to_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);
        let img = Image { rgb: rgb.as_raw(), w, h };
        let d = det.run(&warp_black(&img, &letterbox_rect(w, h), 224, true), &[1, 224, 224, 3]).unwrap();
        let dets = decode_detections(out(&d, "Identity"), out(&d, "Identity_1"), &anchors, 224, 4, 0.5).unwrap();
        let roi = nms(dets, 0.3).first().and_then(|d| pose_roi(d, w, h)).unwrap_or_else(|| {
            let s = letterbox_rect(w, h);
            RotatedRect { width: s.width * 1.25, height: s.height * 1.25, ..s }
        });
        let o = lm.run(&warp_black(&img, &roi, 256, false), &[1, 256, 256, 3]).unwrap();
        let world = o.iter().find(|x| x.data.len() == 117).map(|x| x.data.clone()).unwrap();
        let pose = decode_pose(out(&o, "Identity"), &world, &roi).unwrap();
        let truth = [11usize, 12, 13, 14]
            .iter()
            .map(|&k| &pose[k])
            .filter(|p| p.visibility >= 0.5)
            .map(|p| (p.x.clamp(0.0, w as f32), p.y.clamp(0.0, h as f32)))
            .collect();
        frames.push(Frame { rgb: rgb.into_raw(), w, h, face, truth });
    }
    assert_eq!(frames.len(), 236);
    let with_truth: Vec<&Frame> = frames.iter().filter(|f| !f.truth.is_empty()).collect();
    println!("frames {}, with >=1 confident shoulder/elbow truth points: {}", frames.len(), with_truth.len());
    println!("frames without face landmarks (last face box held): {held}");
    let mut hist = [0usize; 5];
    for f in &frames {
        hist[f.truth.len()] += 1;
    }
    println!("truth points per frame histogram (0..4): {hist:?}");

    // Grid of face-box factors: containment rate over the frames with truth.
    let mut best = (0.0f32, 0.0f32, 0.0f32);
    for wf in [3.0f32, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 7.0] {
        for tf in [0.0f32, 0.25, 0.5, 1.0] {
            let ok = with_truth.iter().filter(|f| {
                f.face.is_some_and(|face| {
                    let b = from_face_with(face, f.w as f32, f.h as f32, wf, tf);
                    f.truth.iter().all(|&p| contains(b, p))
                })
            });
            let rate = ok.count() as f32 / with_truth.len() as f32;
            println!("face box wf {wf} tf {tf}: {:.1}%", rate * 100.0);
            if rate > best.0 {
                best = (rate, wf, tf);
            }
        }
    }
    println!("best {best:?}");

    // Tracked pipeline with the shipped constants.
    let mut tracker = PersonBoxTracker::new();
    let (mut face_ok, mut tracked_ok, mut from_kp, mut lost) = (0, 0, 0, 0);
    let mut score_hist = Vec::new();
    for f in &frames {
        let (fw, fh) = (f.w as f32, f.h as f32);
        let img = Image { rgb: &f.rgb, w: f.w, h: f.h };
        let fb = f.face.map(|face| from_face(face, fw, fh));
        let usable = |b: Option<PersonBox>| !f.truth.is_empty() && b.is_some_and(|b| f.truth.iter().all(|&p| contains(b, p)));
        face_ok += usable(fb) as usize;
        let b = tracker.next(f.face, fw, fh);
        from_kp += (b.is_some() && b != fb) as usize;
        tracked_ok += usable(b) as usize;
        let k = b.and_then(|b| { let r: Result<[Keypoint; KEYPOINTS], String> = stage.run(&img, b); r.ok() });
        if let Some(k) = &k {
            score_hist.push([k[L_SHOULDER].score, k[R_SHOULDER].score, k[L_ELBOW].score, k[R_ELBOW].score]);
        }
        let had_kp_box = b.is_some() && b != fb;
        tracker.update(k.as_ref(), fw, fh);
        lost += (had_kp_box && tracker.next(None, fw, fh).is_none()) as usize;
    }
    let n = with_truth.len() as f32;
    println!("face-only box usable: {face_ok}/{} ({:.1}%)", with_truth.len(), 100.0 * face_ok as f32 / n);
    println!("tracked box usable: {tracked_ok}/{} ({:.1}%); frames using keypoint box {from_kp}; fell back {lost}", with_truth.len(), 100.0 * tracked_ok as f32 / n);
    let mut sh: Vec<f32> = score_hist.iter().flat_map(|s| [s[0], s[1]]).collect();
    sh.sort_by(|a, b| a.total_cmp(b));
    let mut el: Vec<f32> = score_hist.iter().flat_map(|s| [s[2], s[3]]).collect();
    el.sort_by(|a, b| a.total_cmp(b));
    println!("cigpose elbow score quantiles 10/50/90: {:.2} {:.2} {:.2}", el[el.len() / 10], el[el.len() / 2], el[el.len() * 9 / 10]);
    println!("cigpose shoulder score quantiles 10/50/90: {:.2} {:.2} {:.2}", sh[sh.len() / 10], sh[sh.len() / 2], sh[sh.len() * 9 / 10]);

    assert!(tracked_ok as f32 / n >= 0.95, "detection rate below 95%");
}

#[test]
fn person_box_on_user_photos() {
    use infer::blendshapes::BlendshapeModel;
    use infer::detector::Detector;
    use infer::landmarks::LandmarkModel;
    use infer::session::build_session;
    use infer::tracker::Tracker;

    let ub = root().join("models/upper_body");
    let cig = ub.join("cigpose-m_coco_256x192.onnx");
    let reference = root().join("fixtures/private/out/shootout/shootout/results/l2_cigpose_m_coco.json");
    let photos = root().join("fixtures/private/upper_body_photos");
    if !cig.exists() || !reference.exists() || !photos.join("hands_15.png").exists() {
        eprintln!("SKIP person_box_on_user_photos: private fixtures or models missing");
        return;
    }
    let models = root().join("models");
    let load = |n: &str| build_session(&models.join(n), 4).unwrap().0;
    let mut tracker = Tracker::new(
        Detector::new(load("face_detector.onnx")),
        LandmarkModel::new(load("face_landmarks.onnx")),
        BlendshapeModel::new(load("face_blendshapes.onnx")),
    );
    let mut stage = CigPose::load(Engine::Ort, &cig, 4).unwrap();
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&reference).unwrap()).unwrap();
    let names = ["shoulderL", "shoulderR", "elbowL", "elbowR", "wristL", "wristR"];
    for photo in ["hands_15", "hands_16", "hands_17", "hands_19"] {
        let rgb = image::open(root().join(format!("fixtures/private/upper_body_photos/{photo}.png"))).unwrap().to_rgb8();
        let (w, h) = (rgb.width() as usize, rgb.height() as usize);
        let frame = core::frame::Frame {
            width: w as u32,
            height: h as u32,
            format: core::frame::PixelFormat::Rgb24,
            bytes: rgb.as_raw().clone(),
            arrived: std::time::Instant::now(),
        };
        for _ in 0..3 {
            tracker.step(&frame).unwrap();
        }
        let face = tracker.last_landmarks().and_then(face_rect);
        let want: Vec<(f32, f32)> = json["images"][photo]["people"][0]["kp"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32))
            .collect();
        let proxy = |want: &[(f32, f32)]| {
            // Stand-in face box from the reference ears and eyes: width 1.1 x ear distance, height 1.3 x width.
            let (l, r) = (want[3], want[4]);
            let fw = ((l.0 - r.0).powi(2) + (l.1 - r.1).powi(2)).sqrt() * 1.1;
            let c = ((l.0 + r.0) / 2.0, (l.1 + r.1) / 2.0);
            PersonBox { x: c.0 - fw / 2.0, y: c.1 - fw * 0.65, w: fw, h: fw * 1.3 }
        };
        let tracker_found = face.is_some();
        let face = face.unwrap_or_else(|| proxy(&want));
        if !tracker_found {
            println!("{photo}: face tracker found NO face; using a stand-in face box from the reference ears");
        }
        let img = Image { rgb: rgb.as_raw(), w, h };
        for wf in [4.0f32, 5.0, 5.5, 6.0, 6.5, 7.0, 8.0, 9.0] {
            let b = from_face_with(face, w as f32, h as f32, wf, 0.5);
            println!("  grid wf {wf}: reference joints inside {}/6", (5..=10).filter(|&j| contains(b, want[j])).count());
        }
        let mut pb = PersonBoxTracker::new();
        let b1 = pb.next(Some(face), w as f32, h as f32).unwrap();
        let k1 = stage.run(&img, b1).unwrap();
        pb.update(Some(&k1), w as f32, h as f32);
        let b2 = pb.next(Some(face), w as f32, h as f32).unwrap();
        let k2 = stage.run(&img, b2).unwrap();
        println!("{photo}: {w}x{h} face box {face:?} (tracker {tracker_found})");
        println!("  all scores kp0..10 face-box run: {:?}", k1[..11].iter().map(|k| (k.score * 100.0).round() / 100.0).collect::<Vec<_>>());
        println!("  face-derived box {b1:?}, keypoint box {b2:?}");
        for (label, b, k) in [("face box", b1, &k1), ("kp box", b2, &k2)] {
            let inside = (5..=10).filter(|&j| contains(b, want[j])).count();
            print!("  {label}: reference joints inside box {inside}/6; err px / score:");
            for (j, n) in names.iter().enumerate() {
                let (g, r) = (k[5 + j], want[5 + j]);
                print!(" {n} {:.1}/{:.1}", ((g.x - r.0).powi(2) + (g.y - r.1).powi(2)).sqrt(), g.score);
            }
            println!();
            assert_eq!(inside, 6, "{photo} {label}: reference joints outside the box");
            for j in 5..=10 {
                let (g, r) = (k[j], want[j]);
                assert!(g.score >= infer::person_box::MIN_SCORE, "{photo} {label} joint {j} not found: {}", g.score);
                assert!(((g.x - r.0).powi(2) + (g.y - r.1).powi(2)).sqrt() < 10.0, "{photo} {label} joint {j} far from reference");
            }
        }
    }
}
