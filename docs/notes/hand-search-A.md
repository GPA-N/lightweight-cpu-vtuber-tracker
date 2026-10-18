# Hand landmark search, angle A (papers and releases), task H2a

Date of check: 2026-10-17. No model was run, no code was executed, nothing was downloaded except HTTP headers. Sources: alphaXiv paper reports, arXiv abstract pages, the Hugging Face and GitHub public APIs, repo READMEs. Builds on `docs/notes/hand-models-2026.md` (H1).

## Verdict

**No clean ACCEPTED candidate.** Nothing found meets all of: 2026-dated, under 30 MB, 21 keypoints with depth and handedness, CPU-class speed with a published number, permissive licence, weights downloadable.

**One CONDITIONAL candidate, 2D only:** `tasmulaev/rtmpose-m-distill` (HF, created 2026-07-03, Apache-2.0). It passes the literal "2026-dated release" test and the "2D is acceptable" clause, but it is a 2026 fine-tune of 2023 RTMPose-m weights, it is 55 MB in fp32 (about 28 MB if converted to fp16), it has no ground-truth accuracy number, and it has no published CPU time. See the challenge section. It should be treated as a candidate to benchmark, not as a pick.

**SPLite Hand (the item the previous researcher left unevaluated): REJECTED.** No code, no weights, no licence, and it is Oct 2025. Details below.

The honest summary is the same as H1: the permissive, small, 3D, handedness-bearing option is still Google's 2023 MediaPipe hand landmark model, now re-packaged in 2026 by several people.

## Ranked table

Rank is by fit to the brief. "Weights ok" means a file was confirmed downloadable (HTTP 200) or a repo listing shows the file. Sizes and dates are from the HF or GitHub API.

| # | Candidate | Source and date | Weights ok | Licence | Size / params | Speed (reported) | Accuracy (reported) | Outputs | ONNX | Result |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `tasmulaev/rtmpose-m-distill` | HF created 2026-07-03, base RTMPose-m hand5 (arXiv 2303.07399, 2023) | Yes: `onnx/rtmpose-m-distill-256x256.onnx`, HEAD 200, 55,118,513 bytes | Apache-2.0 for weights; training data Slovo is a CC BY-SA 4.0 variant | 55 MB fp32, 256x256, base is 2.58 GFLOPs | none | PCK@0.2 0.9893 against its own teacher labels, not ground truth; hand retention on blurred sign video 98.0 % against 93.1 % for the base | 21 x 2D (SimCC), no depth, no handedness | Yes, opset 11, parity check shown | CONDITIONAL, 2D only |
| 2 | MediaPipe hand landmark via `yakhyo/mediapipe-hand-landmark-onnx` and similar repacks (`svenflow/micro-handpose` 2026-03, `jmjHappy/inference-engine-blaze-hand` 2026-05) | Google 2023 weights, repacked 2026 | Yes | Apache-2.0 | landmark full about 7.7 MB fp16 weights (micro-handpose README), 224x224 | micro-handpose claims 4.1 ms on WebGPU against 11.4 ms for MediaPipe on GPU, not CPU | MediaPipe 3D MPJPE 130.8 mm on AssemblyHands zero-shot with its own 2D points (TransHands Table 5) | 21 x 3D, handedness, presence | Yes | Fails the currency rule, passes everything else. Depth is weak |
| 3 | RTMPose-Hand (hand5) re-exports: `litert-community/RTMPose-Hand-LiteRT` 2026-06-28, `mlboydaisuke/RTMPose-m-Hand-ExecuTorch` 2026-08-14 (fp32 55.1 MB `.pte`, CoreML 28.0 MB `.pte`) | 2023 weights | Yes | Apache-2.0 | 28 to 55 MB | 58.1 ms on Pi 5, 4 threads (from H1) | PCK@0.2 96.4 Hand5, 81.5 COCO-WholeBody-Hand | 21 x 2D | Official ONNX (older) | Same weights as rank 1 without the 2026 fine-tune. 2023 |
| 4 | SPLite Hand, arXiv 2510.16396 (v3 2025-10-30, AICCC Dec 2025) | 2025-10 | **No release found** | none stated | ResNet-18 sparse, 8.79 M params, 72 MB fp32 / 18 MB int8 QAT | 15 FPS on Raspberry Pi 5 CPU (about 67 ms); quarter-channel decoder 0.72 ms | FreiHAND PA-MPJPE 9.0 mm fp32, 9.1 mm int8; worse than SimpleHand (5.8 mm) | MANO-style mesh and joints, input 128x128 edge image | QAT model "converted to ONNX" per the paper, not published | REJECTED |
| 5 | ReJSHand, arXiv 2503.05995 | 2025-03 | No weights listed in repo `daishipeng/ReJSHand` | none stated | 1.91 M head params on a FastViT-MA36 backbone, 224x224 | 72 FPS on an RTX 2080 Ti GPU | FreiHAND PA-MPJPE 6.3 mm | 2D, 3D joints, 778-vertex mesh via the MANO joint matrix | no | REJECTED: date, no weights, MANO |
| 6 | MobRecon (HandMesh) | CVPR 2022 | Yes, Google Drive | MIT code, MANO file required | DenseStack | 80 FPS GPU (ReJSHand table); 6 FPS on Pi 5 (SPLite table) | FreiHAND 6.9 mm | mesh | none | REJECTED: date, MANO |
| 7 | Fast-HaMeR (arXiv 2603.16444) | 2026-03 | Student checkpoints not confirmed (README says train them yourself) | MIT code, MANO non-commercial | MobileNet-large and MobileViT-small students exist in configs | "1.5x faster", no hardware, no CPU number | HO3D-v2 PA-MPJPE 8.1 mm (ConvNeXt-L) | MANO | none | REJECTED: no weights, MANO, no ONNX |
| 8 | TransHands, arXiv 2608.22341 (2026-08-23) | 2026-08 | No code or weights link found | none stated | 67.7 M | none | Re:InterHand MPJPE 7.2 to 8.0 mm; AssemblyHands zero-shot 93.0 mm MPJPE / 23.8 PA | 2D hand points to 3D, sequence model | none | REJECTED: no weights; but see "Idea worth keeping" |
| 9 | Hand Visibility Detector, arXiv 2608.11574 (2026-08-12), repo `ryhara/hand_visibility_detector` pushed 2026-10-13 | 2026 | Repo exists | none listed | 631 M parameters (frozen WiLoR ViT) | none | mAP 0.931, F1 0.896 on HInt | per-joint visibility only | none | REJECTED: ViT-H class. Task is useful, model is not |
| 10 | CS-ViT, arXiv 2609.24424 (2026-09-21, ACM MM 2026) | 2026-09 | Unclear: `checkpoints` folder, contents not visible | none listed | ViT, size not stated | not stated | up to 37.1 % better CS-MJE than SOTA on HO3D | camera-space 3D | none | REJECTED: ViT, no licence, no speed |
| 11 | ScaleHP, arXiv 2606.25619 (2026-06-24) | 2026-06 | No link | none | transformer decoder | not reported | SOTA FreiHAND CS-MPJPE | camera space | none | REJECTED: no code |
| 12 | `nvidia/RT-DETR-Hand-Detector-v1` (HF 2026-09-15) | 2026 | Gated (login and contact share) | NVIDIA Software and Model Evaluation License, "demonstration, not production" | about 20 M params, 640x640 | no CPU number | none | hand boxes only, left/right classes | tagged ONNX | REJECTED: licence, detector only |
| 13 | `DeeJanuz/frametop-hands-models` (HF 2026-10-14) | 2026-10 | ncnn fp16 | **CC BY-NC 4.0** | MediaPipe graphs unchanged | none | self-reported, "a quarter closer" on held-out, IR fisheye data | 21 points | ncnn only | REJECTED: non-commercial, IR domain |
| 14 | Others seen and dismissed | `poptoz/yolo26-hand-pose-face-detection` (YOLO26, AGPL base, 2026-04), `marceloeatworld/yolo26-training` (same), `NightingaleCen/YOLO26m-seg-hand` (AGPL-3.0), `amithroy/lazarus-hand-tracking` (MIT, a gesture classifier with 120 us joblib, not a landmark model), `renesas-rdk/hand_models` (conversion notes, no licence), WatchHand (arXiv 2602.21610, smartwatch sound, not RGB), WristP2 (arXiv 2603.00606, fisheye wrist camera) | | | | | | | | |

Searches that returned nothing relevant: Meta, Apple, Qualcomm, Samsung and ByteDance 2026 hand models (Qualcomm only lists MediaPipe hand detection, 2024, and a gesture recognition model, 2026-02-12, both from MediaPipe); alphaXiv discover_papers over four phrasings from 2025-10 to 2026-10 returned only ViT, multi-view, event, depth, egocentric-video and diffusion papers; HF searches for `hand`, `landmark`, `hand pose` with ONNX filter returned the list above.

## SPLite Hand, full evaluation

Read in full through alphaXiv (arXiv 2510.16396v3, 2025-10-30, National Tsing Hua University, AICCC 2025, Tokyo, December 2025).

- **Weights and code:** none. The paper gives no repository URL. A web search for a repo found none. The paper's own dataset is also not linked.
- **Licence:** none stated. ACM copyright notice only.
- **Date:** October 2025, so it fails the 2026 rule even if weights appeared.
- **Size:** 8.79 M parameters, 71 to 72 MB fp32, 18 MB after int8 quantization-aware training.
- **Speed:** 15 FPS on a Raspberry Pi 5 CPU (Cortex-A76, 4 cores), so about 67 ms per hand. Comparison rows in the same table: MobileHand 35 FPS, MobRecon 6 FPS, SimpleHand 4 FPS. A laptop with 12 threads would be faster than a Pi 5 by an unknown factor; I will not guess it.
- **Accuracy:** FreiHAND PA-MPJPE 9.0 mm fp32 and 9.1 mm int8. That is weaker than the 2024 and 2025 mesh models (SimpleHand 5.8 mm in its table, ReJSHand 6.3 mm).
- **Odd design points that block a drop-in use:** input is a 128x128 edge map from "a proprietary algorithm" (Canny or Sobel claimed equivalent), not RGB; it needs Minkowski sparse convolution, which is not an ONNX operator; the speed-up was measured with a custom operator; the output is a mesh built from a MANO-style lifting matrix, so MANO licensing probably comes with it; the SPLite decoder indexes vertices by spiral sampling, which is a gather pattern that may export but is unproven.
- **Verdict:** rejected on five counts: no weights, no licence, wrong date, not exportable as stated, mediocre accuracy. It is recorded so nobody re-checks it.

## What the numbers say about the user's target (3 to 10 ms, under 30 MB, 21 points)

- Nothing published in 2026 hits that box. The fast 2D models (RTMPose-m hand, MediaPipe) are 2023. The 2026 models are ViT-scale (CS-ViT, ScaleHP, HaMeR family) or have no weights.
- The only CPU-class numbers in the 2026 literature are for a Pi 5, at 58 to 67 ms per hand. They say nothing reliable about a 12-thread laptop.
- Depth is the weak part of the only small permissive 3D model. TransHands reports MediaPipe 3D at 130.8 mm MPJPE on AssemblyHands zero-shot, against 93.0 mm for a 67.7 M lifting network fed the same MediaPipe 2D points. Both numbers are poor in absolute terms (different dataset and camera), so I would not expect usable absolute depth from MediaPipe; relative finger curl should still work, and our task 4.3 only needs curl.

## Idea worth keeping

TransHands shows that 2D points from an off-the-shelf detector can be lifted to 3D with a separate network (MotionBERT-sized, 67.7 M, so too large). The same concept at a tiny scale would be a small MLP lifting 21 2D points from RTMPose-Hand to relative 3D. It needs training on a 3D hand dataset (FreiHAND, HO3D, InterHand2.6M, DexYCB; licences differ and need checking) in the cloud, and it is not released anywhere I could find. This is the same build option as the distilled student in `hand-search-C.md`; I did not cost it here.

## Challenge to the top pick (`tasmulaev/rtmpose-m-distill`)

- **Is it really 2026?** The repo was created 2026-07-03. The architecture, the starting weights and the base training are 2023 (arXiv 2303.07399). What is new is a 100-epoch fine-tune on one GPU, by one author, on one Russian Sign Language dataset (Slovo, 300,238 crops). A strict reading of "current model" says no. A literal reading of "2026-dated release" says yes. The user must choose.
- **Is the accuracy claim real?** The 0.9893 PCK is measured against the model's own pseudo-labels, so it only shows the student copies the teacher. The useful claim is the 98.0 % against 93.1 % hand retention on blurred sign video, which is one 5,100-frame clip and a detection-rate metric, not a position-error metric. There is no number on FreiHAND, COCO-WholeBody-Hand, or Hand5. A fine-tune on one studio-style dataset (frontal upper body, 194 signers) can lose accuracy elsewhere; the card says it is "visually indistinguishable" on clean frames, which is not a measurement.
- **Does it handle fists and occlusion?** Unknown. No fist metric exists for it or for the base model. Our own fixtures must test it.
- **Size:** 55 MB fp32 does not meet the 30 MB limit. fp16 conversion gives about 28 MB, which is inside the limit but untested for accuracy; int8 is a larger risk for SimCC heads.
- **Speed:** no number. The base model is 2.58 GFLOPs at 256x256, and RTMPose-m body (1.93 GFLOPs) takes 11 ms on an i7-11700 per the mmpose table, so I expect 10 to 15 ms for this model, outside the 3 to 10 ms target. This is an inference from FLOPs, not a measurement.
- **Licence trap:** the weights are tagged Apache-2.0, but they were trained on Slovo, whose licence is a CC BY-SA 4.0 variant. Whether share-alike reaches model weights is unsettled; the author licenses the weights Apache-2.0 and does not ship the data. Low risk for a personal tool, a question for a shipped product. Also, the base RTMPose-m hand5 weights were trained on datasets with their own terms (Hand5 mixes several hand sets); the previous note did not audit that either.
- **No depth, no handedness:** needs a second source for depth (the pose model, or a lifting step) and for handedness (pose side of the wrist).
- **Single-author, one-like repo:** low bus factor. If it disappears, the 2023 official RTMPose-m hand5 gives the same architecture.

If I am wrong about anything, I am most likely wrong about the speed: it could be faster with fp16 or a smaller crop, and it could be slower with ONNX Runtime thread contention next to the face and pose graphs.

## Recommendation

1. Do not wait for a 2026 hand model; none that fits exists, and I checked papers, HF, GitHub and company releases.
2. Keep MediaPipe hand landmark (Apache-2.0, 3D, handedness, small) as the baseline, as H1 recommended, and record the 2023 date as an exception.
3. Add `tasmulaev/rtmpose-m-distill` and the official RTMPose-m hand5 as 2D challengers in the cloud benchmark (task group 1), measured on our fist and occlusion fixtures, in fp32 and fp16, with 1, 2 and 12 threads. Only then decide whether the 2026 fine-tune is worth its single-author risk.
4. If the user wants a true 2026 model, the only route is to train one in the cloud (distill RTMPose-Hand or MediaPipe into a smaller student plus a lifting MLP). That is the build option in `hand-search-C.md`.

## Sources

- https://arxiv.org/abs/2510.16396 (SPLite Hand, read in full)
- https://arxiv.org/abs/2503.05995 and https://github.com/daishipeng/ReJSHand (ReJSHand)
- https://arxiv.org/abs/2608.22341 (TransHands), https://arxiv.org/abs/2608.11574 and https://github.com/ryhara/hand_visibility_detector (Hand Visibility Detector)
- https://arxiv.org/abs/2609.24424 and https://github.com/Mine268/CS-ViT (CS-ViT), https://arxiv.org/abs/2606.25619 (ScaleHP), https://arxiv.org/abs/2606.17427 (AR occlusion study)
- https://arxiv.org/abs/2603.16444 and https://github.com/hunainahmedj/Fast-HaMeR
- https://github.com/SeanChenxy/HandMesh (MobRecon)
- https://huggingface.co/tasmulaev/rtmpose-m-distill (API listing and HEAD request on the ONNX file, status 200, 55,118,513 bytes)
- https://huggingface.co/mlboydaisuke/RTMPose-m-Hand-ExecuTorch, https://huggingface.co/litert-community/RTMPose-Hand-LiteRT
- https://huggingface.co/nvidia/RT-DETR-Hand-Detector-v1, https://huggingface.co/DeeJanuz/frametop-hands-models, https://huggingface.co/amithroy/lazarus-hand-tracking
- https://github.com/svenflow/micro-handpose, https://github.com/yakhyo/mediapipe-hand-landmark-onnx, https://github.com/namas191297/cigpose-onnx
- Hugging Face API searches (`search=hand`, `hand pose`, `landmark`, `rtmpose`, author `qualcomm`) and GitHub repository search API queries, all run 2026-10-17
