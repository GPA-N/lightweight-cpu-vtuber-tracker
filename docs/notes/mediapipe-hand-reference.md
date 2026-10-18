# Official MediaPipe hand and pose reference (task 1.5b)

## 1. Summary

The official MediaPipe Hand Landmarker and Pose Landmarker were run on the four user photos and three golden_take frames and compared with our ONNX port (`docs/notes/ub-benchmark.md`). The official models find both fists in hands_17 (score 0.98 each) and put the skeleton on the fists. Our port does not. The official pose gives sensible hip visibility (0.0 when the hips are far off frame). Ours reports about 0.97 to 1.00 everywhere.

Verdict: our 2023 ONNX hand landmark model is not shown to be the weak part. The weak parts are our hand ROI (we crop from the previous or pose wrist instead of running a palm detector with a proper ROI) and our pose visibility handling. The comparison cannot fully separate model from pipeline, because the port dumped overlays only, not landmark numbers (see section 6).

## 2. Setup

- Run: Kaggle CPU script kernel `gpannn/vtuber-hands-ref-run` (private, internet on, no GPU), dataset `gpannn/vtuber-hands-ref` (private). Script: `fixtures/private/kaggle/kh/run.py`. Finished in under 5 minutes (two status checks).
- Library: `pip install mediapipe` (Tasks API, `RunningMode.IMAGE`, CPU).
- Models, from `storage.googleapis.com/mediapipe-models/`: `hand_landmarker/hand_landmarker/float16/1`, `pose_landmarker/pose_landmarker_full/float16/1`, `pose_landmarker_lite/float16/1`.
- Hands: `num_hands=2`, `min_hand_detection_confidence` 0.3 and 0.5 (other thresholds default). Pose: `num_poses=1`, defaults.
- Inputs: `hands_15`, `hands_16`, `hands_17`, `hands_19` (`hands_20` is byte-identical to 19, skipped) and `f0000`, `f0100`, `f0200` from golden_take.
- Outputs: `fixtures/private/out/mp_hands_ref/` (one JSON per image with image and world landmarks, handedness, pose visibility and presence; `results.json`; `annotated/*.png`). The dataset holds the user's photos and stays private on Kaggle.
- Image-coordinate values below are pixels (normalised x,y times width and height). Pose points outside the image give coordinates beyond the frame; that is normal.

## 3. Hands: official vs ours

Handedness labels are MediaPipe's, which assume a mirrored (selfie) image. On these un-mirrored photos the hand at the image left is labelled "Right". Our port never verified its labels.

| Photo | Official hands (conf 0.3 / 0.5) | Handedness (score) | Ours | Wrist / index tip, official (px) | Ours vs official (visual) |
|---|---|---|---|---|---|
| hands_15 (212x148) | 2 / 2 | Right 0.95, Left 0.96 | 2, crop scores about 0.73 | R: wrist (42,86), index tip (30,40). L: wrist (173,92), index tip (171,34) | Both hands found; skeleton shape and place close, a few px off (my estimate under 10 px) |
| hands_16 (181x148) | 1 / 1 | Left 0.88 | 1 at threshold (0.50), left-arm spurious points | wrist (122,58), index tip (114,21) | Roughly on the palm; low confidence; spurious green points on the other arm |
| hands_17 (547x365) | 2 / 2 | Right 0.98, Left 0.98 | not reliable, scores 0.50 to 0.55 | R: wrist (51,131), index tip (61,109). L: wrist (486,153), index tip (481,129) | Fail. Image-left fist: scattered dots about 45 px to the right of the fist, no hand shape. Image-right fist: dots near the fist but no skeleton |
| hands_19 (447x447) | 1 / 1 | Right 0.98 | 1 (0.73) plus spurious points near the opposite elbow | wrist (115,185), index tip (188,81) | Good, within a few px by eye |
| f0000 | 1 / 0 | Right 0.78 | skeleton landed on forehead (earlier note) | wrist (67,239), index tip (183,263) | Official lands on the raised hand next to the head; ours on the head |
| f0100 | 1 / 1 | Right 0.99 | no hand drawn | wrist (48,362), index tip (188,269) | Ours missed |
| f0200 | 1 / 1 | Right 0.99 | no hand drawn | wrist (57,345), index tip (190,268) | Ours missed |

Pixel errors are by eye from overlays. The port's own landmark values were not saved, so no exact pixel error is available for it.

Finger curl (official): in hands_17 the fingertips sit 22 to 24 px from the wrist on a hand about 25 px wide, which is a closed fist, and the overlay shows tightly packed points on both fists. In hands_15 and hands_19 the tips are far from the wrist (open palm). Official curl is plausible everywhere. Ours is plausible only on 15 and 19.

## 4. Pose: official vs ours

Official pose (full) joint positions, px, with visibility in brackets.

| Photo | L shoulder | R shoulder | L elbow | R elbow | L wrist | R wrist | Hips |
|---|---|---|---|---|---|---|---|
| hands_15 | (149,56) [1.00] | (74,64) [1.00] | (186,123) [0.99] | (58,123) [0.96] | (176,87) [1.00] | (42,83) [1.00] | 0.38 / 0.37, y 173 (frame height 148) |
| hands_16 | (102,65) [1.00] | (54,69) [1.00] | (117,78) [1.00] | (49,106) [0.85] | (121,56) [0.99] | (53,135) [0.57] | 0.94 / 0.86, y 139 (in frame) |
| hands_17 | (338,217) [1.00] | (201,222) [1.00] | (441,218) [1.00] | (102,201) [1.00] | (488,157) [1.00] | (53,134) [1.00] | 0.68 / 0.70, y 431 (frame height 365) |
| hands_19 | (323,225) [1.00] | (187,189) [1.00] | (333,341) [0.86] | (78,240) [1.00] | (349,452) [0.70] | (124,177) [1.00] | 0.85 / 0.87, y 433 (in frame) |
| f0000 | (552,512) [0.93] | (149,506) [0.96] | (695,761) [0.32] | (-140,613) [0.91] | (564,631) [0.25] | (43,339) [0.83] | 0.00 / 0.00 |
| f0100 | (529,508) [0.99] | (183,491) [0.99] | (628,840) [0.18] | (-23,614) [0.88] | (608,990) [0.06] | (122,395) [0.86] | 0.00 / 0.00 |
| f0200 | (474,504) [0.91] | (131,481) [0.97] | (495,752) [0.35] | (-106,633) [0.88] | (334,611) [0.21] | (72,399) [0.86] | 0.00 / 0.00 |

The lite model gives similar numbers (within about 10 to 40 px on the large frames, with lower elbow and wrist confidence on f0000 to f0200).

Hip visibility: the official model reports hips at 0.00 on the golden_take frames (hips about 650 px below the frame), 0.37 to 0.38 on hands_15 (hips 25 px below the frame), and 0.68 to 0.70 on hands_17 (hips 66 px below). It reports 0.85 to 0.94 only when the hips are inside the frame (16, 19). So the official model does give a false-positive-ish 0.7 on hands_17, but it tracks how far off frame the hips are. Our port reported 0.97 to 1.00 on every photo, including 15 and 17. That is a defect in the port, probably in the visibility decode (the sigmoid or the output index), since the same family of model gives graded values upstream. This should be fixed before sway (design.md D5) uses hip visibility.

hands_17 pose: official has all arm points at visibility 1.00 and on the body. Ours had elbow visibility 0.15 to 0.19, wrist 0.32 to 0.42, and drew shoulder points on the chest. Our pose failed on this photo.

## 5. Annotated official images (what I saw)

Viewed with the Read tool from `fixtures/private/out/mp_hands_ref/annotated/`.

- hands_17: both raised fists carry a tight cluster of red landmark points with a green skeleton; pose lines run from the shoulders through the elbows to the wrists, and the shoulder line crosses the chest correctly. Clean result. Our overlay for the same photo shows scattered green dots beside the left fist and chest dots in the wrong place.
- hands_16: an open palm raised in front of a man's chest. The official skeleton sits on the palm with all five fingers in order. Pose box lines trace the torso; the hip points sit at the very bottom edge. Our overlay has a green blob on the palm and a spurious green column on the left arm.
- hands_15: two open palms, both skeletons well placed on the fingers, elbows and shoulders traced. Ours is similar but looser.
- hands_19: open palm at the image left, skeleton well on the fingers; the pose lines trace the shoulders and torso down to the hips near the bottom edge. Ours is close on the hand and adds spurious points near the opposite elbow.
- f0000: official skeleton lands on the raised hand beside the head. Pose lines hide the lower face because the elbows are far out of frame. Ours put the hand on the head.

## 6. Verdict

- Is our 2023 ONNX port as good as the official landmarker? Not yet, as run. On clear open palms (15, 19) it is close. On hands_17 and on the golden_take frames the official pipeline finds the hand and ours does not. On hip visibility ours is wrong.
- Is photo 17 failing because of the port or the detector? The detector. The official pipeline runs a palm detector (`hand_detector`) and then a hand landmark model on its ROI, with tracking. Our bench never ran the palm detector and cropped from wrists predicted by pose, which failed on 17 (pose visibility 0.15 to 0.42 at the wrists). The port's hand landmark model itself works on 15 and 19. Whether it works on a correctly placed fist ROI is not yet tested, because we have no landmark dump from the port on a fist. Plausible but unproven: it will work, since the official landmark models were built to cover fists.
- Limit of this comparison: the port bench saved overlays only. Exact pixel error needs a port run that dumps landmarks to JSON, which is a code change in `crates/` and was out of scope here.

## 7. Recommendation

1. Keep the port for now. Do not switch models on this evidence.
2. Run the 2023 palm detector (already in `models/upper_body/`) and use its ROI, not the pose wrist, for hand crops. Re-run hands_17 and golden_take frames. Add a landmark JSON dump to the bench so these tables can be filled with exact pixel errors.
3. Fix pose visibility decode and compare against the official values in section 4 (target: hips about 0.0 on golden_take).
4. If step 2 does not fix hands_17, convert the official models. The `.task` file is a zip: unzip it and it holds `hand_detector.tflite` and `hand_landmarks_detector.tflite` (and `pose_detector.tflite`, `pose_landmarks_detector.tflite` for pose). Convert with `tf2onnx --tflite hand_landmarks_detector.tflite --output ...onnx --opset 13` (or `tflite2onnx`). Then check output order against the official JSON in `fixtures/private/out/mp_hands_ref/`. The official models are Apache-2.0, as are MediaPipe Tasks and the model cards, so a converted copy may be shipped with the notice. Re-confirm the model card licence text before shipping and record it in `docs/notes/licenses.md`.
5. A newer public ONNX export of the hand landmarker was not searched here; task 1.5 still covers that.

## 8. Privacy

The dataset `gpannn/vtuber-hands-ref` and the kernel `gpannn/vtuber-hands-ref-run` are private and hold the user's photos and three webcam frames. They have not been deleted. Ask the user whether to delete them.
