# Hand search, angle C: whole-body models and build options (task H2c)

Date: 2026-10-17. No model was run, nothing downloaded or executed. Sources are model cards, repository pages, arXiv abstracts and Hugging Face listings read on this date. Items I could not read are marked UNVERIFIED. Estimates are marked ESTIMATE and are not measurements.

Target (from the user): finger landmarks plus arm limbs on CPU through ONNX, about 3 to 10 ms per hand or one whole-body pass under about 15 ms extra, under 60 MB total, commercial-friendly licence, works with fists and occlusion, desk webcam framing, current (2026).

## Verdict

**No whole-body model is accepted.** No 2026 whole-body model has a published ONNX and passes the hand-quality bar. The one 2026 family (CIGPose) has no ONNX, and its hand AP is 49.7 to 62.6 on COCO-WholeBody. The ready whole-body ONNX files are 2023 weights. Every one of them is also trained on COCO-WholeBody, whose annotations are research-only (see "Licence trap"). Details are below.

**No tiny 2026 hand model with weights exists** either (part 3). So the hybrid reduces to "pose wrist crop plus a 2023 hand model in a 2026 ONNX export", which is already the plan in `hand-models-2026.md`.

**My recommendation:** do not start the build (part 2) yet. The only evidence we hold about the hand failure (`mediapipe-hand-reference.md`, section 6) points to our hand ROI and not to the hand weights. Fix the ROI with the palm detector first. Build only if the user insists on 2026-trained weights; the plan is below.

## Part 1: whole-body models, ranked

Hand AP here is the COCO-WholeBody hand AP. It is measured on hands inside a full-person crop at 256x192 or 384x288, where a hand is only about 25 to 40 px wide. It is not comparable with PCK@0.2 from a dedicated hand crop. A desk webcam with the upper body filling the frame puts hands in the same small size range, so this number is the honest predictor for a single-pass whole-body model.

| Rank | Candidate | Weights date / lineage | Hand AP | Whole AP | Params / GFLOPs | CPU ms (source) | Size | ONNX | Licence | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | RTMPose-m wholebody, Qualcomm export `qualcomm/RTMPose-Body2d` | 2023 mmpose weights, re-export v0.64.0 for ORT 1.30 (card paper date 2023-03-13; page shows no model date) | not on the card (RTMW-m is 49.1) | 58.2 (mmpose table) | 17.9 M params (card), 2.22 GFLOPs (mmpose) | 13.50 ms ORT i7-11700 (mmpose COCO-WholeBody table). Card latencies 0.9 to 3.2 ms are NPU only | 68.5 MB float, 18.2 MB w8a16 | yes, float and w8a16 zips | Apache-2.0 (card) | Rejected: 2023, 68.5 MB float is over 60 MB, hands too small. Fastest honest whole-body pass |
| 2 | RTMW-m 256x192 (`bukuroo/RTMW-ONNX` `rtmw-m-256.onnx`) | 2023-09 weights (cocktail14), ONNX upload 2025-09-03 | 49.1 | 58.2 | 4.3 GFLOPs | none published. LiteRT build: 110 ms on Raspberry Pi 5 (2026-08-31); the card says XNNPACK declined the fp16 graph | 62.4 MB fp32 | yes | Apache-2.0 on the card; training data includes InterHand2.6M and COCO-WholeBody | Rejected: 2023, 2.4 MB over the cap, no CPU number, small hands |
| 3 | CIGPose-m wholebody (CVPR 2026, arXiv 2603.09418, `53mins/CIGPose`) | 2026 (arXiv 2026-03-10, CVPR 2026) | 49.7 | 59.9 | 2.3 GFLOPs, params not listed | none. ESTIMATE about 14 ms, by scaling 13.5 ms for 2.22 GFLOPs of RTMPose-m wholebody | not known | **none**, checkpoint on Google Drive | Apache-2.0 repo; trained on COCO-WholeBody | Rejected: hand AP only +0.6 over RTMW-m, no ONNX, export risk (graph head), NC training annotations |
| 4 | CIGPose-l wholebody 256x192 | 2026 | 54.0 | 62.6 | 4.6 GFLOPs | none. ESTIMATE about 28 ms | not known | none | as above | Rejected: over time budget, same defects |
| 5 | CIGPose-l+UBody 384x288 | 2026 | 61.2 | 66.9 | 10.7 GFLOPs | none. ESTIMATE about 65 ms | not known | none | as above, plus UBody (licence UNVERIFIED) | Rejected: far over budget |
| 6 | CIGPose-x+UBody 384x288 | 2026 | 62.6 | 67.5 | 18.7 GFLOPs | none | not known | none | as above | Rejected: far over budget, most accurate of the family |
| 7 | RTMW-x / RTMW-l 384x288 and DWPose-style re-uploads (e.g. `Izymka/rtmw-dw-x-l_simcc-cocktail14`, 2026-05-27) | 2023 weights, re-uploaded in 2026 | 66 to 66.4 (RTMW-x, RTMW-l 384, from `ub-models.md` second pass) | 70.1 / 70.2 | 17.7 / 29.3 GFLOPs | none | 229 MB / 369 MB | yes | MIT tag on the re-upload (card is two lines, no lineage); Apache-2.0 upstream | Rejected: 4 to 6 times over the size cap, 2026 date is only an upload date |
| 8 | SDPose-Wholebody (`ViktorLin/SDPose-Wholebody`, arXiv 2509.24980, 2025-09-29) | 2025, Stable Diffusion v2 U-Net fine-tune | not reported on the card | not reported | about a SD2 U-Net (parameter count not stated; roughly 860 M, ESTIMATE from SD2) | far over budget | not stated | none (PyTorch, safetensors) | MIT on the card; the SD2 base has its own licence | Rejected: size, no ONNX, 2025 |
| 9 | Sapiens2 (arXiv 2604.21681, 2026-04-23) | 2026 | not stated | not stated | 0.4 to 5 B params | far over budget | multi-GB | none found | not stated (CC BY 4.0 covers the paper only) | Rejected: size, no small variant, licence not detected |
| 10 | ViTPose++ wholebody | 2022 to 2023 | not re-checked | not re-checked | ViT-B 86 M or larger (from memory, UNVERIFIED) | not checked | large | community exports | Apache-2.0 (from memory, UNVERIFIED) | Rejected: size, 2023 |
| not found | RTMO wholebody, RTMW2, DWPose successor, Qualcomm 2026 retrain, any model on COCO-WholeBody v2, Halpe, GigaHands, HOT3D, Ego-Exo4D hand data | | | | | | | | | I found no release with weights for any of these. Searches: Hugging Face model search for "wholebody pose onnx" (zero results), "rtmw" (only the rows above), "hand pose" and "hand landmark" sorted by creation date (nothing relevant after 2025-10); a web search for 2026 whole-body papers returned only CIGPose. Absence of evidence, not proof of absence |

### Why whole-body fails this brief, in numbers

1. **Hand AP 49.7 to 62.6 is low.** RTMPose-m trained on a dedicated hand crop (Hand5 "alpha", 256x256, 2.581 GFLOPs) reaches PCK@0.2 81.5 on COCO-WholeBody-Hand and 96.4 on Hand5. The metrics differ (AP against PCK), so I do not claim a ratio. The direction is clear: hands in a full-person crop are the weak part of every whole-body model.
2. **Size.** The only whole-body ONNX files that fit a CPU budget are 62.4 MB (RTMW-m fp32) and 68.5 MB (Qualcomm float). Both are over the 60 MB total, before any pose or detector. The 18.2 MB w8a16 Qualcomm file is a QNN-style quantisation; its speed on ORT CPU is UNVERIFIED and activations in 16 bits are not a CPU fast path (stated as a risk, not as a measurement).
3. **Time.** The only CPU number I found for any 133-keypoint model is 13.50 ms for RTMPose-m wholebody at 256x192 (mmpose table, i7-11700, ORT). That fits the "under 15 ms" wish, but it is a 2023 model with weak hands.
4. **Currency.** The only 2026 whole-body model with weights is CIGPose, and it has no ONNX. The H1 note already flagged the export risk (data-dependent graph head).
5. **Fists and occlusion.** No whole-body source reports a fist or occlusion number. UNVERIFIED for all rows.

### Licence trap behind every whole-body weight

`jin-s13/COCO-WholeBody` states: annotations "ONLY for research and non-commercial use" (the page text is inconsistent: CC BY 4.0 in the header, CC BY-NC 4.0 in the link). Images come from COCO with Flickr terms. For commercial use the authors ask to be contacted. RTMW is trained on cocktail14, which includes COCO-WholeBody, UBody and InterHand2.6M (CC BY-NC 4.0). CIGPose is trained on COCO-WholeBody. Repository and model-card licences say Apache-2.0 or MIT, but whether weights trained on non-commercial annotations can ship in a commercial product is an open legal question that the cards do not answer. This applies to the RTMPose-Hand weights (Hand5 includes InterHand-like data, UNVERIFIED) as well. The MediaPipe weights have no such dependency that I know of (Google trained on its own data; the model card says Apache-2.0, as in `mediapipe-hand-reference.md`).

## Part 2: build our own hand model (plan only, nothing run)

### Honest framing

A student distilled from the MediaPipe hand landmarker cannot beat its teacher in expectation. What the build buys is (a) weights trained in 2026 under our control, (b) a smaller or faster net, (c) robustness training on fists and occlusion that we choose. It does not buy accuracy. The MediaPipe hand landmark model is already small (about 2 M params per the Qualcomm card; the OpenCV ONNX is 4.1 MB fp32), so a speed gain is small. Treat the build as a currency purchase, and tell the user so.

### Teacher

- **Primary: the official MediaPipe hand landmarker** (Apache-2.0, `hand_landmarker.task` float16, already run on Kaggle in task 1.5b). Pseudo-labels: 21 x (x, y, z), presence, handedness.
- Improve the teacher with test-time augmentation: 4 rotations (0, +-15, 90 degrees steps are not needed, use +-15) plus a horizontal flip, average after un-warping; drop samples where the TTA spread exceeds 5 percent of hand size. This gives labels slightly better than a single pass.
- **Not used as teacher:** HaMeR, WiLoR, Fast-HaMeR (MANO is non-commercial; WiLoR weights are CC-BY-NC-4.0), RTMW / RTMPose-Hand (trained on NC annotations, see above). Using them to pseudo-label would carry the same problem into the student. This is a conservative reading, not legal advice.

### Images (unlabelled, so only the image licence matters)

| Source | Licence | Use |
|---|---|---|
| Open Images v7 person images filtered to CC BY 2.0 per-image licence field | per-image CC BY, attribution kept in a manifest | main pool, hands cut with the MediaPipe palm detector |
| User's own desk webcam frames (golden_take, 236 frames, private) | user-owned | desk-framing fine-tune, never published |
| Synthetic occlusion: paste random CC0 objects over crops | CC0 | augmentation |
| COCO / COCO-WholeBody images | Flickr terms, annotations NC | **not used** |
| Ultralytics Hand Keypoints | images CC BY-NC-SA 4.0, labels from MediaPipe | **not used** |
| FreiHAND | research only per a 2026 secondary source (UNVERIFIED, no primary page read) | not used |
| InterHand2.6M | CC BY-NC 4.0 | not used |
| HOT3D | hand annotations CC BY-NC-SA, sequences CC BY-SA, no selling of the material | not used |
| HO-3D, GigaHands | licence not found; GigaHands annotations showed "to be released soon" in a cached page | read the primary licence before any use |
| Ego-Exo4D | custom licence, not read | not used |

Finding: **I could not verify a single labelled hand dataset as safe for a commercial product.** That is why pseudo-labels on CC BY images are the only clean route in this plan. Attribution for CC BY images must ship in a credits file.

### Student

- Backbone: MobileNetV3-Large (or EfficientNet-Lite0) at 160x160 (the teacher uses 224). About 3 to 5 M params, 0.1 to 0.2 GFLOPs at 160 px, ESTIMATE.
- Head: SimCC-style 1D classification for x and y (better than direct regression at small input), plus z regression, a presence logit, and a handedness logit. A heatmap head is not needed.
- Loss: KL on soft SimCC targets plus L1 on z, plus BCE for presence and handedness. Teacher confidence weights each sample. Augment with rotation, scale, flip (swap handedness), colour jitter, random erasing, and synthetic occlusion; oversample fist crops (teacher curl ratio under 1.2).
- Export: PyTorch to ONNX opset 17, fp32 (about 12 to 20 MB), plus optional fp16 weights. Check the parity against PyTorch on 100 crops (max abs diff under 1e-4) in the kernel. Output shape and names are chosen to match the MediaPipe ONNX consumer so the Rust side changes little.

### Compute plan and the 1-hour rule

One Kaggle kernel at a time, each estimated under 1 hour (user rule). Three kernels, each saving its output as a Kaggle dataset version:

| Kernel | Work | Estimate (ESTIMATE, to be re-checked by a 5-minute dry run) | Output |
|---|---|---|---|
| A: pseudo-label | palm detector plus landmarker with 5-way TTA over 60 k person images, CPU | 60 k images, about 1.2 hand crops per image, 5 passes at about 5 ms = about 30 min; split in two shards if the dry run shows more | 100 k crops (uint8 160x160) with labels as npz |
| B: train | 20 epochs on a T4 with AMP, data resident in RAM as uint8 | 100 k crops at about 1500 img/s = about 70 s per epoch, about 25 min plus export | ONNX plus PyTorch checkpoint |
| C: evaluate | the protocol below, plus ORT CPU timing | about 10 min | JSON metrics, overlays |

If a dry run projects more than 50 minutes for any kernel, cut the pool, not the checks.

### Effort, risk, expected accuracy

- Effort: about 3 to 4 working days, 300 to 500 lines of Python, one Rust-side shape check. Most of the time goes into the ROI and crop code that must match the runtime, not into training.
- Risk 1, licence: images are CC BY and the teacher is Apache-2.0, so the chain is clean on paper. I have not read the Google model card terms for derived models; read them first. No lawyer review.
- Risk 2, no gain: the student inherits the teacher's failures (it will not fix hands_17 by itself, because that failure is a crop problem).
- Risk 3, domain: CC BY open-image hands are not desk-webcam hands. The 236 golden_take frames help only if the user allows training use.
- Expected accuracy (ESTIMATE from general distillation experience, not measured): mean error against the teacher on held-out crops of 3 to 5 percent of hand size; against ground truth, 0.5 to 1.5 points of PCK below the teacher; fist and occlusion robustness equal to the teacher on average, better only on the augmentations we add. A speed gain of up to 2 times over the 224-px teacher from the smaller input, not measured.

### Evaluation protocol on the four photos

Photos: `fixtures/private/upper_body_photos/hands_{15,16,17,19}.png` (212x148, 181x148, 547x365, 447x447 px; `hands_20.png` is byte-identical to 19 and is skipped). They stay on the private Kaggle dataset `gpannn/vtuber-hands-ref`. Four images with five hands are a smoke test, not a benchmark. No result on them is a claim of accuracy.

1. Reference: the official MediaPipe JSON already in `fixtures/private/out/mp_hands_ref/` (teacher, conf 0.3).
2. Same ROI for all models: use the teacher's palm detector box for every student run, so the test measures the landmark net and not the crop.
3. Metric per hand: NME against the teacher, normalised by wrist to middle-MCP length; report median and worst joint.
4. Fist check on hands_17 (both hands): all fingertips closer to the wrist than 1.3 times the palm width, as in the teacher (22 to 24 px on a 25 px hand).
5. Open palm check on hands_15, 16, 19: fingertip order along the finger, no crossing.
6. Self-consistency: rotate and flip each crop; the landmark spread must stay under 5 percent of hand size.
7. Desk set: report jitter (px, frame to frame) on the 236 golden_take frames, private.
8. Larger held-out check before any decision: 500 hand crops from CC BY images not in training, with 50 hand-labelled by the user or me (fists and occlusions), to get a real PCK.
9. Pass rule: NME at most 1.5 times the teacher's own TTA spread on each photo hand, fist check passes, no crossings. Otherwise do not ship.

## Part 3: cheaper hybrid (pose wrist crop plus a tiny new model)

| Option | Hand model | Date | Cost | Verdict |
|---|---|---|---|---|
| H-a: pose wrist crop (BlazePose, already in the app) plus MediaPipe hand landmark through the 2026 `yakhyo` ONNX | 2023 weights, 2026 export | about 2 M params, desktop ms not measured; Pixel 6 CPU 17.12 ms for the whole hand pipeline | The existing plan. Use the palm detector ROI where pose wrists are unreliable (hands_17) |
| H-b: pose wrist crop plus RTMPose-Hand (hand5 alpha) | 2023 | 2.581 GFLOPs, 58 ms on a Pi 5 CPU, no desktop ORT number; ESTIMATE about 14 ms per hand on i7-11700 by scaling | 2D only, over the 3 to 10 ms goal, NC-training question |
| H-c: pose wrist crop plus a tiny 2026 model | none exists with weights | LightWeightHandNet (Springer 2026, MobileNetV2, 13.48 ms, CMU-Hand and FreiHAND training): no weights or licence found; SPLite Hand (arXiv 2510.16396, 2025-10): no repository or weights link on the arXiv page, FreiHAND PA-MPJPE 9.0 mm quoted | Not usable |
| H-d: pose wrist crop plus our own student (Part 2) | built here | see Part 2 | Only if the user insists on 2026 weights |

Challenge to H-a: its hands_17 failure in our port was a crop problem. The cheap hybrid is only good if the crop is good. A forearm-extrapolated crop from pose wrists depended on pose visibility of 0.15 to 0.42 on that photo and failed. Run the palm detector, or gate the pose crop on wrist visibility.

## Challenge to my own conclusions

- **"Whole-body hand AP is too low."** The AP is measured on all hands in COCO-WholeBody, including tiny and blurred ones. A close desk webcam may be easier than the average. This could be tested on our four photos in one cloud run of RTMPose-m wholebody (`qualcomm/RTMPose-Body2d`), cost about 5 minutes. I did not run it (no model runs here). If the user still wants a whole-body candidate, that run is the cheapest way to check my verdict.
- **"CIGPose has no ONNX."** I only read the README. An unpublished export may exist in the issues; not checked.
- **Latency estimates for CIGPose** are scaled from one measured 2023 model and may be wrong by a factor of 2.
- **Hand AP for the Qualcomm export** was not on the card. I used RTMW-m's 49.1 (from the earlier second pass) as a proxy; RTMPose-m wholebody is probably similar or lower (UNVERIFIED).
- **Dataset licence readings are partial.** InterHand2.6M (CC BY-NC 4.0) and HOT3D (hand annotations CC BY-NC-SA) came from search snippets of the agreements. FreiHAND came from a secondary source. HO-3D and GigaHands terms were not found. Re-read the primary text before any use.
- **The teacher licence for a distilled student** is assumed to be fine under Apache-2.0; that is my reading, not legal advice.
- **The bigger risk to the user's goal is not model age.** It is the crop. Rank 1 in every sensible plan is the palm detector ROI.

## Sources

- https://github.com/53mins/CIGPose (whole-body and body tables, Apache-2.0 per the README) and https://arxiv.org/abs/2603.09418
- https://github.com/open-mmlab/mmpose/blob/main/projects/rtmpose/README.md (RTMW cocktail14 table, COCO-WholeBody ORT table, Hand5 table)
- https://huggingface.co/qualcomm/RTMPose-Body2d
- https://huggingface.co/bukuroo/RTMW-ONNX, https://huggingface.co/Izymka/rtmw-dw-x-l_simcc-cocktail14, https://huggingface.co/litert-community/RTMW-m-WholeBody-LiteRT
- https://huggingface.co/ViktorLin/SDPose-Wholebody (arXiv 2509.24980)
- https://arxiv.org/abs/2604.21681 (Sapiens2), https://arxiv.org/abs/2510.16396 (SPLite Hand)
- https://github.com/jin-s13/COCO-WholeBody (licence text)
- https://projectaria.com/datasets/hot3D/license, https://arxiv.org/abs/2008.09309 (InterHand2.6M), https://ivl.cs.brown.edu/research/gigahands.html
- https://docs.ultralytics.com/datasets/pose/hand-keypoints
- https://link.springer.com/chapter/10.1007/978-981-96-9971-1_20 (LightWeightHandNet)
- Project notes: `hand-models-2026.md`, `ub-models.md`, `upper-body-research.md`, `mediapipe-hand-reference.md`
