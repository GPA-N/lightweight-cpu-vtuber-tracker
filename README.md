# Lightweight CPU VTuber Tracker

*Efficient, optimized, lightweight webcam face, body and hand tracking for VRM avatars.*

**Contents:** [Status](#status) | [Build and run](#build-and-run) | [Controls](#controls-in-the-window) | [Command line](#command-line) | [How it works](#how-it-works) | [License](#license)

Keywords: VTuber, VRM, face tracking, body tracking, hand tracking, motion capture, MediaPipe, ONNX Runtime, Bevy, Rust, CPU-only, webcam.

An efficient, optimized and lightweight webcam face and upper-body tracker that drives a VRM 1.0 avatar, written in Rust. It runs the tracking models on the CPU only (no GPU needed for inference), caps rendering at about 30 fps, and has a focus mode that switches off everything except the avatar render to save CPU and RAM while recording or streaming.

It tracks:

- **Face:** head turn, 52 ARKit blendshapes (blink, mouth, brows, cheeks), and eye gaze.
- **Body:** shoulders, arms, torso twist and lean, from a 3D pose model.
- **Hands:** wrist turn and finger curl, from a 21-point hand model.

Measured on an AMD Ryzen 5 7430U laptop (Windows 11, no discrete GPU): about 10 ms median tracking latency, 30 fps rendering, and about 550 MB working set with the camera at 1280x720.

## Status

Working and in active development. Windows only for now (the camera code uses Media Foundation through `nokhwa`).

Known limits:

- Only VRM 1.0 files load. VRM 0.x files are refused with a clear message.
- The pose and hand models are 2023 MediaPipe-family weights. No small, CPU-fast, permissively licensed newer 3D arm model was found.
- An arm pulled far behind the body is under-reported by the pose model.
- The lower body is not tracked.
- There is no virtual camera output yet. Capture the avatar window with OBS Window Capture; the settings panel has green and blue chroma-key backgrounds for that.

## Build and run

Requirements: Windows 11, a webcam, and the Rust toolchain pinned in `rust-toolchain.toml`.

```powershell
# 1. Fetch and verify the face models (about 6 MB)
powershell -ExecutionPolicy Bypass -File scripts\fetch_models.ps1

# 2. Put the upper-body models in models\upper_body\ (sources and hashes: docs/notes/ub-models.md
#    and models/SHA256SUMS)

# 3. Build
cargo build --release -p app --bin live

# 4. Run with a VRM 1.0 model of your own
target\release\live.exe --body --vrm path\to\model.vrm
```

Model files and sample avatars are not stored in this repository; each has its own license (see `docs/notes/licenses.md`).

### Controls in the window

| Key | Action |
| --- | --- |
| S | Settings panel (Basic and Advanced tabs, saved to `settings.json`) |
| F | Focus mode: hides the preview and debug drawing to save CPU and RAM while recording or streaming |
| L | Cycle lighting presets (Studio, Soft, Flat, Dramatic) |
| H | Help |
| Ctrl+O | Open a `.vrm` model |
| Drop a file | Load a `.vrm` model |

When a model loads but lacks parts the tracker drives (humanoid bones, expressions, morph targets), a warning names what will not move.

### Command line

```
live [--vrm PATH] [--models DIR] [--camera INDEX] [--camera-size WxH] [--threads N]
     [--seconds N] [--body] [--body-models DIR] [--limb blaze|cigpose]
     [--no-overlay] [--focus] [--no-mirror]
```

## How it works

Each camera frame goes to a face pipeline (detector, 478-point landmarks, blendshape model, head-pose fit) and, with `--body`, to a body pipeline (pose model, hand detector, hand landmarks). The body pipeline runs on its own thread. Each joint is used only when its confidence is high enough and it is inside the picture; a part that is not seen keeps its last pose. A solver turns the tracked points into VRM humanoid bone rotations, and One Euro filters smooth them.

The crates: `core` (frame types), `capture` (camera), `infer` (ONNX Runtime sessions and the trackers), `pose` (head fit), `smoothing`, `facelogic`, `avatar` (Bevy window, VRM, solver, settings UI), `app` (the `live` and `app` programs), `bench` and `timing` (measurement).

Design decisions, model comparisons and measurements are written up in `docs/notes/`.

## License

Licensed under either of

- Apache License 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)

at your option. The vendored `vendor/bevy_vrm1` keeps its own license. The tracking models are not part of this repository and carry their own licenses.

## Acknowledgements

Google MediaPipe (face, pose and hand models), OpenCV Zoo (hand model ports), `ort` and ONNX Runtime, Bevy, and `bevy_vrm1`.
