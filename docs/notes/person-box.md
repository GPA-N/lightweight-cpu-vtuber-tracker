# Person box for the CIGPose-m stage (OpenSpec task 11.4)

Date: 2026-10-18. Release build, ONNX Runtime, 4 threads.

## What was added

- `crates/infer/src/person_box.rs`: pure Rust, no model calls, 10 unit tests (written first, seen failing). `face_rect` (bounding box of the face landmarks), `from_face` (shoulders-and-arms box from the face box), `from_keypoints` (box around confident keypoints 0..10), `PersonBoxTracker` (`next` gives the box for the coming CIGPose run, `update` takes the keypoints of the run just done). Not wired into `live`.
- Face box to person box: width = 6.5 x face width, height = width x 256/192, centred on the face, top 0.5 face heights above the face top, clamped to the frame. If the face is outside the frame the whole frame is used.
- Keypoint box: bounding box of the nose, eyes, ears, shoulders, elbows and wrists whose score is at least 0.45, padded by 25 percent of the larger extent, clamped. Needs both shoulders confident, else `None` and the face box is used. Smoothing per box edge: moving outward 0.8 per frame, moving inward 0.2 per frame, so arms are never cut off by smoothing. Lost keypoints drop the memory (next box from the face). When the face tracker has no landmarks, the last face box is held for 30 frames, then nothing.
- CIGPose score scale: on this export the scores of found joints are 0.49 to 0.85 (not logits of 5 and above), and 0.12 to 0.39 for shoulders and elbows that are outside the frame (golden take, 90th percentile 0.39 and 0.34). The threshold 0.45 sits in that narrow gap. This is a data-derived value, not a robust one; recheck it when the stage is wired in.

## Tuning the width factor

Grid over the factor on the golden take and the four photos (`tests/person_box_golden.rs`): the golden take is 100 percent from 3.5 up (the user's face fills the frame, so the box is always the whole frame). hands_17 (both arms raised wide, face 80 px wide) needs at least 6.0 to keep all six arm joints inside; 5.5 loses one wrist. 6.5 keeps a margin. hands_15, 16, 19 are fine from 4 to 5.

## Verification (a): golden_take, 236 frames

Face box: bounding box of the 478 landmarks recorded per frame by the existing face tracker in `golden_take/lm.txt`. 9 frames have no landmarks (face lost); the last face box is held for those.

Usable definition: the BlazePose `pose_landmarks` shoulders (11, 12) and elbows (13, 14) with visibility at least 0.5, projected onto the frame (clamped, because in this take BlazePose puts them below or beside the frame edge, never inside it), must all lie inside the person box (itself clamped to the frame). Every frame has at least one such point (10 frames one, 26 two, 198 three, 2 four). The CIGPose shoulder fallback was not usable: with shoulders outside the frame its scores are 0.19 to 0.39.

| Box | Usable frames |
|---|---|
| Face-derived only | 236/236 (100 percent) |
| Tracked pipeline (face box, then keypoint box when both shoulders score at least 0.45) | 235/236 (99.6 percent) |

The one miss: on one frame CIGPose scored both shoulders above 0.45 although they are out of frame, so the next box came from those keypoints and was smaller than the BlazePose region. The gate is 95 percent, passed.

Caveat, stated plainly: because the take is a face close-up, the box is the whole frame and this check is weak. It proves the box does not cut off the visible arms and survives face loss, not that the width factor is right for a person sitting at a desk. The photos are the better test of the factors.

## Verification (b): four user photos

Face box from the existing face tracker (detector and landmarks, three steps on the still image). The tracker found a face on hands_15 and hands_16. It found no face on hands_17 (head tilted back, laughing) or hands_19 (head down): with the real tracker there is no box on those two. For them a stand-in face box was made from the reference ears (width 1.1 x ear distance, height 1.3 x width); this checks the box geometry, not the tracker. Reference: `l2_cigpose_m_coco.json`. Pixel error is the Euclidean distance to the reference keypoint, all six shoulder, elbow and wrist joints found (score at least 0.45), all six reference joints inside the box.

| Photo | Frame | Face from | Face-derived box, max joint error px | Keypoint box (second frame), max joint error px |
|---|---|---|---|---|
| hands_15 | 212x148 | tracker | 2.7 | 2.7 |
| hands_16 | 181x148 | tracker | 4.5 | 2.6 |
| hands_17 | 547x365 | stand-in | 0.0 | 0.0 |
| hands_19 | 447x447 | stand-in | 5.2 | 5.2 |

Large differences are not a concern here: the boxes are mostly the same crop as the reference after the model's 1.25 padding and frame clamp (hands_17 is identical, hence 0.0). All errors are within the size of a few pixels on small images.

## Open items

- The face tracker's failure on hands_17 and hands_19 (tilted head) means a person leaning far back or looking down loses the face-derived box. The keypoint box carries the stage once found, and the face box is held 30 frames, but a cold start in such a pose has no box. A later option is the whole frame as the first box.
- The score threshold 0.45 and the 30-frame hold are untested live.
