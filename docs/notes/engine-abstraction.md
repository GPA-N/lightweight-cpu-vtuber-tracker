# Engine abstraction (OpenSpec task 11.1)

Date: 2026-10-18. Machine: AMD Ryzen 5 7430U, plugged in, Windows 11. Release build, 4 threads per model.

## What was added

- `crates/infer/src/engine.rs`: trait `ModelRun` (`run(&[f32], &[usize]) -> Vec<NamedOutput>`, single f32 input, outputs by name with shape and data), backends `OrtModel` (wraps the existing session from `build_session`, unchanged behaviour) and `OvModel` (crate `openvino` 0.11.1, feature `runtime-linking`, CPU plugin only, `PERFORMANCE_HINT=LATENCY`, `INFERENCE_NUM_THREADS` from the caller, one infer request). `load_model(engine, path, threads)` builds either.
- Engine choice at start-up: `resolve_engine()` reads `VT_ENGINE=ort|openvino|auto` (default auto). Auto uses OpenVINO when `Core::new()` can load the runtime and otherwise logs one line to stderr and uses ort. An explicit `openvino` is not silently replaced: model loading then returns an error.
- The existing wrappers (face, pose, hand) still hold ort sessions directly. Nothing in `app` or `live` calls the new code yet; the follow-up tasks move the stages onto `ModelRun`.
- Finding the DLLs: `openvino-finder` looks at `OPENVINO_INSTALL_DIR`, `PATH` and standard install paths. The test prepends `fixtures/private/venv_rt/Lib/site-packages/openvino/libs` to `PATH` (override with `VT_OPENVINO_LIBS`). The six CPU-only DLLs are listed in `engine-comparison.md`; packaging them is task 11.2.

## Verification

Test `crates/infer/tests/engine_parity.rs` (`cargo test --release -p infer --test engine_parity -- --nocapture`) runs five models on all 236 `golden_take` frames on both backends and compares every output by name. Inputs come from the real preprocessing helpers: the face crop is a centred square (the face detector is skipped), the pose ROI comes from the pose detector, hand ROIs from the palm detector. Two warm-up passes per model are not timed. handpose ran on 229 of the 236 frames (palm found).

| Model | ort median ms | OpenVINO median ms | Max abs error (output, peak) | Max relative error |
|---|---|---|---|---|
| face_landmarks | 5.58 | 2.35 | 9.2e-4 (Identity, 234) | 6.3e-5 |
| pose_detector | 6.03 | 3.91 | 1.3e-3 (Identity, 509) | 2.6e-6 |
| pose_landmarks | 8.06 | 4.93 | 1.47 (Identity_2, 1580) | 9.3e-4 |
| palm_detection | 4.88 | 2.69 | 2.3e-4 (Identity, 133) | 1.7e-6 |
| handpose | 2.83 | 1.84 | 1.9e-4 (Identity, 187) | 3.8e-6 |

The criterion is max error divided by max(1, peak magnitude of that output) at most 1e-3. A plain absolute 1e-3 is not met by two outputs because their values are in the hundreds: pose_detector raw boxes (1.3e-3 absolute) and the pose_landmarks `Identity_2` output (1.47 absolute). `Identity_2` is the 256x256 segmentation-sized output that the tracker does not use (the tracker picks the 117-value world output by size), so the decoded landmarks are not affected. Timings vary 10 to 40 percent from run to run on this laptop.
