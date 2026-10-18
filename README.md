# Lightweight CPU VTuber Tracker

*Efficient, optimized, lightweight webcam face, body and hand tracking for VRM avatars.*

**Contents:**
[Overview](#overview) |
[Performance and profiling](#performance-and-profiling) |
[Accuracy](#accuracy) |
[Models](#models) |
[How it works](#how-it-works) |
[Using it](#using-it) |
[Build](#build-and-test) |
[Design history](#design-history) |
[Limitations and roadmap](#limitations-and-roadmap) |
[License](#license)

Keywords: VTuber, VRM, face tracking, body tracking, hand tracking, motion capture, MediaPipe, ONNX Runtime, Bevy, Rust, CPU-only, webcam.

## Overview

A native Windows program that reads a webcam, tracks the face, upper body and hands on the CPU, and moves a VRM 1.0 avatar in a Bevy window. There is no browser, no Python at run time and no GPU needed for inference.

What it tracks:

| Part | What drives the avatar |
| --- | --- |
| Head | Head turn from a 478-point face mesh fitted to a canonical face model |
| Face | 52 ARKit blendshapes: blinks, squint, mouth shapes, smile, eyebrows, cheeks |
| Eyes | Eye gaze, either as VRM look expressions or as a look target for bone-type eyes |
| Body | Shoulders, upper arms and forearms from 3D pose landmarks, with a two-bone reach solver so hands reach the head |
| Torso | Roll from the shoulder line, twist from shoulder depth, lean from the head over the shoulders |
| Hands | Wrist direction and roll, finger curl, and a thumb that follows the tracked thumb segments |

Other features:

- **Per-joint confidence.** A joint moves the avatar only when its confidence passes the threshold (80 percent by default) and it lies inside the camera picture. A part that is not seen keeps its last pose. The avatar never falls back to a T pose.
- **Focus mode.** One key hides the camera preview and the debug drawing and stops the tracker threads building them, so a recording or stream only pays for the avatar render.
- **Settings panel** with basic and advanced options, saved to `settings.json`.
- **Model loading** by file dialog or drag and drop, with warnings when a model lacks humanoid bones, expressions or morph targets that the tracker would drive.
- **Debug drawing** on the camera preview: skeleton lines, hand wireframes, eye outlines, irises and gaze pointers.
- **Lighting presets** (Studio, Soft, Flat, Dramatic) with shadows off, so lighting costs no shadow pass.

The project targets, set at the start and used as pass or fail gates everywhere below: median tracking latency under 50 ms, at least 30 rendered frames per second, and peak working set at most 1024 MB.

## Performance and profiling

All numbers are from one laptop: **AMD Ryzen 5 7430U (6 cores, 12 threads), 22 GB RAM, Windows 11, integrated Radeon graphics, plugged in.** They are single runs unless a spread is shown. Run-to-run noise on this laptop is 10 to 40 percent (room light changes the camera frame rate, a browser open adds CPU), so compare numbers only from the same session.

### Targets against measured results

| Target | Limit | Measured (live program, face and body on) | Verdict |
| --- | --- | --- | --- |
| Median tracking latency | under 50 ms | 10.0 to 11.5 ms | pass |
| Rendered frame rate | at least 30 fps | 30.0 to 33 fps (render is capped at 30 on purpose) | pass |
| Peak working set | at most 1024 MB | 468 to 622 MB depending on engine and limb model | pass |

### Live runs of the whole program

Source: `live --body` for 30 s with one person at the desk, Seed-san avatar (`docs/notes/live-limb-gate.md`, 2026-10-18).

| Run | Tracker median / p95 (ms) | Body stage median / p95 (ms) | Render fps | CPU (% of one core, median) | Peak working set (MB) |
| --- | --- | --- | --- | --- | --- |
| BlazePose limbs, ONNX Runtime (default) | 10.1 / 13.0 | 31.2 / 37.2 | 30.0 | 231 | 491 |
| CIGPose-m limbs, ONNX Runtime | 11.0 / 14.0 | 34.7 / 40.3 | 30.0 | 269 | 523 |
| CIGPose-m limbs, OpenVINO | 10.9 / 13.8 | 33.3 / 42.7 | about 30 | 286 | 622 |

Notes on reading this table:

- CPU is a percentage of one core, so 231 means about 2.3 cores busy. It includes camera conversion, both tracker threads and rendering.
- The body stage runs on its own thread at the camera rate, so its 31 to 35 ms does not add to the 10 to 11 ms face latency. Both stay under the 50 ms target.
- The latest live runs (2026-10-18, with the new viewer UI: toasts, lighting presets, settings panel, 1280x720 camera, 80 percent confidence): 30 fps render, tracking latency 9.4 to 11.5 ms, 477 to 579 MB working set, 133 to 231 percent CPU of one core. The camera delivered 10 fps at that size. These are single runs; the UI added no measurable cost within the noise, but its own cost has not been isolated.
- A later 40 s live take with an arm pulled back several times gave tracker median 10.6 ms, 30.3 fps, 512 MB working set, and body stage 36.3 / 44.4 ms median / p95.
- A run where the person left the desk was discarded: with no face the latency rows are meaningless (they show 0.0 ms).

Camera size changes the numbers. At 640x480 the camera delivered 15 to 16 fps with latency 9.7 to 11.9 ms, about 205 percent CPU and 468 to 483 MB. At 1280x720 on the same laptop the camera delivered only 10 fps, with latency about 10 ms, 130 to 230 percent CPU and 480 to 580 MB (single runs, 2026-10-18). Inference cost barely moves because the models run on fixed-size crops; the camera frame rate is what drops. The default is set in the settings (`camera_width`, `camera_height`) and `--camera-size WxH` overrides it.

### Per-model inference time

Median milliseconds per model call at 4 threads, random input in each model's own shape, 60 timed runs after 15 warm-up runs (`docs/notes/engine-comparison.md`, Python harness, so only the ratios carry over to Rust):

| Model | ONNX Runtime | OpenVINO 2026.4.1 | OpenCV 5.0 DNN |
| --- | --- | --- | --- |
| face_detector | 1.17 | 0.53 | 1.75 |
| face_landmarks | 5.73 | 2.19 | 4.37 |
| face_blendshapes | 0.93 | 0.67 | 2.33 |
| pose_detector | 10.09 | 4.68 | 19.79 |
| pose_landmarks | 11.27 | 6.30 | 24.04 |
| palm_detection | 7.38 | 3.96 | 10.47 |
| handpose | 3.22 | 2.88 | 8.26 |

What this means:

- OpenVINO is 1.2 to 2.6 times faster than ONNX Runtime on every model at 5 threads, with the same outputs (largest difference 4.9e-4 on outputs of magnitude 15 to 207). It costs about 120 MB more memory and about 55 MB more files. OpenCV DNN is slower and bigger, so it was rejected.
- The OpenVINO engine layer exists (`crates/infer/src/engine.rs`) and is used for CIGPose-m, where it gave no gain (33.3 against 34.7 ms) and added about 100 MB, so ONNX Runtime stays the default. **Wiring OpenVINO to the face, pose-detector and hand models is the biggest open speed task**; the estimate is a full tracker around 18 ms instead of 39 ms per frame.

### Body stage cost

From the upper-body benchmark (`docs/notes/ub-benchmark.md`, 236 recorded frames, 4 threads unless noted, ONNX Runtime):

| Config | Median (ms) | p95 (ms) | Peak working set (MB) |
| --- | --- | --- | --- |
| Pose only, 1 / 2 / 4 threads | 28.3 / 19.6 / 16.6 | 30.2 / 22.3 / 18.5 | 73 |
| One hand only, 1 / 2 / 4 threads | 5.2 / 4.0 / 3.6 | 5.5 / 5.2 / 4.4 | 45 |
| Pose, hand and face on one thread, 4 threads | 33.7 to 43.1 | 42.4 to 49.5 | 95 |

That serial total missed the 30 fps gate, which is why the body stages run on their own thread. Peak working set of the tracking models alone is about 95 MB; the rest of the 470 to 620 MB process is the renderer.

### Where the CPU and memory go

Measured while looking for savings (`openspec/changes/reduce-live-cpu-and-memory`, `evaluate-lean-renderer`):

| Setup | CPU (% of one core) | Working set (MB) |
| --- | --- | --- |
| Camera and tracker only, no window | 13 | 55 |
| Bare wgpu triangle, 60 fps / 30 fps cap | 9.9 / 6.0 | 123 |
| Bare Bevy 0.19 window (camera and light) | 44.7 | 273 |
| Bare Bevy window with a 33 ms frame cap | 25.2 | slightly lower |
| Bare Bevy window with cap, smaller task pools and no pipelined rendering | 20.0 | slightly lower |
| Full app, tracker running, render uncapped (60 fps) | 113 average | 466 |

Findings that shaped the code:

- About 35 points of the bare Bevy window's CPU are Bevy overhead above the wgpu floor, and the **33 ms frame cap removes most of it**. That is why rendering is capped at about 30 fps: the avatar only changes at camera rate.
- About 290 MB of private memory is a floor under any wgpu-based renderer; Bevy adds about 120 MB. A lighter renderer (kiss3d was trialled) could save at most that, and would need a rewrite of VRM loading. Bevy was kept.
- ONNX Runtime thread spinning is turned off: no latency cost, less CPU when saturated (saturated bench 233 to 202 percent).
- Tried and reverted: Bevy's low-power reactive mode (25 ms) raised hand-off latency by 2 to 4 ms and dropped rendering to 40 fps; a DXGI video-memory probe returned nothing.
- **Focus mode saves the preview build and upload, the debug drawing and the preview panel.** Its CPU and RAM saving has not been measured yet, so no figure is claimed (task 2.2 and 8.2 in the viewer UI change).

### Profiling and measuring it yourself

- The live program prints one line per second: camera fps, tracking fps, render fps, latency (ms), CPU (% of one core) and working set (MB). It writes a report with median, p95, p99, max and mean for every stage to `<out>/live_report.txt` (`--out DIR`).
- `VT_LIMB_LOG=1` logs the wrist depth against the shoulder per frame; `VT_ENGINE=openvino` selects OpenVINO for the CIGPose path; `VT_NO_HEATMAP=1` switches off the BlazePose heatmap refinement for A/B checks.
- `bench.exe --body-bench CONFIG THREADS` times pose, hand and full pipelines on recorded frames. `BENCH_REPEAT`, `BENCH_PACE_MS` (paced, 15 fps style runs) and `BENCH_RECORD` control it. `DUMP_LM=1` dumps raw landmarks for accuracy checks.
- `AVATAR_SHOT=path` makes the window save a screenshot and exit; `APP_STILL_BODY=photo` runs the face, body and hand stages on a single photo with no camera, and `scripts/photo_to_avatar.ps1` wraps it.
- The `timing` crate samples process CPU, working set, private bytes and DXGI video memory through the Windows counters.

## Accuracy

There is no ground-truth labelled data for these photos, so every accuracy figure below is a disagreement with a reference model, not an error against the truth. They were measured to choose models, not to claim a score.

**Hands** (7 images, 9 reference hands; normalised mean point distance NME21 against official MediaPipe, lower is better; `docs/notes/shootout-accuracy.md`):

| Candidate | Hands found | NME21 | Median ms (Kaggle CPU kernel, 1 thread) | Verdict |
| --- | --- | --- | --- | --- |
| MediaPipe-weights ONNX port, palm 0.3 | 9 / 9 | 0.033 | 40 | chosen family |
| opencv_zoo 2023-02 weights, our Rust port | 9 / 9 | 0.066 | 35 | in use (smallest, 8 MB) |
| RTMPose-m hand (2023) | 9 / 9 | 0.055 to 0.070 | 109 to 112 | rejected, 3x slower |
| YOLO26s hand pose (2026, AGPL) | 6 / 9 | 0.099 | 418 | rejected, slow and misses fists |

**Arms**, four photos, normalised by shoulder width against a consensus of the other model families: CIGPose-m 0.043, RTMPose-m 0.045, YOLO26s and m 0.046, RTMPose-s 0.052, official MediaPipe 0.066, YOLO26n 0.085, our BlazePose port 0.094. This check favours COCO-style models; against official MediaPipe the port gap was median 0.055 before the heatmap refinement and 0.026 after (mean 0.065 to 0.032).

**Face and photos.** Face blendshapes come from the MediaPipe blendshape model, so they match its output by construction. Still-photo checks of the avatar (open palm, peace sign, hands at the temples, a smile) are done with `scripts/photo_to_avatar.ps1`; the solved arm angles equal the tracked angles to 0.0 degrees on the photos, so remaining errors are in the tracked points, not the solver. Two heuristics came out of those photos: a strong smile cancels part of the brow-down reading (the face model reads a smile's cheek raise as lowered brows, which made smiling faces look angry), and squint adds to the blink.

### Photo tests of the whole pipeline

Seven more still photos (on top of the four hand photos above) were run through the face, body and hand stages and rendered on an avatar (no camera involved): `APP_STILL_BODY=photo.png` with `scripts/photo_to_avatar.ps1`, release build, 2026-10-18. The photos are third-party stock images and are not included in this repository, and neither are the avatar renders, because the sample avatar's license forbids redistribution. The table gives what the program measured; the last column is a reading of the render by eye, not a score.

| Photo (what it shows) | Hands found | Arm angles tracked (upper arm / forearm elevation, elbow bend) | Strong face readings | Render by eye |
| --- | --- | --- | --- | --- |
| Hands at the temples, face scrunched, mouth open | 2 of 2 (scores 0.93, 0.95) | left -47 / 44 / 99, right -49 / 51 / 103 | brows down 0.88, squint 0.84, blink 0.6, jaw open 0.32 | hands at the cheeks rather than the temples; eyes shut; mouth open |
| Peace sign, smiling | 1 of 1 (0.996) | raised arm -56 / 32 / 98; the other arm hangs out of frame and holds its pose | smile 0.92, squint 0.70 | two fingers up; smile reads as happy after the smile fix |
| Peace sign held near the face | 1 of 1 (0.985) | raised arm -50 / 39 / 99 | eye gaze and mouth press | two fingers up near the face |
| Thumbs up, hand at the chest | 1 of 1 (0.996) | raised arm -59 / 8 / 89; the other wrist is outside the picture and dropped | near neutral | thumb up; second arm held at rest |
| Both hands up, seated, black and white | 1 of 2 (score 0.993; the other hand is cut by the picture edge and not found) | left 0 / 72 / 74, right -11 / 47 / 81 (both arms driven after the edge-margin fix) | squint, frown, mouth press | both arms up; one open palm with fingers, the other hand left at its rest shape |
| Both hands up as claws, screaming | 2 of 2 (0.995, 0.996) | left -49 / 32 / 83, right -52 / 27 / 80 | brows down 0.6 to 0.7, jaw open 0.47, lower lip down 0.56 | both hands curled; hands sit lower than in the photo; eyes shut, mouth open |
| Fists raised at the sides, shouting | 2 of 2 (0.96, 0.98) | left -72 / -32 / 48, right -78 / -43 / 37 | jaw open 0.75, upper lip up 0.63, squint 0.63 | fists closed, arms bent at the sides |

What the photo tests showed, and what was changed because of them:

- On every photo the solved arm angles equal the tracked angles to 0.0 degrees, so the remaining errors are in the tracked points and the avatar's proportions, not in the solver. Hands tend to land a little lower than the photo because the avatar's arms are shorter than the person's.
- A joint outside the picture is dropped even when the model reports it as visible (thumbs-up: a wrist 13 percent below the frame). A margin of 4 percent of the picture size is allowed, because on the seated photo a real wrist was estimated 1.7 percent past the edge and was wrongly dropped by the first, stricter version.
- The face model read a smile's cheek raise as lowered brows, so a smiling face looked angry. A strong smile now cancels part of the brow-down reading, and smile also drives the VRM `happy` expression.
- Squint was not mapped to anything and the eyes only half closed; it now adds to the blink. The face model reports an open, shouting mouth at only 0.3 to 0.75 jaw open, so lower-lip drop adds to mouth open.
- The sample avatar (Seed-san) carries a separate robotic arm that does not follow the humanoid arm bones, so one arm in its renders looks wrong. This is the avatar asset, not the tracker.

**Known accuracy gaps:** an arm pulled far behind the body is under-reported (BlazePose depth reaches about 0 to 0.1 m behind the shoulder plane and rarely goes below); hand placement on very blurred frames is poor; the pose model reports hip visibility near 1.0 even when the hips are out of frame, so hips are never used.

## Models

All models run as ONNX on the CPU. They are not stored in this repository; fetch them and verify them against `models/SHA256SUMS`. Each has its own license; see `docs/notes/licenses.md` for the evidence behind every row.

| Role | Model | Size | License | Weights date |
| --- | --- | --- | --- | --- |
| Face detector | MediaPipe BlazeFace short range (ONNX port) | 0.4 MB | Apache-2.0 | 2023 family |
| Face landmarks | MediaPipe Face Mesh V2, 478 points | 4.7 MB | Apache-2.0 | 2023 family |
| Blendshapes | MediaPipe blendshape MLP, 52 outputs | 1.0 MB | Apache-2.0 | 2023 family |
| Pose detector and landmarks | MediaPipe BlazePose full, 33 points with 3D world coordinates and visibility (ONNX port) | 11.9 MB and 12.8 MB | Apache-2.0 | 2023 weights, port updated 2026-07 |
| Palm detector and hand landmarks | MediaPipe hand, opencv_zoo 2023-02 export | 3.9 MB and 4.1 MB | Apache-2.0 | 2023-02 |
| Optional 2D limb model | CIGPose-m COCO, 256x192 | 53.7 MB | Apache-2.0 | 2026-03 |

**Currency exception.** The goal was models current to late 2026. The pose and hand weights are 2023: no small, CPU-fast, permissively licensed 2026 model with 3D arms or hands was found after a search (`docs/notes/hand-models-2026.md`, `docs/notes/upper-body-research.md`). The only 2026 limb model that fits the budget, CIGPose-m, is 2D only, so it is an opt-in `--limb cigpose` path.

**Candidates tried and rejected**, with the measurement that decided each:

| Candidate | Why it was rejected |
| --- | --- |
| RTMW3D-x (3D whole body, 2024) | 223 ms per call on this CPU, 352 MB, about 7 times the body budget; int8 dynamic quantization gave no gain (MatMul-only 225 ms) or was 17 times slower (all ops, 3.7 s) |
| RTMW3D-l (3D whole body, 2024) | 122 ms at 4 threads (113 ms at 6), 230 MB, about 4 times the body budget before a person detector |
| YOLO26 pose (2026) | 35 to 60 ms per frame, 2D, AGPL license |
| CIGPose-m as the default | 2D only, so an arm pulled behind the body could not be followed even with a hand-size depth cue; kept as `--limb cigpose` |
| OpenCV 5 DNN | Slower than ONNX Runtime on 6 of 7 models, and its Rust bindings need a system OpenCV |
| PriorFormer-class 2D to 3D lifter | About 1.8 ms on CPU but no ONNX export or license confirmed; untested |

## How it works

```
camera thread --> newest-frame slot --+--> face thread: detect (every 5th frame) -> 478 landmarks
 (nokhwa, one                          |      -> blendshapes -> head fit -> One Euro filters --> avatar
  slot, drops stale frames)            |
                                       +--> body thread: pose (3D) -> hands -> confidence gate
                                                -> avatar-space pose --------------------------> avatar
```

- **Capture.** One slot holds only the newest frame, so a slow stage drops old frames and latency stays bounded.
- **Face thread.** The detector runs every fifth frame or when presence drops; between detections the crop follows the previous landmarks. Head rotation comes from fitting the 478 points to a canonical face model.
- **Body thread.** BlazePose gives 3D world landmarks with visibility; the landmark heatmap is used to refine the points the way MediaPipe does. A hand crop is derived from the pose wrist, and the 21-point hand model gives finger curl and wrist roll.
- **Retarget (`crates/avatar/src/body.rs`).** A top-down solver works with world directions and each model's own rest pose, so it works on any VRM humanoid. A two-bone reach solve puts the wrist where the tracked wrist is, scaled to the avatar's arm length. Spine, chest and upper chest take roll, twist and lean. The thumb's first two bones are aimed along the tracked thumb segments because a thumb swings across the palm instead of curling in the finger plane.
- **Head and torso do not fight.** The face tracker reports the head's absolute turn. The head bone's local rotation subtracts how far its parent has turned (the torso twist), so a twisting body is not counted twice.
- **Confidence per joint.** Visibility below the threshold, a position outside the picture, or a whole-pose presence below 0.5 marks a joint as missing. Shoulders alone still drive the torso while unseen arms hold their last pose.
- **Smoothing.** One Euro filters on the landmarks, then one per solved bone (minimum cutoff 1.2 Hz, beta 8), then a render-rate follow with a 15 degree per-frame step cap. A bone with no target keeps its last pose.
- **Face extras.** Eye gaze maps to the VRM look expressions, or to a look target for bone-type eyes (expression-type models must not get a look-at, because `bevy_vrm1` would panic). Eyebrows and mouth shapes are written straight to a model's morph targets after the expression pass, which would otherwise reset them.
- **Crates:** `core` (frame and tracking types), `capture` (camera thread and slot), `infer` (ONNX Runtime and OpenVINO sessions, the trackers, person box, CIGPose stage), `pose` (head fit), `smoothing` (One Euro, lost-face and lost-body filters), `facelogic` (crop, detection scheduling), `avatar` (Bevy window, VRM, solver, settings UI), `app` (the `live` and `app` programs), `bench` and `timing` (measurement).

## Using it

### Run

```powershell
target\release\live.exe --body --vrm path\to\model.vrm
```

Sit back so your whole face and shoulders are in view. Only VRM 1.0 files load; VRM 0.x files are refused with a message. `live --help` lists every flag.

### Controls in the window

| Key | Action |
| --- | --- |
| S | Settings panel: Basic and Advanced tabs, Tab switches, Esc closes, Reset restores defaults |
| F | Focus mode on or off |
| L | Next lighting preset |
| H | Help overlay |
| Ctrl+O | Open a `.vrm` model with the file dialog |
| Drop a file | Load a `.vrm` model |

Messages (refused model, warnings, saved lighting name) appear as toasts at the bottom of the window and fade after four seconds.

### Settings

Saved as `settings.json` next to the program, loaded at start. A missing file gives the defaults; a broken file gives the defaults and a warning; every value is clamped to its range.

| Option | Where | Range or choices | Default | Applies |
| --- | --- | --- | --- | --- |
| Background | Basic | Dark, Grey, Green, Blue (chroma key), White | Dark | live |
| Lighting preset | Basic | Studio, Soft, Flat, Dramatic | Studio | live |
| Confidence (face, body and hand together) | Basic | 30 to 99 percent | 80 | live |
| Show camera preview | Basic | on or off | on | live |
| Show debug drawing | Basic | on or off | on | live |
| Face, body, hand confidence separately | Advanced | 30 to 99 percent each | 80 | live |
| Key, fill, ambient light scale | Advanced | 0.2 to 2.0 | 1.0 | live |
| Anti-aliasing | Advanced | Off, 2x, 4x | Off | live |
| Frame cap | Advanced | 15 to 120 fps | 30 | live |
| Eyebrow gain, gaze gain | Advanced | 0 to 2.0 | 1.0 | live |
| Camera width and height | Advanced | 320x240 to 3840x2160 | 1280x720 | next start |
| Mirror | Advanced | on or off | on | next start |

Settings that the tracker threads use (confidence, preview and debug drawing) are shared through atomics, so changing them never blocks tracking.

### Command line

```
live [--vrm PATH] [--map PATH] [--models DIR] [--camera INDEX] [--camera-size WxH]
     [--threads N] [--seconds N] [--out DIR] [--no-mirror]
     [--body] [--body-models DIR] [--limb blaze|cigpose] [--no-overlay] [--focus]
```

### Models loaded from the window

A model that loads but lacks parts the tracker drives gets a warning that names them, for example "missing humanoid bones, these parts will not move: leftEye, rightEye". Missing bones are skipped, missing expressions stay at their default, and a model with no morph targets cannot animate its face.

## Build and test

Requirements: Windows 11, a webcam, the Rust toolchain pinned in `rust-toolchain.toml`.

```powershell
# face models (about 6 MB), verified against models\SHA256SUMS
powershell -ExecutionPolicy Bypass -File scripts\fetch_models.ps1

# upper-body models go in models\upper_body\ ; sources and hashes: docs/notes/ub-models.md

cargo build --release -p app --bin live
cargo test --release --workspace
```

Build rules used while developing: one cargo process at a time and `CARGO_BUILD_JOBS=4` on the laptop. The test suite has well over 300 unit tests plus golden and parity tests that need the model files.

`scripts\package.ps1` gathers a runnable bundle (the executable, models with hashes, the OpenVINO CPU libraries and a README) into `dist\vtuber-tracker`.

## Design history

The project was built change by change, each with a written spec, design notes and measured results. The notes are in `docs/notes/`; the key decisions were:

1. **CPU-only face tracking spike.** Proved that detector, landmarks, blendshape model and a native VRM render fit the three targets on a CPU-only laptop.
2. **Fixing head fit and expressions.** A head-fit rewrite against a canonical face model, a mapping table from ARKit blendshapes to VRM expressions, and a fix for models whose expression keys are optional (the vendored `bevy_vrm1` patch).
3. **Reducing live CPU and memory.** ONNX Runtime spinning off, a 30 fps cap, task-pool tuning, and the finding that most of the cost is the renderer floor, not the tracker.
4. **Renderer evaluation.** Bare wgpu, bare Bevy and kiss3d were measured; Bevy stayed because the saving did not justify rewriting VRM and MToon support.
5. **Upper-body tracking.** Chose MediaPipe pose and hand models first, then CIGPose-m with OpenVINO (decision D10), then reversed the default to 3D BlazePose after live testing showed a 2D model could not follow an arm pulled behind the body. RTMW3D was measured and rejected on speed.
6. **Live feedback rounds.** Hold the last pose instead of a T pose; per-bone One Euro smoothing; hand roll from the hand model; reach solve so hands touch the head; torso twist and lean; eyes and eyebrows; the debug drawing; per-joint confidence gating; a thumb fix; smile handling.
7. **Viewer UI.** Model loading and checks, focus mode, lighting presets, toasts, help and the settings panel.

## Limitations and roadmap

Limits today:

- Windows only (the camera path uses Media Foundation through `nokhwa`).
- Only VRM 1.0 loads; VRM 0.x is refused.
- Pose and hand weights are 2023 (see the currency exception above).
- An arm pulled far behind the body is under-reported.
- The lower body is not tracked, and head translation and neck bend beyond the face tracker's head rotation are not tracked.
- The Seed-san sample has a separate robotic arm skeleton that does not follow the humanoid bones; use a standard model for arm checks.
- No virtual camera output yet.

Open work, roughly by value:

- Wire OpenVINO to the face, pose-detector and hand models (about 2 times faster in the timing above).
- Virtual camera output: Spout2 sender first, then NDI, then a Windows 11 virtual camera (`add-virtual-camera-output`). Until then, capture the window with OBS Window Capture; the green and blue chroma-key backgrounds make that clean.
- Measure what focus mode saves in CPU and RAM.
- Lower-body sway, VRM 0.x loading, and a hand-label evaluation set.
- Retest a 2D to 3D lifter for arm depth.

## License

Licensed under either of

- Apache License 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)

at your option. The vendored `vendor/bevy_vrm1` keeps its own license. The tracking models are not part of this repository and carry their own licenses.

## Acknowledgements

Google MediaPipe (face, pose and hand models), OpenCV Zoo (hand model exports), the converters of the ONNX ports used for the face and pose models, Intel OpenVINO, `ort` and ONNX Runtime, Bevy, `bevy_vrm1`, and `nokhwa`.
