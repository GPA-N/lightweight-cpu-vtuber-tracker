# Packaging layout

Build the bundle with `scripts/package.ps1`. It copies from `target/release/live.exe` (it does not build) and writes everything under `dist/vtuber-tracker/`. Reruns wipe and recreate only that folder. `dist/` is gitignored.

## Runtime needs (from `crates/app/src/bin/live.rs`)

- Default paths are relative to the current directory: `samples/Seed-san.vrm`, `data/blendshape_map.json`, `models/`, output `fixtures/private/out/live`. `run.bat` changes to its own folder first, so the bundle works from any cwd.
- Models loaded from `--models`: `face_detector.onnx`, `face_landmarks.onnx`, `face_blendshapes.onnx`.
- The VRM is canonicalised, then loaded by Bevy. The asset root is the VRM's parent folder and the file is loaded by bare name (`crates/avatar/src/window.rs`). No `assets/` folder is needed.
- The blendshape map is read at runtime from `--map`. Other `include_str!` data (`landmark_subset_146.json`, `canonical_face_model.obj`) is compiled in.
- ONNX Runtime (`ort =2.0.0-rc.14`, `download-binaries`, `copy-dylibs`) is linked statically. No `onnxruntime.dll` is needed. The CPU execution provider is used.
- `DirectML.dll` exists in `target/release/` as a symlink into the ort cache. It is not in the bundle. The verification run passed without it.
- Webcam: nokhwa with `input-msmf`, which uses Windows Media Foundation (system DLLs only).
- Rendering: Bevy 0.19 on Vulkan (AMD), which uses the GPU driver.

## Bundle layout

```
dist/vtuber-tracker/
  live.exe                      101,792,768 bytes (97.1 MiB)
  run.bat                       starts live.exe with bundle-relative paths, forwards extra flags
  README.txt                    run command, flags, contents, licence note
  models/
    face_detector.onnx          418,437 bytes
    face_landmarks.onnx         4,920,995 bytes
    face_blendshapes.onnx       1,094,706 bytes
    SHA256SUMS                  only the 3 shipped lines (checked against the repo file)
  data/
    blendshape_map.json         742 bytes
  samples/                      copy of the whole samples/ tree
    Seed-san.vrm                10,917,800 bytes
    (other sample folders, if present)
  out/                          created empty; reports go to out/live/
```

Total at the time of packaging: 125.8 MiB, including samples. Without samples the core is about 103.2 MiB (live.exe 97.1 MiB plus models 6.2 MiB). The script reports MiB (1048576 bytes) under the name "MB".

## Findings

- `models/SHA256SUMS` in the repo lists 5 files. Two entries (`model.onnx`, `model.onnx.data`) are the private fallback model, which must not ship. The bundle copy lists only the 3 shipped files.
- `samples/Seed-san.vrm` is under VRM Public License 1.0, and the repo note says "Private spike use only; do not commit or redistribute". Bundling it into `dist/` conflicts with that note. `dist/` is gitignored, but redistribution still matters. Resolve before sharing the bundle.
- The sample folders `100Avatars_031_Devil`, `Godette` and `VRM1_Constraint_Twist_Sample` were present in `samples/` when the script ran. Their licences were not checked here.
- `live.exe` was newer than all `crates/**/*.rs` sources (19:42:21 vs 19:42:06), so the script's staleness warning did not fire.

## Verification

Run from `C:\Windows\Temp` through `run.bat --seconds 4` with camera 0. Exit code 0. The report printed, the avatar window opened (`Avatar spike`, AMD Radeon, Vulkan), and `out/live/live_report.txt` and `live_stats.csv` were written. stdout and stderr contained no missing-file or error lines. One process at a time, and no `live.exe` was left running.

## Docker: honest note

A Docker image is not a simple wrap of this bundle, and no Dockerfile exists.

- `live.exe` is a Windows binary. A Windows container would need a Windows host. A Linux container cannot run it.
- The app uses Windows-only camera capture (Media Foundation via nokhwa) and the Windows DX12/Vulkan display path. Neither works in a Linux container without changes.
- A Linux build would need the camera switched to V4L2, a Linux display stack (Wayland or X11) and GPU passthrough for Vulkan. Camera access would need `--device /dev/video0`, and the window would need a display forwarded from the host.
- Running it in a container gives no packaging benefit over the folder bundle unless those changes are made first.
- The model and VRM files are unchanged by any of this. Only the binary and the platform layer differ.

The realistic next step is to keep the folder bundle, then decide whether a single exe is wanted (a self-extractor or embedded assets), which is deferred.
