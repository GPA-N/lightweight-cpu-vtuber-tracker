# Hand search, angle B: hubs and repositories (task H2b)

Date: 2026-10-17. Nothing was downloaded or executed, and no model was run. Sources are Hugging Face listings and model cards, the GitHub API (authenticated as `GPA-N`, no rate limit hit), and the Kaggle CLI listing command. Items I could not read are marked UNVERIFIED.

Target (from the user): small, fast, current (2026 upload or retrain, not a 2026 re-upload of 2023 weights), CPU through ONNX, about 3 to 10 ms per hand, under 30 MB, 21 keypoints, 3D or relative depth, handedness, fists and occlusion handled, permissive licence.

## Verdict

**None accepted.** I found no model that is 2026-trained, 21-keypoint with depth and handedness, small, ONNX and permissive at the same time. Every candidate fails at least one hard criterion. The two models that are truly 2026-trained fail on licence (YOLO26 hand pose, AGPL) or licence and domain (frametop, CC-BY-NC, infrared fisheye). Every permissive 21-point 3D model traces back to Google MediaPipe weights from 2020 to 2023. This confirms `hand-models-2026.md` and closes its open gap: the PINTO model zoo was checked and holds nothing new for hands.

## What I searched

| Hub | Method | Result |
|---|---|---|
| Hugging Face | Queries `hand landmark`, `hand pose`, `hand keypoint`, `hand tracking`, `mediapipe hand`, `3d hand`, `rtmpose hand`, `handpose`, `hand landmarker onnx`, `hand mesh`, `palm detection`, `wilor`, `hamer`, `rtmw`, `mobrecon`, `wholebody pose`, plus the `keypoint-detection` filter with `hand`. Sorted by last modified or created | Table below. About 10 relevant repos, none a new lightweight model |
| GitHub | 16 topic and keyword queries (`hand-pose`, `hand-landmarks`, `hand-tracking`, `hand-pose-estimation`, `hand-keypoints`, `hand-landmark-detection`, `3d-hand-pose`, `hand-detection`, `mediapipe-hands`, `mano`, `hand landmark onnx`, and so on) with `pushed:>2026-03-01`, about 190 distinct repos | Almost all are applications calling MediaPipe. Eight repos worth a look, table below |
| PINTO0309 | Full top-level tree of `PINTO_model_zoo` (509 folders, pushed 2026-10-17), per-folder last-commit dates, and the owner's 2025-06 to 2026-10 repo list | No new hand landmark model. Details below |
| Qualcomm AI Hub | `qualcomm/MediaPipe-Hand-Detection` card (v0.64.0) | Google weights through a 2020 PyTorch port. Details below |
| Kaggle Models | CLI `models list --search hand` sorted by creation | Nothing relevant. `tarungupta05/hand-landmarker` has no description and is UNVERIFIED (probably the MediaPipe bundle). Others are sign-language classifiers on MediaPipe landmarks |
| ONNX Model Zoo, OpenVINO Open Model Zoo, ncnn, MNN, TNN | Repository metadata (pushed 2026-10-03, 2026-10-15, 2026-10-17, 2026-10-16, 2025-05-09) | Active repos, but I found no hand landmark model added in 2026. UNVERIFIED for the full file lists (I did not walk every folder). The ncnn weights found are community fine-tunes (frametop) |
| MMPose | `open-mmlab/mmpose` | Last push 2025-08-04, last release v1.3.2 (2024-07-12). No 2026 hand model |
| Apple Vision, Google ML Kit | Not usable (closed, platform bound). Both are MediaPipe-class on-device models | Noted only |

## Candidates

Weights lineage is the deciding column. "Date" is the date of the file or card on the hub, not the training date, unless stated.

| # | Candidate | URL | Licence | Size / input | Outputs | Hub date | Weights lineage | ONNX | Speed (source) | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | MediaPipe hand landmark via yakhyo export | https://github.com/yakhyo/mediapipe-hand-landmark-onnx | Apache-2.0 | landmark about 7.7 MB fp32 (2.01 M params, Qualcomm card), 224x224 | 21 x (x, y, z), presence, handedness, world points | pushed 2026-07-28 | Google 2021 to 2023, via PINTO0309 conversion (read in H1) | yes | none on CPU | Fails currency. Meets every other criterion. Still the only fit |
| 2 | Qualcomm `MediaPipe-Hand-Detection` | https://huggingface.co/qualcomm/MediaPipe-Hand-Detection | Apache-2.0 on the card, but the card points to the licence of `zmurez/MediaPipePyTorch` | detector 6.75 MB (1.76 M params), landmark 7.70 MB (2.01 M params), 256x256 | same family | modified 2026-10-07, ai-hub-models v0.64.0 | Card names `zmurez/MediaPipePyTorch` as the source, a PyTorch port of the Google 2020 to 2021 models. Re-export only. Not retrained | yes (float zip, QAIRT 2.50, ORT 1.30.0) | NPU only: landmark 0.50 to 2.05 ms. No desktop CPU figure | Fails currency. Same weights as #1 |
| 3 | YOLO26 hand pose `marceloeatworld/yolo26-training` and `poptoz/yolo26-hand-pose-face-detection` | https://github.com/marceloeatworld/yolo26-training and https://huggingface.co/poptoz/yolo26-hand-pose-face-detection | Not stated. Base is Ultralytics YOLO26s-pose, which is AGPL-3.0 (verified in H1). Training data is the Ultralytics hand-keypoints set | 20.5 MB ONNX, YOLO26s-pose, 10.6 M params, 25.0 GFLOPs | 21 x 2D keypoints and box, `(1, 300, 69)`. No depth, no handedness | 2026-04-03 | **Truly 2026-trained** by the author, from the COCO-pose YOLO26s checkpoint | yes | not given (25 GFLOPs is far above the 3 to 10 ms goal on CPU) | Rejected: AGPL, 2D only, no handedness, too heavy. Pose mAP50 0.942, mAP50-95 0.843 on the Ultralytics set |
| 4 | RTMPose-Hand (hand5 alpha) in LiteRT and ExecuTorch builds | https://huggingface.co/litert-community/RTMPose-Hand-LiteRT and https://huggingface.co/mlboydaisuke/RTMPose-m-Hand-ExecuTorch | Apache-2.0 | 256x256, 28 MB fp16 (LiteRT) | 21 x 2D (SimCC) | 2026-06-28 and 2026-08-14 | MMPose 2023 weights, re-exports. Official ONNX exists upstream, and the PINTO zoo folder `427_RTMPose_Hand` was last committed 2023-12-24 | upstream ONNX only | 58.1 ms on Raspberry Pi 5, 4 threads (H1) | Fails currency, depth and handedness |
| 5 | STMicroelectronics `hand_landmarks` | https://huggingface.co/STMicroelectronics/hand_landmarks | Apache-2.0 | int8 TFLite, 224x224 | (x, y, conf) | 2026-01-23 | PINTO0309 conversion of Google weights | no | 20.75 ms on an STM32N6 NPU | Fails currency, format |
| 6 | `opencv/handpose_estimation_mediapipe`, `kiruso/palm_detection_mediapipe`, `webnn/mediapipe_hand`, `Leo-TX/mediapipe-hand`, `OpenReasonAI/mediapipe-hands-int8`, `hugging-mac/mediapipe-hand-coreml` | https://huggingface.co/opencv/handpose_estimation_mediapipe (and siblings) | Apache-2.0 or unstated (`hugging-mac` says `other`) | MediaPipe sizes | MediaPipe outputs | 2025-06 to 2026-08 | All repackage Google MediaPipe weights. `kiruso` palm detector (created 2026-08-26) is OpenCV Zoo style | yes (`opencv`, `kiruso`), tflite or Core ML (others) | none | Re-uploads of 2023 weights. Not current |
| 7 | `DeeJanuz/frametop-hands-models` | https://huggingface.co/DeeJanuz/frametop-hands-models | **CC-BY-NC-4.0** | ncnn, size not read | hand keypoints for infrared fisheye headset cameras | created 2026-10-14, modified 2026-10-17 | **Real 2026 fine-tune** of `opencv/handpose_estimation_mediapipe` on its own IR dataset | ncnn only | none | Rejected: non-commercial, wrong camera domain, still MediaPipe-derived, no ONNX |
| 8 | `ryhara/hand-visibility-detector` (arXiv 2608.11574) | https://huggingface.co/ryhara/hand-visibility-detector and https://github.com/ryhara/hand_visibility_detector | Model CC-BY-NC-4.0 (Hub tag). GitHub licence field empty | backbones: CSPNeXt, ResNet, ViT, HaMeR, AnyHand (added 2026-10-14) | per-keypoint visibility flags, not landmarks | 2026-08-13 | Fine-tune of `warmshao/WiLoR-mini` (WiLoR, MANO) | no | none | Rejected: visibility only, non-commercial, MANO base. Useful idea for occlusion flags, nothing to ship |
| 9 | WiLoR / HaMeR and Hub mirrors (`rolpotamias/WiLoR`, `BasinShapers/wilor-mlx`, `lyonsno/wilor-mlx`, `changh95/hamer-weights`) | https://huggingface.co/rolpotamias/WiLoR | WiLoR weights CC-BY-NC-4.0. HaMeR code MIT. MANO is non-commercial. MLX ports tag MIT but inherit the base | ViT scale | MANO mesh | 2025-11 to 2026-09 | 2024 to 2025 research weights | community only (`warmshao/WiLoR-mini`, 2024) | GPU class. `GraspRT-Egohand-Perception-Bench` (2026-08-30) measured WiLoR-fast 33.5 ms on a 6 GB GPU, MediaPipe on CPU 34.35 ms for its pipeline | Rejected, as in H1. Useful detail: in that bench, MediaPipe CPU got 15.9 mm PA-MPJPE on FreiHAND against 5.7 to 5.9 mm for WiLoR, and a MobRecon run gave 24.8 mm |
| 10 | RTMW / RTMW3D whole-body (`akore/rtmw-*` 2026-03-23, `litert-community/RTMW-m-WholeBody-LiteRT` 2026-06-28, `bukuroo/RTMW3D-ONNX` 2025-09-03, `Izymka/rtmw-dw-x-l_simcc-cocktail14` 2026-05-27) | https://huggingface.co/bukuroo/RTMW3D-ONNX | Apache-2.0 (RTMW3D); MIT tag on the Izymka upload | 62 to 369 MB | 133 keypoints, RTMW3D adds depth | 2025-09 to 2026-06 | 2023 to 2024 MMPose weights, re-uploads | yes | none | Rejected: over 30 MB, hands are the weak part (see `hand-search-C.md`) |
| 11 | PINTO0309 `488_DEIMv2-Wholebody49` and `hand_visibility_detector` fork | https://github.com/PINTO0309/PINTO_model_zoo/tree/main/488_DEIMv2-Wholebody49 | MIT (repo), weights terms not read | detector class | **boxes** for body parts, not 21 keypoints | last commit 2026-06-07 | Author-trained 2026 on his own data | yes | GPU numbers only | Rejected: detection only, no finger landmarks. PINTO `incomplete-wholebody69` is non-commercial |
| 12 | `Konoa-1025/PipeTRT` | https://github.com/Konoa-1025/PipeTRT | Not read (field shown as unspecified in search) | TensorRT engines | 21 landmarks | pushed 2026-08-15 | MediaPipe-style, weights from MediaPipe | TensorRT only | NVIDIA GPU | Rejected: GPU only |
| 13 | `beapoquiz/heterogeneous-kd-2d-hand-pose-estimation` | https://github.com/beapoquiz/heterogeneous-kd-2d-hand-pose-estimation | None | student model from knowledge distillation | 2D | pushed 2026-08-19, 0 stars | Undergraduate thesis | not read | "real-time" claim not checked | Rejected: unlicensed, 2D, unproven. Only evidence that a distilled 2D student is a known recipe |
| 14 | `renesas-rdk/hand_models` | https://github.com/renesas-rdk/hand_models | None | RTMDet-Nano 2 MB and RTMPose recipes | conversion instructions | pushed 2026-09-11 | MMPose 2023 | recipe | none | Not a model release |

## PINTO model zoo, the gap from H1

The H1 note could not list the PINTO zoo because of the rate limit. I read the full tree this time.

- Hand-related folders: `027_minimal-hand` (last commit 2023-06-27), `033_Hand_Detection_and_Tracking` (2024-01-23), `094_hand_recrop` (2023-06-26), `403_trt_pose_hand`, `420` to `445` Gold-YOLO and YOLOX body-head-hand detectors, `427_RTMPose_Hand` (2023-12-24, README is only a video link), `393_RTMPose_WholeBody`.
- The newest folders (`492` to `506`) are face and head classifiers, line detection, re-identification and document restoration. None is a hand model.
- The newest wholebody work (`488_DEIMv2-Wholebody49`, last commit 2026-06-07) is an object detector with hand boxes and no finger keypoints. The README says the final weights suffered a training bug and the author does not recommend the S and N variants.
- Result: the `033` folder holds the 2021 to 2023 MediaPipe conversions that the yakhyo export builds on. No 2026 hand landmark model exists in the zoo.

## Qualcomm AI Hub, lineage check

The `qualcomm/MediaPipe-Hand-Detection` card (modified 2026-10-07, v0.64.0) says "based on the implementation of MediaPipe-Hand-Detection found at `zmurez/MediaPipePyTorch`" and lists the paper arXiv 2006.10214 (MediaPipe Hands, 2020). The card offers an ONNX export built for ONNX Runtime 1.30.0 and QAIRT 2.50. That is a 2026 export of 2020 to 2021 model weights. The date on the page is not a training date. The card gives NPU timings only (landmark model 0.5 ms to 2.05 ms across chipsets); on a desktop CPU the speed is not published.

## Ranking and challenge

1. **yakhyo MediaPipe hand landmark ONNX (#1)**. Best fit on size (7.7 MB), outputs (21 points with z, handedness, world points), licence and ONNX status. Only fails the currency rule, and it matches the face and pose weights already in the pipeline.
2. **Qualcomm export (#2)**. Same weights, nothing to add.
3. **RTMPose-Hand (#4)**. Best 2D accuracy, 2D only, 2023 weights.
4. **YOLO26 hand pose (#3)**. The only permissively-sized, truly 2026-trained ONNX with 21 points I found. It fails on AGPL, 2D only, no handedness, 25 GFLOPs. It would only become usable if the project buys an Ultralytics enterprise licence.

Challenge to my own result:

- **Is a missing model proof of absence?** No. Hub search matches on repository names and tags, and many private or untagged releases do not appear. The GitHub search matches topics and descriptions. A 2026 paper release with weights on a lab website or Google Drive would not show up in this angle. Angles A and C cover papers.
- **Could a 2026 re-upload hide a retrain?** I checked the cards of #1, #2, #5 and #6 for training dates and lineage. All point to Google weights. The only real 2026 training I found is #3 (YOLO26), #7 (frametop fine-tune) and #8 (visibility fine-tune), and none passes the licence test.
- **Is the licence reading firm?** The YOLO26 licence is inferred from the base model (Ultralytics repo AGPL-3.0 verified in H1), because the author's repo does not state one. The `ryhara` GitHub repo shows no licence, and the Hub model is CC-BY-NC-4.0.
- **Speed.** I found no desktop CPU latency for any 21-point model. The GraspRT bench measured the whole MediaPipe pipeline (detector and crop included) at 34.35 ms on CPU in its own setup, which is not the 3 to 10 ms goal for the landmark network alone. Treat this as an unverified risk. Task 1.3 must measure.
- **Fists and occlusion.** No hub model card publishes a fist or occlusion number. The GraspRT bench numbers (15.9 mm for MediaPipe against 5.7 mm for WiLoR) show MediaPipe is weaker, but the dataset is FreiHAND (hand crops without heavy occlusion). Our own fixtures decide.

## Suggested next step for the lead

Take option 1 from `hand-models-2026.md` (MediaPipe weights through the yakhyo export), record the currency exception, and spend the effort on the hand ROI and the palm detector, which `hand-search-C.md` identifies as the likelier cause of the failures. If the user insists on 2026-trained weights, the only legal route in this search is to train a student on the cloud (the recipe in `hand-search-C.md`); no downloadable 2026 model gets there.

## Sources

- https://huggingface.co/qualcomm/MediaPipe-Hand-Detection (card read in full)
- https://github.com/yakhyo/mediapipe-hand-landmark-onnx
- https://github.com/marceloeatworld/yolo26-training (README read) and https://huggingface.co/poptoz/yolo26-hand-pose-face-detection
- https://huggingface.co/DeeJanuz/frametop-hands-models
- https://huggingface.co/ryhara/hand-visibility-detector and https://github.com/ryhara/hand_visibility_detector
- https://github.com/eddisonpham/GraspRT-Egohand-Perception-Bench
- https://github.com/PINTO0309/PINTO_model_zoo (tree, commits per folder, `488_DEIMv2-Wholebody49/README.md`)
- https://github.com/open-mmlab/mmpose (releases, commits)
- https://github.com/renesas-rdk/hand_models, https://github.com/Konoa-1025/PipeTRT, https://github.com/beapoquiz/heterogeneous-kd-2d-hand-pose-estimation
- Hugging Face listings: litert-community/RTMPose-Hand-LiteRT, mlboydaisuke/RTMPose-m-Hand-ExecuTorch, STMicroelectronics/hand_landmarks, opencv/handpose_estimation_mediapipe, kiruso/palm_detection_mediapipe, bukuroo/RTMW3D-ONNX, akore/rtmw-*
- `docs/notes/hand-models-2026.md` and `docs/notes/hand-search-C.md` (earlier results)
