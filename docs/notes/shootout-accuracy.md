# Candidate shootout: hands and limbs, accuracy and cost (task S1)

Date of run: 2026-10-17. All inference ran on Kaggle (private CPU kernels, no local model run). Raw results: `fixtures/private/out/shootout/shootout/results/<model>.json`, overlays: `.../overlays/<model>/<image>.png`, tables: `fixtures/private/out/shootout/summary.json` (made by `fixtures/private/kaggle/shootout/analyze.py`). The l1 and h2a rows come from a rerun (`fixtures/private/out/shootout_l1/`, see "Run notes").

## Setup

- Inputs: dataset `gpannn/vtuber-hands-ref`, 7 images: `hands_15`, `hands_16`, `hands_17`, `hands_19` (user photos) and `f0000`, `f0100`, `f0200` (golden_take).
- Runtime: onnxruntime 1.31.0 CPU (same version as the ort rc.14 bundle), Python 3.13, 1 thread per session, Intel Xeon 2.2 GHz, 4 cores. `mediapipe` 1.1.1, `ultralytics` 8.4.180. ms is the median of 3 timed runs after one warm-up, includes Python pre and post processing and every stage of the pipeline (detector plus landmark model). **ms is kernel CPU time and only comparable between rows of this note. It is not a desktop or Rust number.**
- Reference: official MediaPipe results in `fixtures/private/out/mp_hands_ref/results.json` (hands at confidence 0.3, pose full). The kernel also re-ran official MediaPipe: it reproduces the reference (NME 0.000 to 0.004).
- **The reference is not ground truth.** No human labels exist. Models that share MediaPipe weights (h1, h2, l1, l5 lite) agree with it by construction, so a low NME for them shows a faithful port, not better accuracy. For independent models (h3, h4, h5, l2, l3, l4) NME measures disagreement with MediaPipe, which can be the reference's fault.
- NME hands: mean point distance divided by the reference hand span (diagonal of the 21 reference points), over all 21 points (`nme21`), the wrist and the index, middle and ring tips. A model hand counts as found only if `nme21 <= 0.5`.
- NME limbs: distance divided by the reference shoulder width. Elbows and wrists that the reference puts outside the frame or at visibility < 0.5 (all elbows and wrists on f0000 to f0200) are excluded from the "in-frame" columns, because the reference only extrapolates there.

## Hands

Found = hands matched / reference hands (9 reference hands over 7 images; the reference at 0.3 finds 2, 1, 2, 1, 1, 1, 1; at 0.5 it misses f0000). NME columns are means over found hands. Handedness agreement is against the official label.

| Id | Candidate | Found | NME21 | NME wrist | NME tips | Median ms (max) | Files MB | Params M | Licence | Weights date | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| h6 | Official MediaPipe HandLandmarker (conf 0.3 / 0.5) | 9/9 / 8/9 | 0.001 / 0.004 | 0.001 | 0.001 | 36.6 (49) | 7.82 (.task, TFLite, not ONNX) | n/a | Apache-2.0 | **2023-04** | BASELINE, not a candidate (needs conversion) |
| h1 | yakhyo ONNX, palm 0.5 / presence 0.5 | 8/9 | 0.019 | 0.037 | 0.017 | 40.3 (57) | 15.5 (4.6 + 10.9) | 1.1 + 2.7 | Apache-2.0 | **2023 weights**, export 2026-07-28 | **CONDITIONAL** (2023 exception) |
| h1 lo | same, palm 0.3 / presence 0.3 | 9/9 | 0.033 | 0.040 | 0.023 | 40.0 (56) | same | same | same | same | CONDITIONAL, use this threshold |
| h2a | opencv_zoo 2023feb, run like our Rust port (palm detector, MediaPipe ROI, pose fallback) | 9/9 | 0.066 | 0.140 | 0.044 | 35.4 (94) | 8.0 (3.9 + 4.1) | 0.96 + 1.0 | Apache-2.0 | **2023-02** | **CONDITIONAL** (2023 exception, fallback is bad) |
| h2b | opencv_zoo own Python pipeline (cv.dnn) | 8/9 | 0.026 | 0.071 | 0.023 | 86.8 (112), cv.dnn | 8.0 | same | Apache-2.0 | **2023-02** | REJECT as a pipeline, keep as a cross-check |
| h3 | tasmulaev rtmpose-m-distill, RTMDet-nano boxes | 9/9 | 0.069 | 0.178 | 0.041 | 108.5 (197) | 55.1 + 4.0 (det) | 13.8 | Apache-2.0 | 2026-07-03 (fine-tune of 2023 weights) | **REJECT** |
| h3p | same, boxes from the palm detector | 9/9 | 0.066 | 0.116 | 0.045 | 111.8 (199) | 55.1 + palm | 13.8 | Apache-2.0 | same | REJECT |
| h4 | official RTMPose-m hand5 (rtmlib), RTMDet-nano boxes | 9/9 | 0.070 | 0.193 | 0.035 | 108.9 (197) | 55.1 + 4.0 | 13.8 | Apache-2.0 | **2023-03** | **REJECT** |
| h4p | same, palm-detector boxes | 9/9 | 0.055 | 0.107 | 0.035 | 112.3 (201) | 55.1 + palm | 13.8 | Apache-2.0 | **2023-03** | REJECT |
| h5 | marceloeatworld YOLO26s hand pose (fp32 / fp16 identical output) | 6/9 | 0.099 | 0.058 | 0.159 | 418 / 421 (431) | 42.9 / 21.6 | 10.7 | **AGPL-3.0, evaluation only** | 2026-04-03 | **REJECT** |
| l2w | CIGPose-m COCO-WholeBody, hand points 91 to 132 (bonus) | 8/9 | 0.127 | 0.148 | 0.167 | 115.5 (116) | 74.3 + 3.7 | 18.5 | Apache-2.0 | 2026-03 (release) | REJECT as a hand model |

Notes:
- Published cost: RTMPose-m hand 2.58 GFLOPs, YOLO26s hand 25.0 GFLOPs (marceloeatworld README). Not measured here for the others.
- Handedness: the MediaPipe 1.1.1 label agrees with the reference 8 of 8 (it is the reference). h1 and h2 labels are **opposite** to the official label on every matched hand (0 of 8, and 1 of 9 for h2a, whose raw value gives "Left" when above 0.5). The convention is consistent, so the fix is to flip the label. For h1 this contradicts the README sentence that the value is the probability of "Left". h3, h4, h5 give no handedness (2D points only, no depth).
- Off-frame check on h2a: on f0000 the palm detector finds nothing, the pose-wrist fallback is used and lands 0.39 span away (a miss). h1 at palm 0.3 finds the hand with NME 0.14. h3 and h4 find it through RTMDet but are 0.30 to 0.34 off.

### Per-photo `nme21` (matched hands; `!` would mean a wrong match, none occurred; `-` = not found)

| Id | hands_15 (2 hands) | hands_16 | hands_17 (2 fists) | hands_19 | f0000 | f0100 | f0200 |
|---|---|---|---|---|---|---|---|
| h6 conf 0.3 | 0.00, 0.00 | 0.00 | 0.00, 0.00 | 0.00 | 0.01 | 0.00 | 0.00 |
| h1 (0.5) | 0.01, 0.01 | 0.01 | 0.04, 0.03 | 0.01 | - | 0.02 | 0.03 |
| h1 lo (0.3) | 0.01, 0.01 | 0.01 | 0.03, 0.03 | 0.01 | 0.14 | 0.02 | 0.04 |
| h2a | 0.01, 0.01 | 0.02 | 0.03, 0.04 | 0.01 | 0.39 (fallback) | 0.03 | 0.05 |
| h2b | 0.01, 0.01 | 0.02 | 0.03, 0.03 | 0.01 | - | 0.04 | 0.06 |
| h3 | 0.02, 0.02 | 0.02 | 0.05, 0.05 | 0.02 | 0.34 | 0.04 | 0.06 |
| h4 | 0.02, 0.02 | 0.03 | 0.04, 0.05 | 0.02 | 0.34 | 0.04 | 0.07 |
| h5 fp32 | 0.02, 0.12 | 0.13 | - , - | 0.01 | - | 0.13 | 0.18 |
| l2w | 0.05, 0.09 | 0.04 | 0.14, 0.09 | 0.07 | - | 0.15 (+1 false hand) | 0.38 |

### Fist check, hands_17 (official finds 2 hands, 0.98 each)

Both fists found by: h1 (0.5 and 0.3), h2a, h2b, h3, h3p, h4, h4p, h6, l2w. **Not found by h5 (YOLO26s hand): 0 of 2, at confidence 0.25.** So the 2023 failure on hands_17 in `mediapipe-hand-reference.md` was the hand ROI, not the hand model: with the palm detector and the correct ROI the 2023 opencv_zoo weights place both fists within 0.03 to 0.04 span of the official result. The fist points of every model that found them are a tight cluster inside the fist, as in the official overlay.

### What the overlays show (viewed)

- `h1_yakhyo/hands_17.png`: both raised fists carry a tight cluster of red points with a short green stub for the thumb. Same look as the official overlay. No scattered dots.
- `h4_rtmpose_m_hand5/hands_16.png`: skeleton sits on the open palm, all five fingers in order, thumb out to the side. Clean.
- `l1_fernandotonon_pose/hands_17.png` (limb): shoulders, elbows and wrists trace both raised arms and the wrists sit on the fists. The hip points fall below the frame bottom.
- `l3_yolo26s_pose/f0000.png`: nose and eyes on the face; shoulders, elbows and hips drawn below the frame in low-confidence (magenta) colour. This is the sane behaviour for a desk frame.

### Hand verdict

- **Best accuracy: h1 yakhyo** (NME21 0.019 at the default threshold, finds both fists). It is the official MediaPipe weights, so it equals the reference within 0.02 to 0.04 span; no independent model beats it on this set.
- **Best efficiency: h2a opencv_zoo** (8.0 MB total, 35 ms), same weights lineage, a little less accurate than h1 (0.066, driven by one f0000 fallback miss; 0.01 to 0.05 on the other photos).
- **Best trade-off: h1 yakhyo with palm 0.3 and presence 0.3** (15.5 MB, 40 ms, 9 of 9 hands, 21 points with z, world points and handedness after flipping the label). All of h1, h2 and h6 are **2023 weights**, which still needs the user's currency decision (H1 note, option 1).
- The 2026 candidates (h3 2026 fine-tune, h5 2026 training) give no accuracy gain here and cost 3 to 12 times more time and 3 to 5 times more file size. h3 and h4 are 2D only, without handedness. h5 is AGPL and misses fists.

## Limbs

In-frame NME = shoulders, elbows and wrists against official pose full, only for reference joints inside the frame (hands_15 to hands_19, plus whatever is in frame on golden_take). Persons: the model's best person on the 5 upper-body joints. "n persons" lists detections per image in order (hands_15, 16, 17, 19, f0000, f0100, f0200).

| Id | Candidate | Shoulder NME | Elbow NME | Wrist NME | Median ms (1 thread) | Files MB | Params M | Licence | Weights date | n persons | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| l5 full | Official MediaPipe pose full | 0.000 | 0.000 | 0.002 | 46.2 | 9.40 (.task) | n/a | Apache-2.0 | **2023-04** | 1 each | BASELINE |
| l5 lite | Official MediaPipe pose lite | 0.040 | 0.099 | 0.119 | 39.8 | 5.78 (.task) | n/a | Apache-2.0 | **2023-04** | 1 each | CONDITIONAL fallback (not ONNX) |
| l1 | fernandotonon pose ONNX (current pick) | 0.044 | 0.089 | 0.089 | 53.1 | 24.7 (11.9 + 12.8) | 3.0 + 3.2 | `other` on the HF card; NOTICE says converted from the Google bundle (Apache-2.0) | **2023 weights**, upload 2026-07-13 | 1 each | **CONDITIONAL** (2023 exception, licence to confirm) |
| l2 | CIGPose-m COCO 256x192 + YOLOX-nano | 0.052 | 0.099 | 0.192 | 99.3 | 56.3 + 3.7 | 14.1 | Apache-2.0 | **2026-03** (release 2026-03-16) | 1 each | **CONDITIONAL** (only 2026 candidate, no proven gain) |
| l2b | CIGPose-l COCO 384x288 + YOLOX-nano | 0.051 | 0.101 | 0.185 | 315 | 113.6 + 3.7 | 28.4 | Apache-2.0 | 2026-03 | 1 each | REJECT (no gain, 3x slower) |
| l2w | CIGPose-m COCO-WholeBody (body part) | 0.072 | 0.109 | 0.168 | 115.5 | 74.3 + 3.7 | 18.5 | Apache-2.0 | 2026-03 | 1 each | REJECT |
| l3n | Ultralytics YOLO26n-pose (ONNX) | 0.060 | 0.131 | 0.129 | 164 | 12.1 | 3.0 | **AGPL-3.0, evaluation only** | 2026 | 2,1,1,1,1,2,2 (false 2nd person) | **REJECT** (licence) |
| l3s | Ultralytics YOLO26s-pose (ONNX) | 0.059 | 0.099 | 0.108 | 418 | 41.9 | 10.4 | **AGPL-3.0, evaluation only** | 2026 | 2,1,1,1,1,1,2 | **REJECT** (licence, slow) |
| l4 | RTMPose-m body7 256x192 + YOLOX-m (rtmlib balanced) | 0.062 | 0.105 | 0.124 | 1114 (YOLOX-m dominates) | 54.3 + 101.4 | 13.6 + 25.3 | Apache-2.0 | **2023-05** | 1 each | **REJECT** (cost, 2023) |

Published cost: RTMPose-m body 1.93 GFLOPs, CIGPose-m 1.9 GFLOPs. Ultralytics GFLOPs were not captured.

### Hips when off frame (f0000: hips about 650 px below the frame, official 0.001)

| Model | hands_15 (ref 0.375) | hands_17 (ref 0.68 to 0.70) | f0000 (ref 0.001) | f0100 (ref 0.001 to 0.002) | Sane? |
|---|---|---|---|---|---|
| l5 full | 0.375 | 0.675 / 0.697 | 0.001 | 0.001 / 0.002 | reference |
| l5 lite | 0.381 / 0.328 | 0.74 / 0.76 | 0.002 | 0.001 | yes |
| l1 | not tabulated | 0.89 / 0.93 (over-confident vs 0.68) | 0.009 / 0.022 | 0.009 / 0.017 | yes on golden_take |
| l2 CIGPose-m | 0.55 / 0.54 | 0.50 / 0.48 | **0.43 / 0.32** | 0.20 / 0.20 | no, mid-range on f0000 |
| l2b | 0.43 / 0.47 | 0.46 / 0.46 | 0.25 / 0.16 | 0.26 / 0.28 | no |
| l3n | 0.08 | 0.02 | 0.000 | 0.000 | yes |
| l3s | 0.01 | 0.02 | 0.000 / 0.001 | 0.000 | yes |
| l4 RTMPose-m | 0.57 | 0.67 / 0.64 | 0.11 / 0.16 | 0.13 / 0.16 | borderline |

The Rust-port hip defect from `mediapipe-hand-reference.md` (0.97 to 1.00 everywhere) is **not reproduced** by this Python port of the same decode: l1 gives 0.009 to 0.022 on golden_take. The Rust code should be compared against `l1_fernandotonon_pose.json` before it is trusted. l1 over-reports hips on hands_17 (0.89 against 0.68) where the hips are 66 px below the frame, so hip weights should be gated at a high threshold or not used (desk mode never uses hips).

### Limb verdict

- **Best accuracy:** cannot be decided without labels. Against the reference the closest independent model is l3s (elbow 0.099, wrist 0.108, but AGPL and 418 ms); among permissive models l1 (0.089 / 0.089) and MediaPipe lite are closest because they share the weights. CIGPose-m (0.099 / 0.192) does not show the +0.8 AP gain of the paper against MediaPipe. This is disagreement with a 2023 model, not proof of worse accuracy.
- **Best efficiency:** MediaPipe lite (5.8 MB, 40 ms), but it is TFLite. Among ONNX files: l1 (24.7 MB, 53 ms) is the smallest permissive pair; l3n is 12 MB but AGPL.
- **Best trade-off:** **l1** (current pick). It is the only permissive ONNX candidate that matches the official pipeline, is the cheapest, and has sane hips on the golden_take frames. It carries 2023 weights, so the currency exception stays open. CIGPose-m is the only 2026 candidate and is CONDITIONAL on a labelled wrist and elbow test.

## ACCEPT / REJECT / CONDITIONAL

| Candidate | Verdict | Reason |
|---|---|---|
| h1 yakhyo | CONDITIONAL | Accuracy equals official MediaPipe, both fists found, 15.5 MB, 40 ms, Apache-2.0. 2023 weights: needs the user's currency exception. Flip the handedness label. Use palm and presence 0.3 to get f0000. |
| h2 opencv_zoo (as h2a) | CONDITIONAL | Both fists found, 8 MB, 35 ms. 2023 weights. Fix the pose-wrist fallback (f0000 miss 0.39). h2b (cv.dnn pipeline) is a cross-check only. |
| h3 tasmulaev distill | REJECT | 2D only, no handedness, 55 MB, about 3x slower than h1, no accuracy gain, no ground-truth accuracy published. |
| h4 RTMPose-m hand5 | REJECT | Same as h3 and 2023 weights. |
| h5 YOLO26s hand | REJECT | AGPL, misses both fists on hands_17 and the f0000 hand, 420 ms, 21 to 43 MB. |
| h6 official MediaPipe hand | BASELINE | Reference. Not ONNX, 2023. |
| l1 fernandotonon | CONDITIONAL | Best trade-off, hips sane on golden_take. 2023 weights; HF licence tag is `other`, confirm the NOTICE text. |
| l2 CIGPose-m | CONDITIONAL | Only 2026 weights, Apache-2.0, ONNX exists and runs on ort 1.31, 56 MB, 99 ms (including YOLOX-nano). No gain shown against the reference, off-frame hips 0.32 to 0.43. Test against labelled wrists first. |
| l2b CIGPose-l 384 | REJECT | 113.6 MB, 315 ms, no accuracy gain. |
| l3 YOLO26n/s-pose | REJECT | AGPL (evaluation only), false second persons, 164 and 418 ms. Behaviour is good, licence is not. |
| l4 RTMPose-m body | REJECT | 1114 ms with YOLOX-m (155 MB), 2023 weights, hips 0.11 to 0.16 off frame. The run did time the stages separately (results/l4_rtmpose_m_body.json): YOLOX-m detector median 1046 ms, RTMPose-m pose model alone median 66.5 ms (1 thread, Kaggle Xeon 2.2 GHz, 7 images). For comparison CIGPose-m pose alone is 66.9 ms and the BlazePose landmark model alone is about 30 ms (detector 22 ms) on the same CPU. |
| l5 official pose full / lite | BASELINE | Reference; lite is the cheap fallback. Not ONNX. |

## Run notes

- Kernels (private, none deleted): `gpannn/vtuber-shootout-run` (v1, all stages, 279 s wall), `gpannn/vtuber-shootout-l1` (rerun of l1, h2a and h2b, 27 s wall). Earlier kernel `gpannn/vtuber-hands-ref-run` and dataset `gpannn/vtuber-hands-ref` kept.
- Bug found and fixed (systematic debugging): in v1 `l1` and the `h2a` fallback failed with `cannot reshape array of size 195 into shape (33,5)`. Cause: the landmark output has 39 points (195 = 39 x 5), the Rust code reads the first 33; the Python port reshaped to 33. The fix is `reshape(39, 5)[:33]`, rerun in `shootout2/run.py`. All 21 model ids then ran with `errors.json` empty.
- No model failed to load. No candidate is NOT RUN. The first run listed h2a f0000 as an error; the rerun replaced it.
- Limits: 7 images, one person per image, no human labels. Timings are 1 thread on a shared Xeon. The golden_take frames are webcam frames with no reference hands beyond what MediaPipe found.
- Licence of YOLO26 files: AGPL-3.0, used only for this evaluation, nothing is shipped.
