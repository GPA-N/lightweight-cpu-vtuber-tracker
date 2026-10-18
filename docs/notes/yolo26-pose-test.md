# YOLO26 pose n / s / m on Kaggle, following the Ultralytics docs

Date: 2026-10-18. Kernel: gpannn/vtuber-yolo26-pose-run (private, Kaggle CPU, Intel Xeon 2.2 GHz, 4 cores, onnxruntime 1.31.0, ultralytics 8.4.18x). Script: fixtures/private/kaggle/yolo26/run.py. Raw output: fixtures/private/out/yolo26/ (ONNX variants) and yolo26_v1/ (PyTorch variants). Licence: AGPL-3.0, evaluation only.

## What the docs say and what this run followed

- Scales are n, s, m, l, x. There is no "base"; m was used as the base scale.
- Weights are COCO keypoints, 17 keypoints, imgsz 640. Default head is one-to-many with NMS (`nms=None`). The NMS-free one-to-one head is `nms=False`; the docs' published accuracy and CPU ONNX speed use `nms=False`. ONNX exported with `nms=False` outputs (N, 300, 57).
- Published numbers (docs, COCO val, CPU ONNX on an Intel Xeon at 2.0 GHz, thread count not stated): n 57.2 AP, 40.3 ms, 2.9 M params, 7.6 GFLOPs; s 63.0 AP, 85.3 ms, 10.4 M, 24.1 GFLOPs; m 68.8 AP, 218.0 ms, 21.5 M, 73.3 GFLOPs.
- The earlier shootout (row l3) exported the default raw head and decoded it with our own code. That is not the docs' configuration. This run uses Ultralytics predict on PyTorch weights and on ONNX exported with `nms=False` and with the default `nms=None`, conf 0.25.

## Results (Kaggle CPU, relative only)

Ultralytics ONNX with `nms=False`, per image, median of 7 images, preprocess + inference + postprocess:

| Scale | 1 thread | 4 threads | Shoulder / elbow / wrist NME | People per photo |
|---|---|---|---|---|
| n | 166 ms | 124 ms | 0.059 / 0.133 / 0.087 | 2,1,1,1 |
| s | 459 ms | 254 ms | 0.052 / 0.090 / 0.076 | 2,1,1,1 |
| m | 1296 ms | 622 ms | 0.049 / 0.079 / 0.083 | 2,1,1,1 |

NME is the distance to the official MediaPipe output over the 4 photos with arms in frame (reference, not ground truth). For comparison on the same CPU: BlazePose landmark model about 30 ms plus detector 22 ms with NME 0.044 / 0.089 / 0.089; RTMPose-s fixed box 27 ms (1 thread), 0.045 / 0.099 / 0.073.

Default head (raw ONNX, NMS by Ultralytics) gives the same accuracy within 0.01 and the same speed class; PyTorch is slower (n 106 ms, s 268 ms, m 687 ms inference) and does not speed up with threads here.

## Findings

- Using the docs' `nms=False` head fixed the earlier weak wrist result for n: wrist NME 0.129 before, 0.087 now. The earlier shootout row l3n understated YOLO26n.
- Accuracy is in the same range as BlazePose and RTMPose-s against the MediaPipe reference; m has the best elbow (0.079) and s the best wrist of the three (0.076).
- Speed: the Kaggle 4-thread numbers are 2.5 to 3 times the docs' published CPU times, so this CPU is slower than the docs' machine or the docs used more threads. The laptop was not measured. The model runs the full 640x640 frame, whereas BlazePose runs a 256x256 crop.
- A second person is reported on hands_15 for every variant (a background person may be real), and on f0100 or f0200 for some heads.
- Licence AGPL-3.0: using YOLO26 in a distributed app means open-sourcing the app or buying an Ultralytics enterprise licence.

## Laptop timing (AMD Ryzen 5 7430U, 12 logical cores, plugged in)

Tool: fixtures/private/spikes/yolo-time (Rust, ort 2.0.0-rc.14, the same session settings as the app, letterbox to 640x640 in Rust, ONNX exported with nms=False on Kaggle). Input: the 236 recorded frames, no camera. Median total per frame (preprocess + inference), with 95th percentile:

| Model | 1 thread | 2 threads | 4 threads | 5 threads | 8 threads |
|---|---|---|---|---|---|
| YOLO26n-pose | 94.6 ms | 82.1 ms | 59.6 ms (p95 72.5) | 61.0 ms | 54.9 ms |
| YOLO26s-pose | 290 ms | 272 ms | 124.6 ms | 97.2 ms | 105.7 ms |

A person was found in 233 of 236 frames for n (mean top confidence 0.79) and 236 of 236 for s. The same machine measured the current BlazePose pipeline at 16.6 ms median with 4 threads (docs/notes/ub-benchmark.md). So YOLO26n costs about 3.6 times as much as the current pose stage and would by itself exceed the 30 ms gate; s is worse. Verdict: reject on speed for this laptop, in addition to the AGPL licence.

## YOLO26n-pose on OpenVINO 2026.4.1 (same laptop, 5 threads, 640x640 random input, 60 runs, Chrome was open so expect about 15 percent noise)

The NMS-free ONNX export (nms=False) cannot be compiled by OpenVINO: its ONNX reader rejects the head's GatherElements node (shapes [1,8400,56] and [1,300,4]). Two forms that work, exported on Kaggle (gpannn/vtuber-yolo26-export):

| Form | Output | Median ms (two rounds) | ONNX Runtime on the raw ONNX |
|---|---|---|---|
| Raw ONNX (default head) on OpenVINO | (1, 56, 8400), NMS done by the caller | 35.0, 38.1 | 37.8, 54.0 |
| Ultralytics OpenVINO IR, default head | (1, 56, 8400) | 35.2, 38.3 | |
| Ultralytics OpenVINO IR, nms=False | (1, 300, 57) | 40.1, 41.8 | |

OpenVINO gives YOLO26n only 1.0 to 1.4 times the speed of ONNX Runtime (the MediaPipe models gain about 2 times), because the model is dominated by 640x640 convolutions (7.6 GFLOPs). At best about 35 ms per frame, against about 6 to 10 ms for the BlazePose pose stage on OpenVINO and 16.6 ms on ONNX Runtime. It is body only (17 keypoints, no fingers).

## Not done

Hand-labelled ground truth, l and x scales, fine-tuning on desk framing.
