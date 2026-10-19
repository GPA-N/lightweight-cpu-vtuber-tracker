# Upper-body tracking: verification pass and synthesis

Stale 2026-10-19: describes removed `--limb` modes and the CIGPose-m choice; see README.md and docs/notes/instanthmr.md for the current state.

Date: 2026-10-17. Research only; no builds, no model downloads, nothing executed. This note ties together the three researcher notes (`ub-models.md`, `ub-retarget.md`, `ub-prior-art.md`) and records what was re-checked. The decisions are in `openspec/changes/add-upper-body-tracking/design.md`.

## Spot checks of the researcher notes

| Claim in note | Check (2026-10-17) | Result |
|---|---|---|
| ub-models: RTMPose-m 90+ FPS on i7-11700, 75.8 AP | mmpose `projects/rtmpose/README.md` line 20 | Correct, but it is the body-only model |
| ub-models: RTMW-m 58.2, RTMW-l 66.0 / 70.1 whole AP | same README, table at lines 317-320 | Correct. The table has no CPU latency for RTMW (`-`); RTMW-m is 4.3 GFLOPs |
| ub-models: Qualcomm MediaPipe-Pose has pose_detector 3.14 MB and landmark 12.9 MB ONNX | Hugging Face API: repo lists only `.gitattributes`, `LICENSE`, `README.md`, `release_assets.json` | Not verified as files in the repo. Latencies are NPU. Do not rely on it |
| ub-models: no hand ONNX found on Hugging Face ("search returned zero") | HF search | Wrong. `opencv/palm_detection_mediapipe` and `opencv/handpose_estimation_mediapipe` (fp32 3.9 MB and 4.1 MB, Apache-2.0, updated 2025-06-20) exist; `webnn/mediapipe_hand` has TFLite files (updated 2026-04-07) |
| ub-models: no MediaPipe pose ONNX that ort can load | HF API | Wrong. `fernandotonon/QtMeshEditor-blazepose-onnx` (11.9 MB) and `QtMeshEditor-poselandmarks-onnx` (12.8 MB), Apache-2.0, updated 2026-07-13, give 33 landmarks plus world landmarks, visibility and presence |
| ub-prior-art: google-ai-edge/mediapipe pushed 2026-10-16 | `gh api` | Correct (2026-10-16T20:57:35Z, Apache-2.0) |
| ub-prior-art: kalidokit pushed 2025-08-18, deprecated | `gh api` | Push date correct; `archived` is false; the deprecation notice was not re-read |
| ub-prior-art: DarkStar1997/vtubing, MIT, three MediaPipe models on CPU | `gh api` | Repo exists, MIT, pushed 2026-10-01; README not re-read |
| ub-retarget: bevy_vrm1 `body_tracking.rs` role unknown | Read the vendored file | Gaze-driven (`LookAt` target), not a body-data source. Per-bone `<Bone>BoneEntity` components and `RestTransform` / `RestGlobalTransform` are the right API |
| ub-retarget: sway sketch accumulates translation | Self-flagged | Confirmed; design stores rest translation |

## Model facts used by the design

- Pose: `fernandotonon/QtMeshEditor-blazepose-onnx` `pose_detector.onnx` 11,926,470 B (input 224x224 in [-1,1], outputs [1,2254,12] and [1,2254,1]) and `QtMeshEditor-poselandmarks-onnx` `pose_landmarks.onnx` 12,766,212 B (input [1,256,256,3] in [0,1]; outputs [1,195] 39x(x,y,z,visibility,presence), [1,1] presence, [1,256,256,1] segmentation, [1,64,64,39] heatmap, [1,117] 39x(x,y,z) world landmarks, hip-centred). Apache-2.0 on the Hub. Sources: https://huggingface.co/fernandotonon/QtMeshEditor-poselandmarks-onnx and https://huggingface.co/fernandotonon/QtMeshEditor-blazepose-onnx.
- Hands: https://huggingface.co/opencv/palm_detection_mediapipe (`palm_detection_mediapipe_2023feb.onnx` 3,905,734 B) and https://huggingface.co/opencv/handpose_estimation_mediapipe (`handpose_estimation_mediapipe_2023feb.onnx` 4,099,621 B); card says the int8 variants may produce invalid results.
- Alternative: https://huggingface.co/bukuroo/RTMW-ONNX `rtmw-m-256.onnx` 62,376,212 B (over the 50 MB cap), updated 2025-09-03.
- No CPU latency figure was found for any of these files. The budget table in the design is an estimate, replaced by task 1 of the change.

## 2026 date table

| Item | Date | Source |
|---|---|---|
| google-ai-edge/mediapipe | pushed 2026-10-16 | `gh api` |
| pykeio/ort | pushed 2026-10-17 | `gh api` |
| not-elm/bevy_vrm1 | pushed 2026-10-03 | `gh api` |
| Tau-J/rtmlib | pushed 2026-08-03 | `gh api` |
| fernandotonon pose ONNX repos | updated 2026-07-13 | HF API |
| qualcomm/MediaPipe-Pose-Estimation | updated 2026-10-07 (no ONNX files) | HF API |
| opencv hand ONNX repos | updated 2025-06-20 (exception) | HF API |
| open-mmlab/mmpose | pushed 2025-08-04 (stale) | `gh api` |
| yeemachine/kalidokit | pushed 2025-08-18 (stale) | `gh api` |

## Open items

- CPU latency of pose landmarks, hand landmarks and detectors on the reference laptop (task 1).
- Pose stability with hips out of frame (task 1.4).
- A 2026-dated ONNX hand model (task 1.5).
- Whether the converted graphs' licence claims hold; same status as the face models in `licenses.md`.

## Limb model currency (task 11.9, 2026-10-18)

| Item | Date | Licence | Note |
|---|---|---|---|
| CIGPose-m COCO 256x192 | ONNX release 2026-03-16 (namas191297/cigpose-onnx) | Apache-2.0 (wheel header) | Chosen limb model, design.md D10. 2D only. |
| OpenVINO | 2026.4.1 | Apache-2.0 | CPU engine, ORT fallback. |
| MediaPipe hand (palm_detection and handpose, opencv_zoo 2023-feb) | 2023 | Apache-2.0 | Stated exception: no small CPU-fast commercially usable 2026 hand model was found (hand-search-A/B/C.md, shootout-accuracy.md). Pasted claim that the MediaPipe Tasks `HandLandmarker` bundle is newer was not verified; the Tasks bundle is believed to hold the same hand models. |
| BlazePose ONNX port | 2023 | Apache-2.0 | Fallback limb path (`--limb blaze`). |
