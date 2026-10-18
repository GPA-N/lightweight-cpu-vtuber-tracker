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

## Open

- A live run with the camera has not been done (needs the user at the camera).
- No per-joint confidence from the model: a confidence proxy (for example the hand model's score for the wrist) was not added.