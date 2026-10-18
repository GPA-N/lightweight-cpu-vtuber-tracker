# Upper-body and hand models for CPU tracking (R1)

Date of research: 2026-10-17. Scope: 21-point hand landmarks with finger curl and out-of-frame hand handling, and upper-body pose (shoulders, elbows, wrists, hips) with 3D or world landmarks at 640x480 input. The desk-framing case is the main target: head, shoulders, elbows, wrists and hands are visible, and hips usually are not.

Budget: median under 50 ms per frame on CPU, 30 fps, 1024 MB peak working set. The current face tracker takes about 12 ms per frame.

Status key: VERIFIED means a fetched primary source states the fact. UNVERIFIED means no source was read, or the source was blocked or inconclusive. Every number below carries its source URL and access date (2026-10-17). Sources not dated in 2026 are flagged STALE and checked for a successor.

## Runtime constraint

ort 2.0.0-rc.14 bundles ONNX Runtime 1.31.0 (https://github.com/pykeio/ort/releases/tag/v2.0.0-rc.14, accessed 2026-10-17, VERIFIED). The release page names ONNX Runtime 1.31.0 and mentions no opset or ONNX spec version (VERIFIED by fetch, 2026-10-17). Opset support for 1.31 is UNVERIFIED. A conflicting rc.13 note that cites ONNX Runtime 1.28 is not reconciled. Any candidate must be loaded with ort and its opset checked before use.

Important: the MediaPipe `.task` bundles are believed to be zip containers of TFLite graphs (UNVERIFIED on the pages read). If so, ort cannot load them directly and conversion is extra work. Check this first when fetching a bundle.

## Candidates

### A. MediaPipe Pose Landmarker (Google, Tasks API)

- URL: https://developers.google.com/edge/mediapipe/solutions/vision/pose_landmarker (accessed 2026-10-17). Page footer: text CC-BY 4.0, code Apache-2.0 (VERIFIED). Model-bundle licence not stated on the page (UNVERIFIED).
- Variants: lite, full, heavy (float16 `.task` bundles). Pose detector 224x224; landmarker 256x256 (VERIFIED, same page).
- Outputs: 33 landmarks with image 2D, 3D and world landmarks, plus visibility and presence per point (VERIFIED, same page). The page's 33-point list includes hips (UNVERIFIED on the page text read; treat as a check item), so hip visibility can gate the hip weights in desk mode.
- Size: UNVERIFIED. CPU latency: UNVERIFIED on this page. Format: the `.task` container and its TFLite graph are UNVERIFIED on this page (not read); ONNX compatibility needs checking.
- Out-of-frame hands: the pose model does not cover hands in detail; it gives wrist and index-tip points only.
- Date status: the Tasks API page is current; the underlying BlazePose design dates from 2020 (STALE flag on the design, successor is this Tasks API bundle).

### B. Qualcomm MediaPipe-Pose-Estimation (Hugging Face)

- URL: https://huggingface.co/qualcomm/MediaPipe-Pose-Estimation (accessed 2026-10-17). Metadata licence Apache-2.0. Last updated 2026-10-07 (VERIFIED from the page).
- Files: pose_detector 3.14 MB float (815K parameters), pose_landmark_detector 12.9 MB float (3.36M parameters). Input 256x256 (VERIFIED, README). Upstream: zmurez/MediaPipePyTorch (VERIFIED, README).
- Warning: the repo ships QNN precompiled ONNX and context binaries for Snapdragon NPUs. The latencies in the README are NPU numbers, not CPU numbers (VERIFIED). Whether the plain ONNX graph runs on CPU in ort 1.31: UNVERIFIED.
- Keypoint count and 3D output: UNVERIFIED.
- Date status: 2026 update, but the model is the 2020-era BlazePose graph (STALE on design).

### C. RTMPose body (mmpose, 2D, 17 COCO keypoints)

- URL: https://github.com/open-mmlab/mmpose/tree/main/projects/rtmpose (accessed 2026-10-17). Accuracy: AP COCO at 256x192: t 68.5, s 72.2, m 75.8, l 76.5; at 384x288: m 77.0, l 77.3 (VERIFIED, same page). CPU speed: RTMPose-m 90+ FPS on i7-11700 CPU; GPU 430+ FPS on GTX 1660 Ti (VERIFIED, same page). Table CPU/GPU: t 300+/940+, s 200+/710+, m 90+/430+, l 50+/280+ (VERIFIED).
- Licence: mmpose code Apache-2.0 (VERIFIED, https://raw.githubusercontent.com/open-mmlab/mmpose/main/LICENSE, accessed 2026-10-17). Model-weight licence: UNVERIFIED (checkpoints are on the mmpose model zoo, licence text not read).
- Outputs: 2D only, 17 COCO keypoints. Includes hips, which are often out of frame in desk mode. No 3D, no visibility per point beyond the confidence score (UNVERIFIED on the exact score semantics).
- Top-down: needs a person box. For desk framing a fixed crop of the upper frame can stand in for a detector (ASSUMPTION, to be tested).
- Out-of-frame hands: none. Hands need RTMW (below).
- Date status: paper 2024 (STALE flag). Successor checked: RTMW and RTMW3D exist (2024-2025 era); no verified 2026 RTMPose successor found (UNVERIFIED).

### D. RTMW (whole-body, 133 keypoints, includes hands)

- URLs: https://github.com/Tau-J/rtmlib (accessed 2026-10-17; licence Apache-2.0 VERIFIED from https://raw.githubusercontent.com/Tau-J/rtmlib/main/LICENSE). RTMW paper: arXiv 2407.08634 (2024, STALE flag).
- Accuracy (rtmlib README, VERIFIED): RTMW-m 58.2 whole-body AP at 256x192; RTMW-l 66.0 at 256x192 and 70.1 at 384x288; RTMW-x 70.2 at 384x288.
- Keypoints: 133 in total, including 21 per hand (VERIFIED from the whole-body definition in the README). This is the only verified single-model option that gives hands and body together.
- CPU speed: UNVERIFIED. The RTMW3D-L 30 FPS figure is GPU-only and not applicable.
- Performance preset in rtmlib: YOLOX-m person detector plus RTMW-x (VERIFIED from https://huggingface.co/SceneWorks/dwpose-onnx, a mirror; it reports Apache-2.0 but the mirror is not primary). The detector adds CPU cost, so RTMW-m with a fixed crop is the realistic candidate.
- Out-of-frame hands: hands are predicted only when the body crop includes them. Behaviour when a hand leaves frame is UNVERIFIED.
- 3D: RTMW3D exists (VERIFIED name only; licence and CPU speed UNVERIFIED).

### E. OpenCV Zoo MediaPipe hand models (ONNX, 2023)

- URLs: https://huggingface.co/opencv/handpose_estimation_mediapipe (file handpose_estimation_mediapipe_2023feb_int8bq.onnx, 21 keypoints per detected hand) and https://huggingface.co/opencv/palm_detection_mediapipe (palm_detection_mediapipe_2023feb_int8bq.onnx). Licence Apache-2.0 (VERIFIED from the model cards, accessed 2026-10-17). Size, input, speed: UNVERIFIED.
- Outputs: 21 keypoints per hand. No world landmarks, no finger curl directly (curl must be computed from the 21 points).
- Out-of-frame hands: the palm detector scans the full frame, so a hand that enters the frame is found without a body box (VERIFIED in MediaPipe design; this model is the palm detector plus landmark regressor).
- Date status: STALE (2023 release). Successor check for a 2026 hand model: no verified 2026 hand ONNX found on Hugging Face (hand search returned zero models).

### F. MediaPipe Hand Landmarker (full)

- URL: https://developers.google.com/edge/mediapipe/solutions/vision/hand_landmarker (accessed 2026-10-17, VERIFIED): palm detector 192x192, landmarks 224x224, float16. Whole-pipeline average on Pixel 6: CPU 17.12 ms, GPU 12.27 ms (VERIFIED, same page; device is a phone, not a desktop).
- Outputs: image landmarks, world landmarks, handedness (VERIFIED). Tracking mode skips palm detection unless tracking fails (VERIFIED).
- Size: UNVERIFIED. Format: UNVERIFIED on the page read; same ONNX caveat as A.
- Out-of-frame hands: tracking mode re-runs palm detection when tracking fails, so a hand that re-enters is re-acquired (VERIFIED, same page). The full-frame scan of the palm detector is from MediaPipe design (UNVERIFIED on the page read).

### G. Ultralytics YOLO11-pose and YOLO26-pose

- Licence: AGPL-3.0 with a separate Enterprise licence for commercial use (VERIFIED, https://huggingface.co/Ultralytics/YOLO11/blob/main/README.md; AGPL text confirmed at https://raw.githubusercontent.com/ultralytics/ultralytics/main/LICENSE). Not usable in a closed product without the Enterprise licence. Excluded from the ranking.
- CPU ms (YOLO11 pose, hardware not named): n 52.4, s 90.5, m 187.3, l 247.7, x 488.0 (VERIFIED, same HF page). YOLO26-pose 640 input: n 40.3, s 85.3, m 218.0, l 275.4, x 565.4 (VERIFIED, https://docs.ultralytics.com/tasks/pose/, 2026, hardware unnamed).
- Outputs: 17 COCO keypoints, body only, no hands, no 3D. Hips included.

### H. Other candidates (not ranked)

- MoveNet (Google): UNVERIFIED. Kaggle page returned only the title. Expected 17 2D keypoints, no 3D, no hands.
- Intel human-pose-estimation-0001 (OpenVINO, 2D, 18 keypoints): the Intel original and its licence are UNVERIFIED. A 2026-10-14 FP16 re-upload by Advantech-EIOT appeared in search. Not ranked.
- PINTO0309 ONNX exports: search returned no files (UNVERIFIED).
- DWPose: licence UNVERIFIED beyond the SceneWorks mirror claim.

## Ranked top 3 (desk framing)

Ranking criteria: ONNX usable with ort, licence allows commercial use, hands and body from a path that fits the budget, and stability when hips are out of frame.

| Rank | Combo | Body | Hands | Licence (verified) | Size | CPU ms (source) | Hips out of frame | Main risk |
|---|---|---|---|---|---|---|---|---|
| 1 | RTMW-m (whole-body, fixed upper-frame crop, no detector) | 17 COCO, 2D | 21 per hand, 2D | Apache-2.0 code (mmpose, rtmlib); weights UNVERIFIED | UNVERIFIED | UNVERIFIED for RTMW-m; RTMPose-m 90+ FPS i7-11700 (body only) | Hips still output; hip score must gate weights | No CPU number for RTMW; no 3D; weights licence unread |
| 2 | Qualcomm MediaPipe-Pose (ONNX, pose detector 3.14 MB + landmark 12.9 MB) + OpenCV handpose ONNX (palm + hand, 2023) | 33 points, 2D (3D UNVERIFIED) | 21 per hand from palm + hand model | Apache-2.0 metadata (Qualcomm); OpenCV Apache-2.0 | 3.14 + 12.9 MB; hand size UNVERIFIED | UNVERIFIED on CPU; NPU numbers only | Hip visibility per point | Plain ONNX may be NPU-only; hand model is 2023 (STALE) |
| 3 | MediaPipe Pose Landmarker (lite) + MediaPipe Hand Landmarker (full), format UNVERIFIED (TFLite-to-ONNX path not checked) | 33 points, 2D + 3D world | 21 per hand, world landmarks | Apache-2.0 code; model bundle licence UNVERIFIED | UNVERIFIED | Hand pipeline 17.12 ms CPU on Pixel 6 (VERIFIED, phone); pose UNVERIFIED | Hips have visibility; world coords give 3D | Conversion path UNVERIFIED; ort 1.31 opset UNVERIFIED |

## Recommended pick

Rank 1 is RTMW-m with a fixed upper-frame crop. The reasons are: a single model gives body and both hands with 21 points each; the licences are verified as Apache-2.0 for code; and the body speed in the published table is a real CPU number. Rank 2 is the ONNX-native MediaPipe-derived path. Rank 3 needs conversion work.

Challenge to the pick: the RTMW-m pick rests on a CPU speed I did not find for RTMW itself. The published CPU number belongs to body-only RTMPose-m (90+ FPS on an i7-11700). Adding whole-body keypoints and a fixed crop may or may not keep the median under 50 ms, and this is the main open question. RTMPose is a 2024 design (STALE), and no 2026 successor was verified. The Qualcomm option is newer (2026-10-07) but its CPU behaviour is unknown and it targets NPUs. On the evidence I have, the honest verdict is that no candidate is verified against the 50 ms budget on CPU. The benchmark below decides it.

Upper-body-only modes: none of the verified candidates ships a separate upper-body checkpoint. The usable option is to run the full-body model and read only the upper-body keypoints (head, shoulders, elbows, wrists), with a crop that matches the desk view. A dedicated upper-body checkpoint was not found (UNVERIFIED search gap).

Hip handling for desk framing: all three candidates output hip points even when the hips are out of frame, so the tracker should drop hip weights when hip visibility or score is low. This is a design rule, not a verified model behaviour (ASSUMPTION until the benchmark checks it).

## Second pass: 2026 candidates, accuracy, efficiency, Pareto (2026-10-17)

Accuracy and efficiency columns. "Hand AP" is COCO-WholeBody hand AP. No source gave hand PCK or MPJPE for a 2026 model, and no source gave wrist or elbow error, so those cells are UNVERIFIED.

| Candidate (date of source) | Whole-body AP (COCO-WB) | Hand AP / hand PCK | Wrist, elbow error | CPU ms (hardware) | MB | Params / GFLOPs | Licence | Source |
|---|---|---|---|---|---|---|---|---|
| RTMW-x 384x288 (mmpose page; akore card listing 2026-03-23) | 70.2 | 0.664 hand AP | UNVERIFIED | UNVERIFIED | UNVERIFIED | GFLOPs 29.3 (params UNVERIFIED) | Apache-2.0 code (VERIFIED); weights UNVERIFIED | https://github.com/open-mmlab/mmpose/tree/main/projects/rtmpose (accessed 2026-10-17); https://huggingface.co/akore/rtmw-m-256x192 (2026-03-23) |
| RTMW-l 384x288 (mmpose; akore card) | 70.1 | 0.663 | UNVERIFIED | UNVERIFIED | UNVERIFIED | GFLOPs 17.7 | as above | as above |
| RTMW-l 256x192 (mmpose; akore card) | 66.0 | 0.598 | UNVERIFIED | UNVERIFIED | UNVERIFIED | GFLOPs 7.9 | as above | as above |
| RTMW-m 256x192 (mmpose; akore card) | 58.2 | 0.491 | UNVERIFIED | Pi 5 CPU/XNNPACK 4 threads 110 ms p50 (LiteRT build, 2026-08-31; https://huggingface.co/litert-community/RTMW-m-WholeBody-LiteRT). Pixel 8a GPU ~6 ms (LiteRT, GPU, not CPU). | 66 MB (fp16 LiteRT) | GFLOPs 4.3 | as above | as above |
| RTMPose-hand alpha 256x256 (mmpose page) | n/a | PCK@0.2 81.5 (COCO-WB-Hand) | n/a | UNVERIFIED | UNVERIFIED | 2.581 GFLOPs | mmpose Apache-2.0 code | mmpose page; pre-2025 (STALE) |
| CIGPose-x (arXiv 2603.09418, 2026-03-10) | 67.0 (67.5 with UBody) | UNVERIFIED | UNVERIFIED | UNVERIFIED | UNVERIFIED | UNVERIFIED | Not stated (UNVERIFIED); code at github.com/53mins/CIGPose | https://arxiv.org/abs/2603.09418 |
| Sapiens2 (arXiv 2604.21681, 2026-04-23) | UNVERIFIED (no WB AP on page) | UNVERIFIED | UNVERIFIED | UNVERIFIED | UNVERIFIED | 0.4 to 5 B params (far over budget) | CC-BY 4.0 (paper licence per arXiv page) | https://arxiv.org/abs/2604.21681 |
| MediaPipe Hand Landmarker (full) | n/a | UNVERIFIED | UNVERIFIED | Pixel 6: CPU 17.12 ms, GPU 12.27 ms (phone) | UNVERIFIED | UNVERIFIED | Apache-2.0 code | https://developers.google.com/edge/mediapipe/solutions/vision/hand_landmarker |
| Qualcomm MediaPipe-Pose (HF, 2026-10-07) | UNVERIFIED | UNVERIFIED | UNVERIFIED | NPU numbers only; CPU UNVERIFIED | 3.14 + 12.9 | 0.815 M + 3.36 M params | Apache-2.0 metadata | https://huggingface.co/qualcomm/MediaPipe-Pose-Estimation |
| OpenCV handpose (2023, STALE) | n/a | 21 kp per hand, accuracy UNVERIFIED | n/a | UNVERIFIED | UNVERIFIED | UNVERIFIED | Apache-2.0 | https://huggingface.co/opencv/handpose_estimation_mediapipe |

Notes on the table:
- The akore HF repo is a transformers port (`trust_remote_code`), not an ONNX file. Its README gives the AP table above and is dated by the listing (2026-03-23). Its licence is apache-2.0 on the card (VERIFIED from card).
- The LiteRT card (2026-08-26 to 08-31) gives device numbers only. Its own text says XNNPACK declined the fp16 graph on Android. The Pi 5 row is a CPU number that conflicts with "XNNPACK declined" and should be re-run on a named desktop before use.

Pareto (accuracy against CPU cost, verified numbers only):
- Most accurate overall: RTMW-x 384x288, 70.2 whole-body AP, 29.3 GFLOPs. Out of CPU budget (no CPU timing, and the cost is far above RTMW-m).
- Most efficient overall: RTMW-m 256x192, 58.2 AP at 4.3 GFLOPs. Its only CPU number is 110 ms on a Raspberry Pi 5 (LiteRT build), which is over the 50 ms budget on that device.
- Best tradeoff under budget (extra 15 ms or less, under 200 MB): NOT FOUND. No candidate has a verified desktop CPU time under 50 ms. RTMW-m is the only candidate that fits the GFLOPs range, and its CPU time is decided by the benchmark.

Deployability on ort 2.0.0-rc.14 (CPU), with the conversion each needs:
- RTMW-m / RTMW-l (ONNX): ONNX files exist on Hugging Face (bukuroo/RTMW-ONNX, 2025-09; listing only, format and licence unread). Loadable on ort CPU is UNVERIFIED (opset UNVERIFIED). akore needs a PyTorch export to ONNX.
- RTMW-m LiteRT (litert-community, 2026-09-08): not loadable by ort. Needs a TFLite-to-ONNX conversion (UNVERIFIED path).
- RTMW-m ExecuTorch (mlboydaisuke, 2026-08-23): not loadable by ort. Needs an ExecuTorch-to-ONNX conversion (UNVERIFIED path).
- Qualcomm MediaPipe-Pose: ships QNN precompiled ONNX for Snapdragon NPUs. Plain-ONNX CPU load in ort 1.31 is UNVERIFIED.
- OpenCV handpose ONNX (2023): loadable in ort on paper; opset and speed UNVERIFIED.
- MediaPipe Tasks (.task): TFLite inside. Needs conversion (UNVERIFIED).
- CIGPose and Sapiens2: PyTorch code. ONNX export needed; no ONNX build found.

Challenge to the pick, updated: the 2026 LiteRT card gives RTMW-m a CPU number of 110 ms on a Raspberry Pi 5 (4 threads). That is over the budget on that device. A desktop CPU may be faster, but no verified desktop number exists. The rank-1 pick is therefore provisional. If the benchmark on the desktop CPU shows RTMW-m over 50 ms median at 1 thread, the pick falls and no candidate is left that is verified under budget.

Search gaps (2026): no 2026 RTMW successor with an ONNX build and CPU timing was found; no 2026 hand ONNX model was found on HF (search returned zero hand models); PINTO0309 files and MoveNet were not found.

## Benchmark plan (crates/bench)

Input: fixtures/private/golden_take/f*.jpg (236 camera frames; the earlier count of 706 was wrong. Handling of av_*.png is UNVERIFIED and they are excluded from the frame count). Use the sorted order as a frame stream at 640x480, matching the camera mode in crates/bench (CAMERA_WIDTH 640, CAMERA_HEIGHT 480, CAMERA_FPS 30).

Steps:
1. Add an upper-body TrackStep to the bench pipeline (the existing `TrackStep` trait in crates/bench/src/pipeline.rs takes a `Frame` and returns `TrackingOutput`). Keep the face tracker out of this path.
2. Load each candidate with `infer::session::build_session` and the ort version from Cargo (1.31 per the rc.14 release page). Record the opset error if any.
3. Run one pass over all 236 frames per threads setting (1, 2, 4). Use `--threads` from the bench CLI.
4. Report: median ms per frame, p95, fps achieved, and peak working set from `timing::memory::process_memory`. Pass if median is under 50 ms, fps is at least 30 on the threads setting, and peak is under 1024 MB.
5. Check hip behaviour: count frames where hip visibility or score is below a threshold. Report the stability of shoulder and elbow positions over frames (frame-to-frame jitter in pixels), which tests whether the tracker stays put when hips are missing.
6. Check hands: count frames with a hand detected, and frames where a hand is near the frame edge. No ground truth labels are known for golden_take (UNVERIFIED that labels exist); if none exist, use the detector count and jitter as proxies and say so.
7. Run `bench --check-image <path>` on one frame per candidate for a first-run sanity check before the timed run.

Stop rule: if no candidate reaches a median under 50 ms at 1 thread, report that and do not pick a model on speed alone.

## Open items

- RTMW CPU fps (not found).
- MediaPipe pose and hand bundle sizes and CPU latency (not on the page).
- Qualcomm plain ONNX CPU support and keypoint count.
- MoveNet, Intel human-pose-estimation-0001, PINTO0309 files.
- Model-weight licences for RTMPose, RTMW and DWPose.
- 2026 successors for RTMPose and the 2023 OpenCV hand models: none verified.
- ort 1.31 opset support for every candidate.
- Repo LICENSE for rtmlib and mmpose: now verified as Apache-2.0 at raw.githubusercontent.com (accessed 2026-10-17).
