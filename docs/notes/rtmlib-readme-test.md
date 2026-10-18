# rtmlib test, following the rtmlib README

Date: 2026-10-18. Kernel: gpannn/vtuber-rtmlib-readme-run (private, Kaggle CPU, Intel Xeon 2.2 GHz, 4 cores, onnxruntime 1.31.0, rtmlib 0.0.16). Script: fixtures/private/kaggle/rtmreadme/run.py. Raw output: fixtures/private/out/rtmreadme/. No errors.

## What was tested and why

The README (github.com/Tau-J/rtmlib, repo pushed 2026-08-03) sets up `Body` and `Wholebody` in three modes and an RTMO one-stage option, and runs the person detector only every N frames in `PoseTracker` (det_frequency=10). The earlier shootout row l4 ran Body(mode="balanced") with the detector on every image, so it measured a mostly-detector cost. This run times the detector and the pose model separately, and the pose model alone on a fixed full-image box (the cost of a frame where the detector is skipped), at 1 and 4 onnxruntime threads. Inputs are the same 7 images. Accuracy is the normalised distance to the official MediaPipe output, which is a reference and not ground truth.

## Body (17 keypoints), 1 thread unless stated

| Mode | Detector ms | Pose alone ms (1 thread / 4 threads) | Shoulder / elbow / wrist NME (fixed box) | Weights |
|---|---|---|---|---|
| lightweight (YOLOX-tiny + RTMPose-s) | 109 | 27.4 / 16.0 | 0.045 / 0.099 / 0.073 | 2023-05 |
| balanced (YOLOX-m + RTMPose-m) | 1077 | 68.0 / 34.5 | 0.054 / 0.090 / 0.068 | 2023-05 |
| performance (YOLOX-x + RTMPose-x 384x288) | 3869 | 523 / 249 | 0.061 / 0.113 / 0.086 | 2023-06 |
| RTMO-m one-stage, no detector | - | 898 / 446 | 0.043 / 0.112 / 0.095 | 2023-12 |
| RTMO-s one-stage, no detector | - | 375 / 191 | 0.043 / 0.142 / 0.114 | 2023-12 |

For comparison on the same Kaggle CPU (1 thread): BlazePose landmark model about 30 ms plus detector 22 ms, NME 0.044 / 0.089 / 0.089; YOLO26n-pose 164 ms, NME 0.060 / 0.131 / 0.129.

With the README det_frequency=10 schedule the average per frame is pose + detector/10: lightweight 27.4 + 10.9 = 38 ms (1 thread), 16.0 + 6.6 = 23 ms (4 threads); balanced 68 + 108 = 176 ms (1 thread), 34.5 + 54 = 88 ms (4 threads).

## Whole-body (133 keypoints, includes 21 per hand)

Hands found match the reference on all four photos in every mode (2, 1, 2, 1). Mean hand NME21: lightweight 0.049, balanced 0.051, performance 0.044 (our 2023 opencv/MediaPipe port with the yakhyo export scores 0.019). Pose-alone time, 1 thread / 4 threads: lightweight 146 / 75 ms, balanced 255 / 128 ms, performance 540 / 268 ms. False positives 0 to 2.

## Verdict

- The earlier l4 rejection used a detector-dominated number. The fair figure for RTMPose-m alone is 68 ms (1 thread), which is slower than the BlazePose landmark model (about 30 ms). RTMPose-s (lightweight) is similar in speed (27 ms) and similar in accuracy to MediaPipe's BlazePose, not better.
- Every rtmlib model in the README is 2023 weights, so none passes the late-2026 currency rule either.
- RTMO one-stage and whole-body are too slow on this CPU for the 15 ms-class budget.
- Whole-body does give fingers and body in one pass, but at 75 ms or more with 4 threads it costs far more than pose (16 ms) plus the MediaPipe hand model (3.6 ms).
- No rtmlib configuration beats the current BlazePose and MediaPipe hand pipeline. Timings are Kaggle CPU and relative only. A local timing on the laptop was not run.
