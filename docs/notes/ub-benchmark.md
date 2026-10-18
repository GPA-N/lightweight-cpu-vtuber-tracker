# Upper-body benchmark (add-upper-body-tracking, tasks 1.1 to 1.3)

## 1. Summary

The full tracker (pose + one hand + face) does not meet the budget on this PC. Median per frame is 33.7 ms at 4 threads (the best setting), above the 30 ms gate. Frame rate is 24 to 29 FPS, below the 30 FPS gate. End-to-end median is under 50 ms at 2 and 4 threads, and peak working set is about 95 MB. Pose alone at 4 threads is 16.6 ms and passes its own share.

Recommendation: do not integrate yet. Next lever per design.md D6 is to move the body stages to a second inference thread, then re-measure. Choice recorded for the user; no integration code written.

## 2. Setup

- Machine: this Windows 11 PC (laptop). Power state and thermal state were not logged in the bench, so run-to-run spread is uncontrolled noise.
- Build: `target\release\bench.exe`, built with `CARGO_BUILD_JOBS=4`. Runtime: ort 2.0.0-rc.14, CPU execution provider.
- Input: `fixtures/private/golden_take/f*.jpg`, 236 frames. First 5 frames are warm-up, not timed.
- Each config and thread count is one process. Headline config repeated in four processes (one matrix run plus three repeats).
- Source: `crates/bench/src/body_bench.rs`, entry `bench.exe --body-bench CONFIG THREADS`.

## 3. Models (task 1.1 and 1.2)

All four files are in `models/upper_body/`. Hashes are listed in `models/SHA256SUMS`. Each file is under the 50 MB cap.

| File | Size (bytes) | SHA-256 (first 16 hex) | IR | Opset (ai.onnx) | Producer | Inputs | Outputs |
|---|---|---|---|---|---|---|---|
| pose_detector.onnx | 11,926,470 | 389df409fbf7f6b1... | 8 | 18 | tf2onnx 1.17.0 | input_1 NHWC 224 | Identity [1,2254,12], Identity_1 [1,2254,1] |
| pose_landmarks.onnx | 12,766,212 | 528137735df0bf09... | 8 | 18 | tf2onnx 1.17.0 | input_1 NHWC 256, 0..1 | Identity [1,195], Identity_1 [1,1], Identity_2..4 |
| palm_detection_mediapipe_2023feb.onnx | 3,905,734 | 78ff51c38496b7fc... | 7 | 13 | tf2onnx 1.13.0 | NHWC 192 | Identity [1,2016,18], Identity_1 [1,2016,1] |
| handpose_estimation_mediapipe_2023feb.onnx | 4,099,621 | db0898ae717b76b0... | 7 | 13 | tf2onnx 1.13.0 | NHWC 224 | Identity [1,63], Identity_1 [1,1], Identity_2 [1,1], Identity_3 [1,63] |

- All four load in ort 2.0.0-rc.14 on CPU with no error. No fallback needed.
- Opset and IR were read by a byte-only protobuf header scan (`fixtures/private/out/ub_inspect/scripts/opset.py`). The graph was never parsed or run.
- Output order for handpose (landmarks, score, handedness, world) is assumed from the shapes. Not verified against a reference. Palm detector was loaded but not run in this benchmark.
- Pose detector output is timed but not decoded in this benchmark.

## 4. Timing (task 1.3)

Frame time is the per-frame bench step (pose, hand, or both plus face, as named). Times in ms. Whether camera capture and drawing are inside the timed span was not checked against the bench source in this pass. Peak working set and private memory in MB.

| Config | Threads | Runs | Median | p95 | FPS | Peak WS | Peak private |
|---|---|---|---|---|---|---|---|
| pose | 1 | 1 | 28.26 | 30.20 | 35.1 | 73 | 58 |
| pose | 2 | 1 | 19.60 | 22.26 | 50.2 | 73 | 59 |
| pose | 4 | 1 | 16.60 | 18.50 | 59.4 | 74 | 59 |
| hand | 1 | 1 | 5.18 | 5.50 | 191.3 | 45 | 30 |
| hand | 2 | 1 | 4.00 | 5.22 | 243.7 | 45 | 30 |
| hand | 4 | 1 | 3.61 | 4.40 | 271.1 | 45 | 30 |
| full | 1 | 1 | 48.89 | 56.73 | 19.9 | 92 | 77 |
| full | 2 | 1 | 36.06 | 40.88 | 27.3 | 93 | 79 |
| full | 4 | 4 | 33.74 (33.67, 43.01, 43.11) | 42.37 (44.57, 48.61, 49.54) | 28.7 (28.6, 24.0, 24.1) | 95 | 81 |

Raw rows are in `fixtures/private/out/ub_bench/results.csv`.

Noise: the headline config (full, 4 threads) ranged from 33.7 to 43.1 ms median across four processes, a spread of about 9 ms (about 28 percent of the mean). The two higher runs may reflect power or thermal state. Treat the headline as roughly 34 to 43 ms median, not a single number.

Face cost is inferred, not measured alone: full minus (pose + hand) at 4 threads is about 13 ms, close to design.md's estimate of about 12 ms.

## 5. Gates (design.md D6, tasks 1.6 and 3.5)

| Gate | Target | Result (full, 4 threads) | Verdict |
|---|---|---|---|
| Total median with face | < 30 ms | 33.7 to 43.1 ms | FAIL |
| End to end | < 50 ms | median 33.7 to 43.1; p95 up to 49.5 | PASS (narrow) |
| Peak working set | <= 1024 MB | 94 to 95 MB | PASS |
| FPS | >= 30 | 24.0 to 28.7 | FAIL |

At 1 thread the end-to-end median is 48.9 ms (passes by under 2 ms). At 2 threads it is 36.1 ms (fails the 30 ms gate). design.md predicts 25 to 40 ms serial on one thread; the measured 48.9 ms is above that range.

Predictions vs measured (design.md):

| Stage | Predicted | Measured | Note |
|---|---|---|---|
| Pose landmarks, every frame | 6 to 12 ms | 16.6 ms at 4 threads (detector and landmarks both in the timed pose config) | Above prediction; detector is timed only, not decoded. Whether the bench runs the detector on every frame was not confirmed, so the combined figure is not split |
| Hand landmarks | 2 to 4 ms | 3.6 ms at 4 threads | Within range, but run on full frame with no palm detector |
| Palm | 2 to 4 ms | not measured | Not run in this bench |
| Tracker total with face | 22 to 30 ms | 33.7 to 43.1 ms | Above prediction |

## 6. Accuracy (photos and golden_take)

Accuracy notes are visual. The drawings were checked by eye against the photos. No ground truth exists for these photos, so these are judgements, not metrics.

Drawing key: cyan body points (visibility above 0.5), green hand points when the hand score is above 0.5, red when it is not.

### 6.1 Five user photos (`fixtures/private/upper_body_photos/hands_*.png`)

Annotated PNGs: `fixtures/private/out/ub_bench/photo_hands_{15,16,17,19,20}.png`.

| Photo | Size | Hands | Handedness | Finger curl | Wrist / elbow / shoulder | Hip visibility | Failures |
|---|---|---|---|---|---|---|---|
| hands_15 | 212x148 | 2 detected (both crops score about 0.73) | not verified | Plausible open palms, spread fingers drawn | Wrist points near the visible wrists; body points cluster on face and shoulders | Reported visible (about 0.97 to 1.00); hips are out of frame | Hip visibility false positive |
| hands_16 | 181x148 | Right hand: crop score 0.50 (raw 0.00), at threshold | not verified | Raised right hand drawn as open palm; confidence too low to trust | Right wrist visibility 0.015, so pose does not see the wrist; a green column on the left arm is not a hand | Reported visible; hips out of frame | Right wrist not seen by pose; left-arm points spurious |
| hands_17 | 547x365 | Not reliable. Both crop scores 0.5 to 0.55 | not verified | Fists raised against sky: no closed hand skeleton, only scattered dots | Elbow visibility 0.149 and 0.192; wrist visibility 0.322 and 0.419 | Hips out of frame (upper body only); reported visible | Failure: no clear hand shape on either fist |
| hands_19 | 447x447 | 1 detected (right crop score 0.73, raw 0.99); left crop no hand (score 0.50, raw 0.01) | not verified | Open palm on the viewer's left: clear skeleton on fingers | Face points correct; green dots near opposite elbow are spurious (left crop is below threshold) | Reported visible; hips out of frame | Caption text overlaps torso; spurious points near elbow |
| hands_20 | 447x447 | Identical to hands_19 | identical | identical | identical | identical | Byte-identical file: SHA-256 68CA9CF7D60F753D4538AAE2638143A4EF9F5B2C99BFCD2EEA4D8807A6787749. Not an independent sample. |

Across all five photos the pose model reports hip visibility of about 0.97 to 1.00, but in every drawing the image ends above the hips. This is a false-positive hip visibility, so hip visibility cannot be trusted for the sway system (design.md D5) without a check.

### 6.2 Golden_take frames (headline config, frames 0, 100, 200)

Annotated PNGs: `fixtures/private/out/ub_bench/full_frame000.png`, `full_frame100.png`, `full_frame200.png`.

- Frame 000: green hand points sit on the forehead and hair, not on the raised hand at the left edge. The raised hand gets no skeleton. Failure: hand crop lands on the head.
- Frame 100: motion blur. No hand points drawn.
- Frame 200: raised hand at the left edge, no hand points drawn. Face points are placed correctly.
- Hand scores on these frames were not summarised numerically. Judgement is from the drawings only.

Hand accuracy on golden_take is poor in these three frames.

## 7. Limitations

- Detector boxes are not decoded. Pose timing includes the detector run only.
- Wrist crops come from the landmark model, not from palm or pose detection.
- Handedness convention and sigmoid on hand score and handedness are not verified.
- Extension and finger curl values are proxies, not validated.
- Pose visibility is a sigmoid of an unverified output.
- Photos are small (148 to 447 px) and upscaled to 224 or 256.
- Only one crop source (previous-frame wrist landmarks) was run. Task 1.4 (crop-source comparison and jitter) was not run.
- Timing ran on this PC at the user's request. The project memory says heavy compute runs in the cloud. Flagged for the user.

## 8. Licences (task 1.3 record)

| File | Source | Licence | Evidence |
|---|---|---|---|
| pose_detector.onnx, pose_landmarks.onnx | fernandotonon/QtMeshEditor-blazepose-onnx, QtMeshEditor-poselandmarks-onnx | Apache-2.0 | Hub metadata recorded in `docs/notes/upper-body-research.md` (not re-fetched in this run) |
| palm_detection_mediapipe_2023feb.onnx, handpose_estimation_mediapipe_2023feb.onnx | opencv/palm_detection_mediapipe, opencv/handpose_estimation_mediapipe | Apache-2.0 | Model cards, verified 2026-10-17 (`docs/notes/ub-models.md`) |

Upstream model-weight licence text for the pose files was not read. Re-check before shipping.

## 9. Dates (design.md D9)

- Pose files: Hub update 2026-07-13 (per research note).
- Hand files: updated 2025-06-20. This is the recorded exception in design.md D1. Task 1.5 (one more search for a 2026 hand ONNX) was not run. The exception stands.

## 10. Task status

- 1.1 done: four files present, hashes recorded, under 50 MB.
- 1.2 done: all four load, I/O and opset recorded.
- 1.3 done: pose, hand, full at 1, 2, 4 threads measured. Gates reported above.
- 1.4 not done: crop-source comparison and jitter not measured.
- 1.5 not done: no new 2026 hand search. Exception recorded.
- 1.6 not closed: gate fails at 4 threads. Levers (design.md D6) not yet applied, so the decision is for the user.

## 11. Recommendation

Choice for the user: keep the current models (no switch). Apply the next lever in order: move the body stages to a second inference thread (task 3.5), then re-measure the headline config. Also fix hand placement on golden_take (frame 000 lands on the head) and the hip visibility false positive before any sway work relies on it.
