# Live gate for the limb stages (task 11.7)

Stale 2026-10-19: describes removed `--limb` modes (cigpose, blaze); see README.md and docs/notes/instanthmr.md for the current state.

Date: 2026-10-18. Laptop: AMD Ryzen 5 7430U, plugged in, 640x480 camera at about 16 fps in the current light, Seed-san avatar, `dist/vtuber-tracker/live.exe --body --seconds 30`, one person at the desk. Reports in `dist/vtuber-tracker/out/live_{blaze,cigpose,cig_ov2}/live_report.txt`.

| Run | Tracked frames | Tracker median / p95 (ms) | Body stage median / p95 (ms) | Render fps | CPU (% of one core, median) | Peak working set (MB) |
|---|---|---|---|---|---|---|
| `--limb blaze` (ONNX Runtime) | 436 face-tracked | 10.1 / 13.0 | 31.2 / 37.2 | 30.0 | 231 | 491 |
| `--limb cigpose`, ONNX Runtime | 474 face-tracked | 11.0 / 14.0 | 34.7 / 40.3 | 30.0 | 269 | 523 |
| `--limb cigpose`, OpenVINO | 388 face-tracked | 10.9 / 13.8 | 33.3 / 42.7 | about 30 | 286 | 622 |

All three pass the three targets (median tracking latency under 50 ms, 30 fps render, working set under 1024 MB). One earlier OpenVINO run was discarded: the person had left the desk, so only 64 of 495 frames had a face and the latency figures were meaningless.

## Reading

- CIGPose-m costs about 3.5 ms more per body frame than BlazePose, 38 percent more CPU and 32 MB more memory. The body stage runs on its own thread at the camera rate, so the tracker latency stays at about 11 ms.
- OpenVINO gives no speed gain for CIGPose-m (33.3 against 34.7 ms) and adds about 100 MB and more CPU, as the earlier timing predicted. `live` therefore loads CIGPose-m on ONNX Runtime unless `VT_ENGINE=openvino`. OpenVINO only pays off for the face, pose-detector and hand models (2 to 3 times faster in `docs/notes/engine-abstraction.md`), and those are not wired to the engine layer yet: that is the next worthwhile speed task.
- The 22 to 25 ms estimate for the body stage was wrong: the measured body stage is 31 to 35 ms because CIGPose-m is about 17 ms per call with the crop and decode, and the MediaPipe palm detector and hand model add the rest.
- Revert rule (design.md D10): the budget passes and the four photos were not worse, so CIGPose-m is the default (`--limb cigpose`); `--limb blaze` stays available.

## Caveats

- Arm quality in the live run was not judged here; no one reviewed the avatar on screen. The photo comparison (`limb-photo-rerun.md`) is the only visual evidence.
- Bundle bug found and fixed on the way: the OpenVINO DLLs next to `live.exe` were not found (the loader reads PATH only); `engine.rs` now puts the executable folder on PATH when `openvino_c.dll` is there.
- Peak private bytes with OpenVINO are 1043 MB (914 MB without); the gate uses the working set.

## Decision change (2026-10-18, after the user's report)

The user could not get arm pulled-backwards tracking and would have scrapped a 2D model had it been flagged. CIGPose-m is 2D only (this was recorded as an accepted cost in design.md D10 but not put to the user as a blocker). live now defaults to `--limb blaze` (BlazePose, real 3D world landmarks and visibility); `--limb cigpose` stays as an opt-in 2D path with the hand-size depth cue. A late-2026 3D replacement was searched: RTMW3D exists (RTMPose family, PyTorch checkpoints; ONNX only in community or browser builds, large x model, no CPU timing found) and PriorFormer (arXiv 2508.18238, 2025) is a 2D to 3D lifter of about 1.8 ms on CPU with no ONNX or licence confirmed. Neither is a drop-in; no small permissive 2026 3D arm model was found.

## RTMW3D-x laptop timing (task 11.10, 2026-10-18)

Model: Soykaf/RTMW3D-x on Hugging Face (the file rtmlib's Wholebody3d downloads), rtmw3d-x_8xb64_cocktail14-384x288, weights dated 2024-06-26, 352 MB. Input [1,3,384,288], outputs [1,133,576], [1,133,768], [1,133,576] (SimCC x, y, z). ONNX Runtime, 4 threads, random input, 12 runs (fixtures/private/out/rtmw3d_time.py): median 223.2 ms, p95 225.4 ms. That is about 7 times the 30 ms body budget for the pose network alone, with a person detector still to add, and it is not 2026 weights. Rejected. The smaller RTMW3D sizes are PyTorch checkpoints only (no ONNX found). Downloaded file deleted after the test.

### RTMW3D-x int8 quantization (task 11.10, 2026-10-18)

One attempt, on Kaggle (kernel gpannn/vtuber-rtmw3d-quant, fixtures/private/kaggle/rtmw3d_quant/run.py): ONNX Runtime dynamic int8 (QInt8) of the Soykaf/RTMW3D-x file, two variants. Laptop timing, 4 threads, 8 runs, random input (rtmw3d_time.py): MatMul and Gemm only, 349.5 MB: median 225.2 ms (fp32 was 223.2 ms, no gain). All ops including Conv, 88.6 MB: median 3723 ms (17 times slower: the CPU has no int8 convolution kernels worth using on Zen 3). Static QDQ int8 was not tried; without VNNI the expected gain is small and the model would still be several times over the 30 ms budget. Quantization does not rescue RTMW3D-x. Downloaded files deleted.

### RTMW3D-l (task 11.10, 2026-10-18)

The l size has only PyTorch checkpoints (rbarac/rtmpose3d, rtmw3d-l_8xb64_cocktail14-384x288-794dbc78_20240626.pth, 2024 weights, Apache-2.0 tag). Exported to ONNX on a private Kaggle CPU kernel (fixtures/private/kaggle/rtmw3d_l_export/run.py, kernel gpannn/vtuber-rtmw3d-l-export, mmpose 1.3.2 plus mmcv-lite with stubs for the unused mmdet and mmcv ops imports); torch safe loading was kept (weights_only=True with a small allow-list of numpy types) after turning it off was blocked. Result: 230 MB ONNX, input [1,3,384,288], outputs SimCC x [1,133,576], y [1,133,768], z [1,133,576], max difference from PyTorch 3e-6. Laptop timing (ONNX Runtime, random input, 12 runs, rtmw3d_time.py): 4 threads median 122.5 ms (p95 125.5), 6 threads median 112.5 ms (p95 132.1). That is about 4 times the 30 ms body budget before a person detector, and OpenVINO and int8 gave nothing on the x model, so they are not expected to help here. It has real z (SimCC depth relative to a root point) and its accuracy is close to x, but it cannot run at 30 fps next to the face tracker on this CPU. Rejected for the live path. It could suit an offline or high-end-CPU mode; ONNX kept at fixtures/private/models_dl/rtmw3d_l/rtmw3d-l.onnx.

## BlazePose live take with arm pulled back (2026-10-18)

40 s, `live --body` (BlazePose default, heatmap refinement on), user pulled an arm back several times; per-frame log from `VT_LIMB_LOG=1` (fixtures/private/out/blazelog_summary.py). Gates pass: tracker median 10.6 ms, render 30.3 fps, working set 512 MB, body stage 36.3 / 44.4 ms. Wrist depth against the shoulder (metres, + toward the camera), right arm: median +0.30, typical reach +0.35 to +0.49, and about five clear dips to 0.0 to 0.1 (hand back level with the shoulder plane) matching the pulls; values below -0.1 on 10 frames only (min -0.22). So BlazePose follows the pull back to the shoulder plane; it rarely reports the hand behind the shoulders, so a pull far behind the body is probably under-reported. No gain was applied (no ground truth for depth). Not reviewed: whether the avatar's arm visibly followed on screen.

## Round of fixes from the user's live feedback (2026-10-18)

Reports: wrist did not follow (no high five), the avatar fell back to the T pose when tracking was lost, arms jittered, eyes did not move. Changes: (1) hand direction and palm roll now come from the 21-point hand model when it found the hand (crates/avatar/src/body.rs solve_arm; the pose-model index and pinky points stay as fallback); on the photos Twist now shows an open hand with palm to the camera for hands_16 and hands_19. (2) A bone without a target keeps its last pose until a target returns; the 0.5 s hold and 1 s ease to rest were removed (advance_body). (3) Per-bone One Euro filter on the solved rotations (window.rs QuatSmoother, min cutoff 1.2 Hz, beta 8): a unit test shows 3 degrees of noise cut to under 40 percent travel while a 60 degree move is followed within a second. (4) Eyes: eyeLook* blendshapes are now mapped to lookLeft, lookRight, lookUp and lookDown (the old rule forbidding gaze entries was removed), which drives Seed-san (lookAt type expression); for Twist-type models (lookAt type bone) crates/avatar/src/gaze.rs moves a look target in front of the face and bevy_vrm1 LookAt turns the eye bones. Tests green, clippy only the 2 old bench warnings. A last live run of 40 s on Twist did not crash, but the person was away so no face was tracked (0 ms latency rows): arms, jitter and gaze have not been judged live yet. Tuning knobs: BODY_FILTER_* in window.rs, GAZE_SPAN_* in gaze.rs, weights 0.7 in data/blendshape_map.json.

## Live screenshots with the debug overlay (2026-10-18)

AVATAR_SHOT=path makes live.exe save the window and exit. Seed-san shot: overlay shows shoulder line, neck line, ear line, orange right arm, lime hand wireframes; the user sat too close, so the face was cut off and no eye outline was drawn. Twist shot (about 16 s run, tracker 10.7 ms, 30.2 fps, 502 MB): eye outlines, irises and gaze pointers drawn; the raised arm with the hand behind the head came out as a raised arm with the hand near the top of the head; the lowered hand sat in front of the chest. Single frames only.
