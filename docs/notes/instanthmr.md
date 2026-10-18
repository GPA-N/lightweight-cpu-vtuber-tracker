# InstantHMR on this CPU (2026-10-19)

Repository: https://github.com/mohamdev/InstantHMR (code Apache-2.0). Weights: https://huggingface.co/momolesang/InstantHMR (`instanthmr.onnx`, 80,961,622 bytes, SHA-256 F717558094F57D7C9CD084D7981874DBCC3F08E7308501EB98C12844E2CCE3B7), license "sam-license" (a distillation of facebook/sam-3d-body-dinov3, trained on facebook/sam-3d-body-dataset labels). The license text was not read in full: the page that the repository points to returned 404 for the fetch tool. Read it before any public use.

Machine: AMD Ryzen 5 7430U (Zen 3: AVX2, no AVX-512, no VNNI), 6 cores, Windows 11.

## Model facts (from the repository code, via fetched excerpts)

- Inputs: `image` (N,3,224,224) RGB 0..1 with ImageNet mean/std; `cliff_cond` (N,3). Outputs: `mhr_params` (204), `shape_params` (45), `cam_trans` (3), `joints_2d` (70,2) in -1..1 of the crop, `joints_3d` (70,3) metres, body-centred, x right, y down, z away.
- Crop: square around the detector box, side = longest side x 1.2 (the demo first pads the detector box by 10 percent).
- `cliff_cond` without model metadata (this file has none): `[2*cx/w - 1, 2*cy/h - 1, max(bw,bh)/max(w,h)]` of the box in the full frame. The focal-length variant is used only when the model metadata or the caller asks for it.
- MHR70 joint order (instanthmr/skeleton.py): 0 nose, 1-2 eyes, 3-4 ears, 5-6 shoulders, 7-8 elbows, 9-10 hips, 11-12 knees, 13-14 ankles, 15-20 feet, 21-40 right hand (thumb, index, middle, ring, pinky: tip then three joints each), 41 right wrist, 42-61 left hand, 62 left wrist, 63-68 olecranon, cubital fossa, acromion, 69 neck. Checked on photos: shoulder width 0.37 to 0.40 m, forearm 0.20 to 0.24 m; a finger's `third_joint` is its knuckle (0.04 to 0.08 m from the wrist).

## Speed and memory (same recorded frames and photos, 4 to 6 threads)

| Run | Median | Peak working set |
| --- | --- | --- |
| Model only, ORT, 1 thread | 162 ms | |
| Model only, ORT, 4 / 6 threads | 58 / 58 ms | 348 MB (own process) |
| Model only, OpenVINO, 4 / 6 / 8 / 12 threads | 49.7 / 40.9 / 40.9 / 41.2 ms | 515 MB |
| Whole path (box, model, hand stage), ORT, 236 frames | 70.6 ms (p95 84.2) | 325 MB |
| Whole path, OpenVINO | 61.0 ms (p95 74.2) | 455 MB |

OpenVINO properties tried (full model, 6 threads, 40 runs): default 40.5 ms; joints only 40.9; CPU pinning 40.7; hyper-threading off 40.5; pinning and no hyper-threading 41.0; static input shape 38.4; 5 threads 41.9.

## Techniques tried

Five Haiku agents researched runtime options, quantization, temporal tricks, graph surgery and the repository (their reports were unverified in places and were checked by measuring). Results:

- Cutting the graph to `joints_2d` and `joints_3d` with `onnx.utils.extract_model`: 2461 to 2441 nodes, same speed.
- fp16 to fp32 initializers: identical outputs, same speed (ORT upcasts fp16 on CPUs without fp16 math).
- Dynamic int8 (`quantize_dynamic`): 97 ms, 2.3 cm mean error, 7.8 cm worst.
- Static int8 QDQ (`quantize_static`, per-channel, S8S8, MinMax, Conv/MatMul/Gemm, 104 calibration crops from 13 photos with crop jitter and flips): ORT 85 ms and 7.53 cm mean error (60.2 cm worst), 172 MB peak; OpenVINO 32.7 ms and 8.90 cm (62.4 cm worst). Not usable. The RepViT depthwise convolutions and the attention decoder are the likely cause; excluding them was not tried.
- Not tried: knowledge distillation to a smaller RepViT (M1.1 has about 43 percent fewer MACs but needs training), a lower input resolution (needs a re-export), NNCF accuracy-aware quantization (the Python 3.14 install of OpenVINO hung and the 3.12 environment has no NNCF), running every other frame with interpolation.

## Integration

`crates/infer/src/instant_hmr.rs` (crop, preprocessing, camera condition, decoding, `InstantHmr`), `crates/infer/src/engine.rs` (`run_two`: two named inputs on both engines), `crates/app/src/hmr_body.rs` (person box, joints to BlazePose-style landmarks, hand stage, `to_avatar_space`). `live --body` uses it when `models\upper_body\instanthmr_joints.onnx` exists (`scripts\fetch_instanthmr.ps1`), else BlazePose; `--limb instanthmr|blaze|cigpose` forces one. `APP_LIMB=instanthmr` runs it in the still-photo harness.

## Accuracy (added 2026-10-19, after the user doubted that the pipeline was just tuned for BlazePose)

The earlier consensus files (RTMPose, YOLO26 outputs) were deleted, so the reference here is CIGPose-m alone (2D, COCO-trained, the best model in limb-accuracy-consensus.md). Four photos (hands_15, 16, 17, 19), shoulders, elbows and wrists whose CIGPose score is at least 0.5 and which lie in the image, 22 joints; error is pixel distance over CIGPose's shoulder width. Test: cargo test --release -p app --bin live arm_probe -- --ignored --nocapture.

| Model and box | Shoulder | Elbow | Wrist | All |
| --- | --- | --- | --- | --- |
| BlazePose port | 0.056 | 0.097 | 0.054 | 0.070 |
| InstantHMR, whole-image box | 0.089 | 0.175 | 0.244 | 0.163 |
| InstantHMR, box from CIGPose joints (as the live path builds it after frame 1) | 0.074 | 0.182 | 0.201 | 0.148 |

A Python re-implementation of the preprocessing then swept the box (whole image, upper body, upper body with head room), padding (black, ImageNet-mean grey, edge), crop expansion (1.0, 1.2, 1.4, 1.6) and the camera condition (box, or fixed zeros): 72 settings. Range 0.100 to 0.200. Best: upper-body box, black padding, expansion 1.0 (shoulder 0.050, elbow 0.121, wrist 0.133, all 0.100). The default 1.2 with the upper-body box gave 0.113. So the box matters a lot (the whole-image box was 50 percent worse), which supports the doubt that my first six-photo render comparison, which used a poorer box, undersold the model; but the best setting is still behind BlazePose at the elbows and wrists. Caveats: 22 joints, one reference of another model family, MHR wrist and elbow definitions may differ from COCO's, and the settings were picked on the same photos they were scored on. Not done: change the crop expansion (1.0 was best here, 1.2 is the training value), refine the wrist with the hand model's wrist landmark, or score with human labels.

## Open

- A live run with the camera has not been done (needs the user at the camera).
- No per-joint confidence from the model: a confidence proxy (for example the hand model's score for the wrist) was not added.

## Wrist from the hand model, elbow tests, lead filter, removal of the old paths (2026-10-19)

- Wrist: where the hand model finds a hand, its wrist landmark replaces the body model's wrist in the image plane (depth stays the body model's; the metre shift comes from the shoulder width in pixels and in metres; a hand more than half a shoulder width from the body model's wrist is ignored). Probe on four photos against CIGPose-m: wrist 0.201 to 0.050, all joints 0.148 to 0.107 (BlazePose 0.070).
- Elbow following the wrist shift: share 0.25 gave elbow 0.187, 0.5 gave 0.204, 1.0 gave 0.244, against 0.182 with no follow. Not kept.
- Box shape: a box running down to the frame bottom (what a full-person detector draws for a cut-off person) gave 0.148, the same as the upper-body box. The official pipeline's preprocessing (read from the repository's inference.py: 1.2 times square crop, black constant border, bilinear resize, ImageNet normalisation, pixel-form camera condition when the model carries no cliff_focal metadata) matches this implementation.
- Lead filter: live run (60 s) gave body stage 86.7 ms median, 117.2 p95 (offline 75 ms: model 64 ms, hand stage 11 ms), 10 fps body, working set 639 MB median, 392 percent CPU, tracker 11.6 ms. The user found the arms better than BlazePose but laggier. Elbows and wrists are now extrapolated by the pipeline age times 0.6 (dead zone 0.08 m/s, at most 12 cm). Not yet run live.
- License: Meta's SAM License, 2025-11-19, from https://github.com/facebookresearch/sam-3d-body/blob/main/LICENSE (read as a tool summary): redistribution only under the agreement with a copy; no explicit commercial restriction; military, ITAR, espionage, weapons uses prohibited; no reverse engineering; acknowledge in published research; termination for breach or patent suits; California law.
- Removed from the code: BlazePose pose detector and landmark stage with heatmap refinement (infer::upper_body::UpperBody and helpers), CIGPose-m (infer::cigpose), the 2D-to-3D arm lifter (vatar::arm_lift) and their tests (cigpose_parity, engine_parity, person_box_golden, the arm accuracy probe). The hand stage stays in infer::upper_body. The accuracy numbers above remain as the record of the probe.

## Elbow re-placement (2026-10-19, rejected)

With the wrist corrected from the hand model, the elbow was re-placed so the upper arm and forearm kept their lengths (two-bone reach, closest to the old elbow). Probe against CIGPose-m on the four photos: elbow 0.219, against 0.182 with no correction (BlazePose 0.097). Not kept: the model's elbow is simply less accurate than BlazePose's, and moving it to fit a corrected wrist does not help.

## Pipeline parity with the official demo

The official demo (mohamdev/InstantHMR) finds people with an RF-DETR detector. Our box comes from the face landmarks instead, so the demo's detector stride is not needed.

- Crop: a square, 1.2 times the longest box side. We match the demo.
- `cliff_cond`: the pixel form, which we use. Our model has no cliff_focal metadata. The focal form (1.05 times the image diagonal) is not used.
- Joints: `joints_3d_cam` is `joints_3d + cam_trans` in the demo. `cam_trans` is not in our cut model. The body-centred joints are all the avatar needs.
- Smoothing: the demo's One Euro filter (live mode) is ported. `joints_2d` uses 1.0 Hz minimum cutoff and beta 0.01; `joints_3d` uses 1.0 Hz and beta 4.0. It runs on the 13 joints in `SLOTS`.
- Model: the only released model is 224 px, fp16, 77 MB. The 288 px B0 variant is an unpublished phone experiment of ours.

## Fingers

Probe (`cargo test --release -p app finger_probe -- --ignored --nocapture`): InstantHMR's 21 hand points (`hand_landmarks`) against the 2023 hand stage (`Mp2023Hands`, PROVISIONAL) on hands_15, 16, 17, 19, matched by side. Error is the mean pixel distance divided by the 2023 palm length (wrist to point 9), averaged per finger group over the matched hands.

| finger group | mean error / palm (hands 6) |
| --- | --- |
| wrist | 0.673 |
| thumb | 0.724 |
| index | 0.756 |
| middle | 0.657 |
| ring | 0.626 |
| pinky | 0.561 |

Hands found: InstantHMR 2, 1, 2, 1 and the 2023 stage 2, 1, 2, 1. The index finger is the worst group (0.756), and left and right are not swapped: same-side error is 0.46 to 1.03 palm lengths against 4.6 to 19.6 for the opposite side.
