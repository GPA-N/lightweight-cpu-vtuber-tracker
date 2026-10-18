# YuNet check

Question: should OpenCV YuNet replace our current face detector?

**Answer: SKIP.** The weights are NOT 2026. The saving is about 0.66 ms per frame at best, and that number uses a 160x120 benchmark input that is too small for our use.

## Findings

The `face_detection_yunet_2026may.onnx` file is a dynamic-shape re-export of the 2023mar weights. Its commit message says it was made for the OpenCV 5.x ORT engine. The README says it was "re-exported from face_detection_yunet_2023mar.onnx with static H/W dims replaced by symbolic dims." The libfacedetection.train weight commits are dated 2022 to 2023. Our detector is therefore NOT a 2026 model. The 2026 date is only on the export.

The licence is MIT for opencv_zoo, and the upstream libfacedetection code is 3-clause BSD. The newest libfacedetection release is v3.0 (2021-09-24). Its 2026 commits are SIMD fixes and a Rust binding, with no YuNet weight change. The libfacedetection.train repo was last pushed 2026-07-10. It added YuNet-pose support but no new weights.

The zoo README reports 0.834 / 0.824 / 0.708 on WIDER Face easy / medium / hard, but its eval table reports 0.8844 / 0.8656 / 0.7503. The two disagree, so we cannot tell which figure is right. The paper claims are not checked, because the Springer page redirected to a login and we could not read it.

## Table

| Check | Item | Status | Source |
|---|---|---|---|
| 1 | 2026may ONNX is 2023mar weights re-exported | VERIFIED. **NOT 2026** for weights | github.com/opencv/opencv_zoo, models/face_detection_yunet (commit 26cc381e4d) |
| 1 | Newest libfacedetection release | VERIFIED: v3.0, 2021-09-24 | api.github.com/repos/ShiqiYu/libfacedetection/releases |
| 1 | Licence (zoo / libfacedetection / .train) | VERIFIED: MIT / BSD-3 / BSD-3 | repo LICENSE files via gh api |
| 2 | Zoo CPU mean 0.69 ms, Intel 12700K, OpenCV 4.10, input 160x120 | VERIFIED | github.com/opencv/opencv_zoo/blob/main/benchmark/README.md |
| 2 | Zoo RPi4 Cortex-A72 1.5 GHz, 6.23 ms, input 160x120 | VERIFIED | same as above |
| 2 | Paper latency, hardware, single-scale | UNVERIFIED (Springer login redirect) | doi 10.1007/s11633-023-1423-y |
| 2 | Hard AP 0.708 vs 0.7503 vs 81.1% | CONFLICTING, UNVERIFIED | zoo README vs eval table vs hub.baai.ac.cn/view/31672 |
| 3 | Detector cadence | VERIFIED: every 5 frames while face held and presence >= 0.5; every frame if face lost or presence < 0.5 | crates/facelogic/src/schedule.rs (interval 5) |
| 3 | Our detector cost | USER-GIVEN: about 4 ms per run. Not checked in repo | user brief |
| 3 | Landmark cost | USER-GIVEN: about 12 ms per frame. Not checked in repo | user brief |
| 3 | Average saving, steady state | CALCULATED: 0.2 x (4 - 0.69) = 0.66 ms per frame | formula below |
| 4 | YuNet drives 478-point landmark model | YES, with an adapter. YuNet gives 5 points (right eye, left eye, nose, mouth corners); crop.rs needs eye points at k0 and k1 and a matching box format | demo.py lines 55-60, 73; crates/facelogic/src/crop.rs |
| 4 | Helps hands or limbs | NO. YuNet is face only | model design |

## Arithmetic

Detector runs on 1 frame in 5 when the face is held and presence is good (schedule.rs, interval 5). Average detector cost per frame is 0.2 x cost per run.

- Current: 0.2 x 4 ms = 0.80 ms per frame
- YuNet at zoo 0.69 ms: 0.2 x 0.69 = 0.138 ms per frame
- Saving: 0.80 - 0.138 = 0.66 ms per frame
- Share of frame: 0.66 / (12 + 0.80) = about 5 percent

When the face is lost, the detector runs every frame. Saving then is 4 - 0.69 = 3.31 ms per frame.

## Caveats

The 0.69 ms number uses a 160x120 input on a desktop CPU. Our detection needs a larger input to find small or distant faces, so YuNet would cost more than 0.69 ms. The 4 ms and 12 ms figures come from the brief, not from a benchmark run. The paper latency and hard AP claims are UNVERIFIED.
