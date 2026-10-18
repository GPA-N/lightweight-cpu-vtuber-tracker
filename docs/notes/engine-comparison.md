# Inference engine comparison: ONNX Runtime vs OpenVINO 2026 vs OpenCV 5 DNN

Date: 2026-10-18. Machine: AMD Ryzen 5 7430U (12 logical cores), plugged in, Windows 11. Python 3.12 environment in fixtures/private/venv_rt (throwaway). Versions: onnxruntime 1.31.0 (same as the ort 2.0.0-rc.14 crate bundles), OpenVINO 2026.4.1 (released 2026-10-01), OpenCV 5.0.0 (released 2026-06-06; latest 4.x is 4.14.0). Scripts: fixtures/private/out/engine_time.py, engine_parity.py, engine_mem.py.

What "OpenVC 2026" was taken to mean: OpenVINO (its releases are numbered 2026.x). OpenCV was tested too because the name is close and we use OpenCV's model zoo files.

Method: our own ONNX models (and YOLO26n-pose) with random float inputs in the model's own shape, 15 warm-up runs, 60 timed runs. Timing only. Accuracy parity is checked separately below. Python wrapper overhead is small; the Rust ONNX Runtime numbers in the live app are higher (landmarks 7.9 ms vs 5.7 ms here) because the app also renders.

## Median ms per model, 4 threads (95th percentile in the raw output)

| Model | ONNX Runtime | OpenVINO 2026.4.1 | OpenCV 5.0 DNN |
|---|---|---|---|
| face_detector | 1.17 | 0.53 | 1.75 |
| face_landmarks | 5.73 | 2.19 | 4.37 |
| face_blendshapes | 0.93 | 0.67 | 2.33 |
| pose_detector | 10.09 | 4.68 | 19.79 |
| pose_landmarks | 11.27 | 6.30 | 24.04 |
| palm_detection | 7.38 | 3.96 | 10.47 |
| handpose | 3.22 | 2.88 | 8.26 |
| yolo26n_pose | 58.3 | failed to compile | 103.5 |

At 5 threads (the app default) OpenVINO is 1.2 to 2.6 times faster than ONNX Runtime on every model (face_landmarks 2.61 vs 5.74, pose_landmarks 5.68 vs 11.10, handpose 1.70 vs 3.66). At 1 thread the gap is small (1.1 to 1.4 times, except blendshapes where ONNX Runtime wins). OpenCV DNN is slower than ONNX Runtime on all models except face_landmarks (4.37 vs 5.73) and prints that it ignores the CPU target setting in its new graph engine.

All seven models back to back, per iteration: ONNX Runtime 40 ms wall and 115 to 128 ms CPU time; OpenVINO 22.6 ms wall and 90 to 111 ms CPU time (4 and 5 threads). Peak working set of the process: ONNX Runtime 184 to 186 MB, OpenVINO 294 to 318 MB (about 120 MB more).

Rough per-frame estimate for the full tracker at 5 threads (detector every 5th frame): face about 3.4 ms with OpenVINO vs 8.2 ms; pose detector plus landmarks 10.2 ms vs 19.3 ms; one hand (palm plus handpose) 4.7 ms vs 11.1 ms. Total about 18 ms vs 39 ms, which would bring the full tracker inside the 30 ms gate that it fails today (33.7 to 43 ms median).

## Output parity (ONNX Runtime vs OpenVINO, same random input)

Largest absolute difference per output is at most 4.9e-4 against output magnitudes of 15 to 207 (blendshapes 1.1e-6), except the unused 256x256 segmentation output of pose_landmarks (1.1 against values up to 1433). Numerically equivalent for our purposes; to be confirmed on real frames.

## Size: the smaller version

The OpenVINO pip package is 245 MB, mostly the NPU compiler (100 MB), GPU plugin (35 MB) and extra frontends. A CPU-only runtime needs only: openvino.dll 16.5 MB, openvino_intel_cpu_plugin.dll 46.3 MB, openvino_onnx_frontend.dll 5.4 MB (0.5 MB IR frontend instead if models are converted to OpenVINO IR files), tbb12.dll 0.3 MB, tbbmalloc.dll 0.1 MB, openvino_c.dll 0.2 MB for the C API. About 69 MB (about 63 MB with IR models), against about 13 MB for onnxruntime.dll. OpenCV 5 headless wheel is 112 MB and its Rust bindings need a system OpenCV install.

## Rust integration facts

- Crate `openvino` 0.11.1 (updated 2026-10-09, Apache-2.0, repo intel/openvino-rs pushed 2026-10-09), with `openvino-sys` 0.11.1; the runtime is loaded dynamically.
- OpenVINO licence is Apache-2.0. It runs on this AMD CPU (devices listed: CPU, GPU), although Intel tunes it for Intel CPUs.
- Our pipeline uses the `ort` crate (2.0.0-rc.14, updated 2026-10-12). A swap would put the model calls behind a trait with the engine chosen at start-up and ONNX Runtime kept as the fallback.

## Verdict

OpenCV 5 DNN: reject (slower, bigger, awkward in Rust). OpenVINO 2026.4.1: worth a Rust spike. It roughly halves the tracker's time and CPU per frame on this laptop, at the cost of about 55 MB more files and about 120 MB more memory. Not yet verified: real-frame parity, end-to-end live latency, the Rust crate on Windows with these exact models, and behaviour on other users' CPUs. These are timings of the same 2023-era MediaPipe weights (the engine changes speed, not the model date).
