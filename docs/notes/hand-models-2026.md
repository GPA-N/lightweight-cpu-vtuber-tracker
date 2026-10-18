# Hand and limb models, 2026 currency check (task H1)

Date of check: 2026-10-17. No model was run locally. Only repository metadata, model cards, HTTP headers and paper abstracts were read. Nothing downloaded was executed.

## Summary

Limbs come first, fingers second.

1. **Hands: no 2026-trained weights satisfy the brief.** Every lightweight hand model that is 21-keypoint, 3D, has handedness and is permissively licensed carries Google MediaPipe weights from 2021 to 2023. Several of them were re-exported or re-uploaded in 2026, which is why they look current on Hugging Face. The official `hand_landmarker.task` bundle has `Last-Modified: Wed, 26 Apr 2023`. The newest MediaPipe release (v1.1.1, 2026-10-14) lists no hand or pose model change. The genuinely new 2026 hand work (Fast-HaMeR, WiLoR updates, ScaleHP and others) is GPU-class, depends on MANO (non-commercial) or has non-commercial weights. **The user has to decide.** The choices are listed under "Decision for the user" below.
2. **Limbs: one candidate is truly 2026.** CIGPose (CVPR 2026, arXiv 2026-03-10, repo pushed 2026-10-15, Apache-2.0) beats RTMPose at the same cost on COCO. Its weights are on Google Drive, there is no ONNX, and conversion is unproven. The ready-to-run fallback is RTMPose-m, whose weights date from 2023 and whose ONNX has been exported again in 2026 (Qualcomm AI Hub, 2026-10-07, built against ONNX Runtime 1.30).
3. **The current pick, `fernandotonon` BlazePose ONNX (2026-07-13), is a 2026 upload of 2023 weights.** The same applies to the pose landmarks graph. See "Challenge" below.

## Date evidence (verified from the source, not from the model card text)

| Item | Evidence | Result |
|---|---|---|
| `hand_landmarker.task` (`float16/latest` and `/1`) | `Last-Modified` header on storage.googleapis.com, 7 819 105 bytes | 2023-04-26 |
| `pose_landmarker_lite/full/heavy.task` | same method, 5.78 / 9.40 / 30.66 MB | 2023-04-27 and 2023-04-28 |
| `mediapipe` on PyPI | upload times | 0.10.33 2026-03-18, 0.10.35 2026-04-27, 1.0.0 2026-07-27, 1.0.1 2026-08-14, 1.1.0 2026-10-06, 1.1.1 2026-10-14. Licence Apache 2.0 |
| MediaPipe GitHub release notes v0.10.35, v1.0.0, v1.1.0, v1.1.1 | read in full | Text embedders, LiteRT, Flutter, build changes. No hand or pose model change |
| `fernandotonon/QtMeshEditor-blazepose-onnx` | HF created and modified 2026-07-13, `pose_detector.onnx` 11 926 470 bytes | Detector only. Landmarks are in `QtMeshEditor-models/mocap/pose/pose_landmarks.onnx` (12 766 212 bytes). NOTICE.txt says they are converted from the Google Pose Landmarker bundle |

Note on the repo name: the ids `fernandotonon/poselandmarks-onnx` and `fernandotonon/blazepose-onnx` do not exist on the Hub. The real ids are `fernandotonon/QtMeshEditor-blazepose-onnx` and `fernandotonon/QtMeshEditor-models`.

## Limb models (shoulders, elbows, wrists)

Accuracy is quoted as published. The papers I could read give COCO AP and whole-body AP. I found **no published per-joint wrist or elbow numbers** for CIGPose or RTMPose in the sources above, so the shoulder, elbow and wrist ranking below rests on overall COCO AP. That gap should be closed by our own benchmark (task group 1), measured on wrist and elbow error only.

| Candidate | Weights date | Licence | Size / input | Accuracy (source) | CPU cost (source) | ONNX status |
|---|---|---|---|---|---|---|
| BlazePose GHUM Full (current pick) | 2023-04 (re-uploaded 2026-07-13) | Apache-2.0 | landmarks 12.8 MB fp32, detector 11.9 MB, 256x256 / 224x224 | PCK@0.2 95.5 (yoga), 96.3 (dance), 95.7 (HIIT); mAP 62.6 / 67.4 / 68.0 on Google's own sets, not COCO (MediaPipe `docs/solutions/pose.md`) | 27 ms for the whole pipeline on a 2017 MacBook Pro, Lite 25 ms, Heavy 38 ms (same page) | Converted by the author, DENSIFY workaround for sparse weights, parity claimed against the Python reference |
| RTMPose-t / s (body) | 2023-01 | Apache-2.0 | 256x192 | COCO AP 68.5 / 72.2 (mmpose RTMPose README) | ORT 3.20 / 4.48 ms per person on i7-11700, 0.36 / 0.68 GFLOPs (same table) | Official ONNX zips, rtmlib (pushed 2026-08-03) runs them with ONNX Runtime |
| RTMPose-m (body) | 2023-01 | Apache-2.0 | 256x192, 13.6 M params | COCO AP 75.8, PCK@0.1 Body8 94.1 (same table) | ORT 11.06 ms on i7-11700, 1.93 GFLOPs | Official ONNX. Qualcomm AI Hub re-exported the wholebody variant (rtmpose-m coco-wholebody 256x192, 68.5 MB float, ONNX Runtime 1.30.0) on 2026-10-07 |
| RTMPose-l (body) | 2023-01 | Apache-2.0 | 256x192 | COCO AP 76.5 | ORT 18.85 ms, 4.16 GFLOPs | Official ONNX |
| RTMW-m / l (whole-body) | 2023-09 | Apache-2.0 | 256x192 / 384x288 | COCO-WholeBody AP 58.2 (m), 66.0 (l 256x192), 70.1 (l 384x288, RTMPose README) | 4.3 / 7.9 / 17.7 GFLOPs | Official ONNX, rtmlib |
| **CIGPose-m (body)** | **paper 2026-03-10, repo pushed 2026-10-15** | **Apache-2.0** | 256x192, 14 M params, 1.9 GFLOPs | COCO val AP 76.6, AR 79.3 (CIGPose README) | Not published. FLOPs match RTMPose-m, so expect similar order of cost. Unmeasured | **None.** Checkpoint on Google Drive, MMPose project code, graph neural network head. Export unproven |
| **CIGPose-l (body)** | same | Apache-2.0 | 256x192 / 384x288, 28 M params | COCO AP 77.6 (4.2 GFLOPs) / 78.5 (9.4 GFLOPs) | Unmeasured | None |
| CIGPose-m whole-body | same | Apache-2.0 | 256x192, 2.3 GFLOPs | COCO-WholeBody body AP 69.0, hand AP 49.7, whole AP 59.9 | Unmeasured | None |
| CIGPose-x whole-body | same | Apache-2.0 | 384x288, 18.7 GFLOPs | whole AP 67.0 (67.5 with UBody), hand AP 60.2 | Unmeasured | None |
| ViTPose-small | 2022 weights, repo pushed 2025-12-25 | Apache-2.0 | 256x192 | COCO AP about 73.8, from memory of the paper, not re-checked | Unmeasured | Community ONNX exports (for example `JONNYVERSE/vitpose-plus-small-ONNX`, 2026-08-24) |
| YOLO26-pose (Ultralytics) | paper 2026-06-02 | **AGPL-3.0** (repo licence verified) | n/a | Not checked | Not checked | Rejected: licence trap for a shipped product unless Ultralytics Enterprise is bought |
| DWPose | repo last pushed 2023-12-12 | Apache-2.0 | n/a | n/a | n/a | Rejected: not current |
| MoveNet successor | none found | | | | | Nothing newer than 2021 found |
| Sapiens | 2024, repo 2026-05-26 | Licence not detected (`NOASSERTION`, CC-BY-NC family) | large ViT | n/a | far above budget | Rejected |
| BBoxMaskPose v2 / PMPose | arXiv 2026-01-21 | Not checked | n/a | Reported as top-down 2D estimator | Not checked | Listed only. Licence and weights not verified |

### Limb verdict

- **Most efficient (2023 weights, official ONNX): RTMPose-t**, 3.2 ms per person on an i7-11700 with ONNX Runtime. Accuracy is 68.5 AP, which is the weakest here.
- **Most accurate permissive and 2026: CIGPose-l 384x288** (78.5 AP). It costs 9.4 GFLOPs and its export is unproven.
- **Best trade-off that is 2026: CIGPose-m 256x192.** It has about the same FLOPs as RTMPose-m (1.9 against 1.93) and 0.8 AP more (76.6 against 75.8). It is accepted under the user's currency rule only after an ONNX export passes a parity check on a cloud machine. Until then, treat it as a candidate.
- **Best trade-off that is ready today: RTMPose-m.** Weights are 2023. It must be flagged to the user as an exception under the currency rule.

RTMPose and CIGPose are 2D only (x, y and a score per keypoint, no depth, no visibility). BlazePose returns z, visibility and metric world coordinates, which the retarget code may use. If depth is needed, keep BlazePose for the torso and use the 2D model for wrists and elbows, or lift in 3D by bone length constraints. This is a design decision for the retarget task.

## Hand models (21 keypoints, fingers)

| Candidate | Weights date | Licence | Size / input | Outputs | Accuracy (source) | CPU cost | ONNX status |
|---|---|---|---|---|---|---|---|
| MediaPipe hand landmark full via `yakhyo/mediapipe-hand-landmark-onnx` | Google 2023 weights, ONNX export 2026-07-28 | Apache-2.0 | 224x224 N-batch, landmark model about 2 M params (Qualcomm card) | 21 x (x, y, z), presence, handedness, 21 world points | Google paper numbers not re-checked here | Not measured (no local compute). Qualcomm lists 1.0 ms on a phone NPU, 4.77 ms on an automotive GPU, which says nothing about desktop CPU | Real ONNX release, built from PyTorch state dicts that were loaded from PINTO0309's conversion of Google's float32 TFLite. No tf2onnx path needed. Palm detector 192x192, 2016 anchors, decode and NMS must be written by us |
| Same weights via Qualcomm AI Hub `qualcomm/MediaPipe-Hand-Detection` | HF modified 2026-10-07, ai-hub-models v0.64.0 | Apache-2.0 | 256x256 landmark model | same | n/a | NPU numbers only | ONNX exports exist on AI Hub, graph-level checks not done |
| STMicroelectronics `hand_landmarks` | HF modified 2026-01-23 | Apache-2.0 | int8 TFLite, 224x224 | (x, y, conf) only, no world, no handedness | n/a | 20.75 ms on STM32N6 NPU, not CPU | TFLite int8 only, provenance is PINTO0309's MediaPipe conversion |
| RTMPose-Hand (hand5 "alpha") | 2023, LiteRT re-export 2026-06-28, Pi 5 CPU measured 2026-08-31 | Apache-2.0 | 256x256, 28 MB fp16 (LiteRT) | 21 x 2D only via SimCC, no z, no handedness | PCK@0.2 96.4 on Hand5, 81.5 on COCO-WholeBody-Hand, AUC 83.9; 2.58 GFLOPs (mmpose README) | 58.1 ms on Raspberry Pi 5 CPU, 4 threads (LiteRT card). Desktop ORT figure not in the README | Official ONNX in the mmpose and rtmlib model lists. Needs a hand crop |
| Fast-HaMeR ConvNeXt-L, feature distillation | paper 2026-03-17, repo pushed 2026-03-01 | Code MIT. **MANO model needs the MANO licence (non-commercial)** | 240 M params | MANO mesh and 21 joints | HO3D-v2 PA-MPJPE 8.1 mm against HaMeR 7.7 mm (paper, read in full) | 40 FPS on an RTX 4060 Ti. No CPU number. Far above 3 to 10 ms on CPU | No ONNX. No public checkpoint confirmed |
| WiLoR | repo pushed 2026-04-07 | Weights `CC-BY-NC-4.0` (HF `rolpotamias/WiLoR`), MANO non-commercial | large | MANO | HO3D-v2 PA-MPJPE 7.5 mm (as quoted in the Fast-HaMeR paper) | Reported above 130 FPS on a GPU | Community ONNX (`warmshao/WiLoR-mini`). Licence trap |
| HaMeR | repo pushed 2026-02-07, MIT | MIT code, MANO non-commercial | ViT-H, 671 M params | MANO | 7.7 mm HO3D-v2 | 27 FPS on an RTX 4060 Ti | Not CPU class |
| SPLite Hand (arXiv 2510.16396) | **2025-10, not 2026** | Not found | small | 3D | Reported 15 FPS on a Raspberry Pi 5 | | Rejected on date. Recorded because it is the only lightweight 3D model I found |
| YOLO26 hand pose (`poptoz/yolo26-hand-pose-face-detection`, 2026-04-03) | 2026 | Not stated, YOLO26 is AGPL | ONNX | Not documented | Not documented | | Rejected: licence unknown and AGPL base |

### Hand verdict

- **Most accurate in the literature:** WiLoR / HaMeR family. Rejected for our pipeline: GPU only, MANO and weight licences are non-commercial.
- **Most efficient that actually has 21 points, 3D, world points and handedness:** MediaPipe hand landmark (full or lite) through `yakhyo` ONNX. Weights are 2023.
- **Best trade-off:** the same MediaPipe model, with a wrist crop taken from the pose model instead of the palm detector.
- **Best 2D-only alternative for fists and occlusion:** RTMPose-Hand, with higher PCK on public sets but no depth and no handedness.

Fist and occlusion evidence: I found no published number for fists specifically. The Fast-HaMeR paper reports occlusion and hand-object results on HO3D-v2 and shows qualitative cases; I did not read the MediaPipe hand paper body. Our fixtures must measure fist behaviour.

## Decision for the user

The currency rule cannot be met for hands with a lightweight, permissive, 3D model. I did not quietly accept anything. The options are:

1. Accept the 2023 MediaPipe weights through the 2026 `yakhyo` ONNX export (Apache-2.0, ready, no conversion), and record the exception in `upper-body-research.md`. Uses the same weights as the current face and pose pipeline.
2. Accept RTMPose-Hand (2023 weights, 2D only) and give up depth and handedness.
3. Pay the cost of a GPU-class 2026 model (Fast-HaMeR, WiLoR) on a server. That breaks the 3 to 10 ms on-CPU goal and brings MANO licence problems.
4. Fine-tune or distil our own 2026 hand landmark model on the cloud. Out of scope for this change.

My recommendation is option 1 for the first release, because the budget and licence conditions leave nothing else, and the limbs (which come first) get a current model anyway.

## What the Rust pipeline needs

### Limbs: CIGPose-m or RTMPose-m (top-down)

- Person box: reuse the BlazePose detector or the face box; for one seated user a previous-frame box expanded 1.25 times is enough. Re-detect every 10 frames or when the score drops (rtmlib does the same with `det_frequency=10`). RTMDet-nano is 0.99 GFLOPs if a real detector is wanted.
- Pre-processing: affine crop of the box to 256x192 with aspect ratio kept, mmpose mean and std (123.675, 116.28, 103.53 / 58.395, 57.12, 57.375), RGB, NCHW.
- Output: SimCC vectors `simcc_x [K, 2*W]` and `simcc_y [K, 2*H]`. Take argmax, divide by the split ratio 2, map back through the inverse affine. Confidence is the min of the two peak values. Same decode as the LiteRT hand card.
- CIGPose-m output head is not the same as RTMPose-m's; it needs an export from the MMPose project config and a check of the output tensor names before any Rust work.
- Temporal smoothing: One-Euro on wrists and elbows.

### Hands: MediaPipe hand landmark (yakhyo ONNX)

- Crop source: wrist and index-knuckle from the pose model, or the palm detector. Detector-free route: take the pose wrist, extend 1.5 times the forearm length along the elbow-to-wrist direction to estimate the hand centre, then square box of 2.0 times the hand length. This saves 3 to 4 ms per frame and a second graph.
- If the palm detector is used: input 192x192, 2016 anchors, weighted NMS, ROI rotation so wrist to middle MCP points up, scale 2.6, shift y by -0.5 (the standard MediaPipe ROI transform).
- Landmark input: 224x224 RGB crop in [0,1], rotation-corrected. Outputs: 63 screen values, presence, handedness, 63 world values. Mirror the crop for left hands and flip x of the output.
- Tracking: reuse the landmarks of the previous frame to compute the next ROI. Run the palm detector only when presence drops under 0.5.
- ONNX Runtime 1.31 (ort 2.0.0-rc.14): the yakhyo graphs are plain convolutions, depthwise convolutions, PReLU/ReLU and sigmoids. I expect no op problem, but I have not opened the graphs (loading for metadata is allowed, I skipped it to stay inside the time budget). That check belongs in task group 1.

### If we ever convert the official bundle ourselves

- A `.task` file is a zip. It holds `hand_detector.tflite` and `hand_landmarks_detector.tflite`. Unzip with a tool that does not execute anything, into an empty directory.
- tf2onnx, onnx2tf and tflite2onnx all work on the float16 bundle only after dequantising. Known problems: float16 constants (Dequantize nodes), the DENSIFY op for sparse weights (the pose detector has it, tf2onnx fails; the `fernandotonon` export script works around it by densifying through the TFLite interpreter), NHWC to NCHW transposes, and PRelu slopes shaped `[1,1,C]`. Do it on Kaggle, not locally.
- Licence: Apache-2.0 for code and weights (MediaPipe repo licence, verified through the GitHub API).

## Pareto (accuracy against CPU cost)

Cost is the number published for the closest stated hardware. The units are not equal across rows, so the order is indicative only. Rows marked "unmeasured" must be benchmarked on Kaggle.

| Task | Point | Accuracy | Cost | On the front? |
|---|---|---|---|---|
| Limbs | RTMPose-t | 68.5 AP | 3.2 ms (i7-11700, ORT) | yes (cheapest) |
| Limbs | RTMPose-s | 72.2 AP | 4.5 ms | yes |
| Limbs | RTMPose-m | 75.8 AP | 11.1 ms | yes |
| Limbs | CIGPose-m | 76.6 AP | about RTMPose-m, unmeasured | yes if the cost holds |
| Limbs | RTMPose-l | 76.5 AP | 18.9 ms | dominated by CIGPose-m |
| Limbs | CIGPose-l 384x288 | 78.5 AP | 9.4 GFLOPs, unmeasured | yes (most accurate) |
| Limbs | BlazePose Full | different benchmark, PCK@0.2 about 96 | 27 ms full pipeline on a 2017 laptop | not comparable, has z |
| Hands | MediaPipe hand full | not re-checked | unmeasured, 2 M params | yes (only 3D lightweight) |
| Hands | RTMPose-Hand | PCK@0.2 96.4 (Hand5) | 2.58 GFLOPs, 58 ms on Pi 5 | 2D only |
| Hands | Fast-HaMeR ConvNeXt-L | 8.1 mm PA-MPJPE | GPU only | outside the CPU budget |
| Hands | WiLoR / HaMeR | 7.5 / 7.7 mm | GPU only | outside, licence |

## Challenge to my own picks

- **Is CIGPose really 2026?** Paper on arXiv 2026-03-10, CVPR 2026 poster, repo pushed 2026-10-15. Yes. The checkpoint upload date is not visible on Google Drive and was not verified.
- **Is the accuracy gain real?** The README table is the authors' own. +0.8 AP over RTMPose-m is small and may vanish once a different detector or crop is used. The benchmark must compare wrist and elbow error directly.
- **Is conversion faithful?** Unknown for CIGPose: the graph head and the "canonical embedding" replacement use uncertainty-based selection, which can introduce data-dependent control flow that ONNX export handles badly. This is the main risk and the reason RTMPose-m stays as the ready fallback.
- **Licence traps found:** WiLoR weights are CC-BY-NC; every MANO-based model needs the non-commercial MANO file; Ultralytics YOLO26 is AGPL-3.0; the Sapiens licence was not detected. The CIGPose repo licence is Apache-2.0 per the GitHub API, but the README only says "released under the LICENSE", and the pretrained backbone licences inherited from MMPose were not checked.
- **Is the current BlazePose pick really 2026?** Only the upload. The weights are Google's April 2023 bundle. The user's rule allows "model upload date", but the intent (a current model) is not met. I am saying this plainly so it is not read as a pass.
- **Things I could not verify:** desktop CPU timings of any model (no local compute allowed), the per-joint wrist and elbow accuracy of any limb model, the hand-landmark accuracy number in the MediaPipe paper (the alphaXiv lookup failed), whether the Fast-HaMeR checkpoints are public, the ViTPose-small AP (from memory), PINTO model zoo 2026 hand exports (GitHub rate limit stopped the listing; the 033 folder was not re-dated).

## Sources

- https://storage.googleapis.com/mediapipe-models/hand_landmarker/hand_landmarker/float16/latest/hand_landmarker.task (header)
- https://storage.googleapis.com/mediapipe-models/pose_landmarker/pose_landmarker_full/float16/latest/pose_landmarker_full.task (header)
- https://pypi.org/pypi/mediapipe/json and https://github.com/google-ai-edge/mediapipe/releases
- https://github.com/google-ai-edge/mediapipe/blob/master/docs/solutions/pose.md and `models.md`
- https://huggingface.co/fernandotonon/QtMeshEditor-blazepose-onnx and https://huggingface.co/fernandotonon/QtMeshEditor-models
- https://github.com/53mins/CIGPose and https://arxiv.org/abs/2603.09418
- https://github.com/open-mmlab/mmpose/blob/main/projects/rtmpose/README.md
- https://github.com/Tau-J/rtmlib
- https://huggingface.co/qualcomm/RTMPose-Body2d
- https://huggingface.co/litert-community/RTMPose-Hand-LiteRT
- https://github.com/yakhyo/mediapipe-hand-landmark-onnx
- https://huggingface.co/qualcomm/MediaPipe-Hand-Detection and https://huggingface.co/STMicroelectronics/hand_landmarks
- https://arxiv.org/abs/2603.16444 and https://github.com/hunainahmedj/Fast-HaMeR
- https://huggingface.co/rolpotamias/WiLoR and https://github.com/rolpotamias/WiLoR
- https://arxiv.org/abs/2510.16396 (SPLite Hand)
- https://github.com/ultralytics/ultralytics (AGPL-3.0 via the GitHub API)
