# Limb accuracy against a consensus of independent models

Date: 2026-10-18. Script: fixtures/private/out/consensus_eval.py (results in consensus_eval.json, overlays consensus_hands_*.png). No model was run for this; it reuses the outputs of the shootout, the rtmlib run and the YOLO26 run.

## Why a consensus

The earlier shootout scored every model against the official MediaPipe output, which favours models that share MediaPipe's weights. There are no human labels yet. So each model is scored here against the median of the other independent model families, never its own: mp (MediaPipe official full), rtm (RTMPose-m), cig (CIGPose-m) and yolo (YOLO26m). A model from the mp family is scored against the median of rtm, cig and yolo, and so on. Error is the pixel distance divided by the consensus shoulder width, over the four photos with arms in frame (hands_15, 16, 17, 19), six joints (both shoulders, elbows, wrists), joints outside the image skipped.

## Result (lower is better)

| Model | Shoulder | Elbow | Wrist | All |
|---|---|---|---|---|
| CIGPose-m (2026 weights) | 0.040 | 0.055 | 0.033 | 0.043 |
| RTMPose-m (rtmlib, fixed box) | 0.041 | 0.054 | 0.041 | 0.045 |
| YOLO26s-pose | 0.039 | 0.051 | 0.047 | 0.046 |
| YOLO26m-pose | 0.036 | 0.056 | 0.045 | 0.046 |
| RTMPose-s (rtmlib, fixed box) | 0.032 | 0.069 | 0.057 | 0.052 |
| MediaPipe official full | 0.046 | 0.087 | 0.065 | 0.066 |
| YOLO26n-pose | 0.055 | 0.118 | 0.083 | 0.085 |
| BlazePose ONNX port (our pick) | 0.057 | 0.128 | 0.097 | 0.094 |

## Reading it honestly

- This check is biased the other way from the earlier one: three of the four families are COCO-trained 2D models that share COCO's joint conventions, while MediaPipe labels joints differently. Both tests are therefore skewed, in opposite directions, and only human-labelled wrists, elbows and shoulders can settle it. Four photos is also a small sample; differences of 0.01 are noise.
- The ranking agrees on two things: our BlazePose ONNX port is less accurate than the official MediaPipe (0.094 vs 0.066 here; 3.5 to 12.7 px off in the earlier test), so part of the gap is the port's crop and decoding and is fixable; and YOLO26n is clearly less accurate than YOLO26s and m (0.085 vs 0.046).
- CIGPose-m is the most accurate here and the only model with 2026 weights besides YOLO26 (Apache-2.0, 2D only, no depth or visibility). On the Kaggle CPU its pose model alone takes 67 ms (1 thread), about 2.2 times the BlazePose landmark model. Its speed on the laptop is not measured.
- Depth: BlazePose gives 3D world landmarks and visibility; every other model here is 2D. The avatar retargeting uses 3D arm directions.

## Laptop timing (AMD Ryzen 5 7430U, plugged in, random input, 80 runs, median ms)

Script: fixtures/private/out/limb_engine_time.py. Model files: RTMPose from download.openmmlab.com (2023-05 body7), CIGPose-m from the namas191297/cigpose-onnx release extracted on Kaggle (gpannn/vtuber-cigpose-extract). Pose network only; a person box (detector or tracked box) is extra for the RTM and CIG models, and BlazePose has its own detector (pose_detector, 4.7 to 10 ms).

| Model | Size | 4 threads ORT / OpenVINO | 5 threads ORT / OpenVINO | 1 thread ORT / OpenVINO |
|---|---|---|---|---|
| BlazePose landmarks (our pick, 2023 weights) | 12.8 MB | 6.66 / 4.65 | 6.48 / 4.35 | 14.6 / 12.8 |
| RTMPose-s body7 (2023 weights) | 21.9 MB | 5.76 / 5.89 | 5.31 / 5.46 | 15.3 / 17.0 |
| RTMPose-m body7 (2023 weights) | 54.3 MB | 14.9 / 15.3 | 13.6 / 13.7 | 41.0 / 44.1 |
| CIGPose-m COCO (2026 weights) | 56.3 MB | 15.3 / 15.7 | 13.7 / 14.1 | 41.7 / 44.9 |

OpenVINO gives no gain on the RTMPose and CIGPose networks (they are plain convolution stacks); it gains about 30 to 35 percent only on BlazePose. An earlier run of the same BlazePose model measured 11 ms on ONNX Runtime at 5 threads when Chrome was busy, so expect about 10 to 40 percent run-to-run noise on this laptop.

Reading: CIGPose-m costs about 14 ms per frame, about 2 to 3 times BlazePose landmarks, and the same as RTMPose-m. RTMPose-s is as fast as BlazePose. A tracker with CIGPose-m for limbs, OpenVINO for the face and hand models (about 3.4 plus 4.7 ms) and a tracked person box would be roughly 22 to 25 ms per frame, inside the 30 ms gate. All of the RTM and CIG models are 2D only, with no depth or visibility.

## Next step if accuracy matters

Hand-labelled joints for a proper score, and a decision on how to get arm depth if a 2D-only model is used.
