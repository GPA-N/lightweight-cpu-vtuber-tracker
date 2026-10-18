# Photo to avatar: still-image slice (add-upper-body-tracking)

Date: 2026-10-17. Scope: four photos (`hands_15`, `16`, `17`, `19`; `hands_20` is a byte-identical copy of 19) are run through a new upper-body stage and the result poses the VRM avatar (arms, hands, fingers, torso roll), rendered to a PNG. Still images only. No camera, no live integration, no second thread.

All hand results below are PROVISIONAL (2023 port): the hand model is the opencv_zoo MediaPipe palm detector plus handpose of February 2023. It is a stopgap behind a trait (`infer::upper_body::HandStage`); no replacement model is chosen yet. Limbs come from the pose model only.

## How to run

```
powershell -File scripts\photo_to_avatar.ps1 -Model seed|twist -Photo hands_19
python -I scripts/photo_to_avatar_compare.py hands_19     # pixel error vs official MediaPipe (needs seed run)
python -I scripts/photo_to_avatar_compose.py              # composites
```

The script sets `APP_STILL_BODY` (still mode, camera never opened), kills the process after 150 s, and writes to `fixtures/private/out/photo_to_avatar/`: `{model}_{photo}.png` (avatar), `_lm.png` (photo with landmarks, mirrored like the webcam preview), `.json` (landmarks), `.txt` (log with `LIMBS` solver numbers and `RENDERED` numbers read back from the engine), `composite_{model}_{photo}.png`. Models: Seed-san (`seed`) and VRM1 Constraint Twist Sample (`twist`). Debug build.

Incident: one run of `app.exe` without the still env var started the live camera path (the camera light came on; the lead killed it). Cause: my shell env had not carried over. The script now always sets `APP_STILL_BODY`, refuses to start without it, and kills on timeout.

## What was built

| Piece | File | Tests |
|---|---|---|
| Data types (33 pose points with visibility and metric world coords, 21-point hands, side, score, handedness, model label) | `crates/core/src/body.rs` | 1 |
| Stage: pose detector anchor decode, body ROI (hip-centre keypoint only places the crop), pose landmarks, palm detector anchor decode, NMS, hand ROI, hand landmarks, side assignment by nearest pose wrist; hand model behind `HandStage` | `crates/infer/src/upper_body.rs` | 12 |
| Retarget: mirror in one function, world-direction solve with per-model rest, hand roll from palm normal, finger curl, torso roll, hold-at-rest for missing joints | `crates/avatar/src/body.rs` | 12 |
| Avatar apply (`BodyPlugin`, `run_with_body`), only registered in still-body mode, live path unchanged | `crates/avatar/src/window.rs`, `body.rs` | 0 (visual) |
| `APP_STILL_BODY` mode, overlay, JSON dump | `crates/app/src/main.rs`, `still_body.rs` | 0 |

`cargo test --workspace -q`: all pass. `cargo clippy --workspace --all-targets -q`: two pre-existing warnings in `crates/bench/src/body_bench.rs` (`is_multiple_of`, `map_or`), none from the new code. No release build was made (debug builds only).

Rules kept: hips are never read for anything (a test sets hips to NaN and far away); mirror applied once in `to_avatar_space` (person's left arm drives the avatar's right bones, like the mirrored preview); rest direction per bone from `RestGlobalTransform` (test with a large 120 degree rest rotation, T-pose versus A-pose within 1 degree); missing bones skipped (no upperChest, no shoulders, no fingers, empty rig: no panic).

## Findings that changed the bench conclusions

- The old bench cropped the pose from the whole letterboxed image: shoulders landed on the face. With the pose detector ROI the pose matches the official MediaPipe reference (table below).
- Pose presence output (`Identity_1`) is already a probability; the sigmoid applied to it was wrong. Hand score and handedness are probabilities too (the bench applied a sigmoid to them, so its "0.50 to 0.55" scores were meaningless).
- The palm detector needs 0..1 input in this port (-1..1 gave no detection).
- Handedness raw below 0.5 matched the geometric left hand and above 0.5 the right hand on all six hands, for un-mirrored photos.
- Hip visibility from this pose ONNX is still higher than official (0.95 to 0.99 against 0.37 to 0.94; hands_15 0.98 against 0.38). Not used anywhere. Cause not found.

## Accuracy against official MediaPipe (pose_full and hand_landmarker)

Pixel error in source pixels, `scripts/photo_to_avatar_compare.py`.

| Photo | Size | Mean error of shoulders, elbows, wrists, index points | Worst point | Hands (ours / official) | Hand error: wrist, index tip, mean of 21 |
|---|---|---|---|---|---|
| hands_15 | 212x148 | 3.5 px | 5.3 (index) | 2 / 2 | 2.1, 0.6, 0.6 (left); 0.2, 0.7, 0.4 (right) |
| hands_16 | 181x148 | 4.6 px | 11.7 (left elbow) | 1 / 1 | 2.4, 1.1, 0.7 |
| hands_17 | 547x365 | 5.5 px | 12.9 (left elbow) | 2 / 2 | 2.3, 1.2, 2.0 (left); 4.0, 2.1, 1.5 (right) |
| hands_19 | 447x447 | 12.7 px | 25.0 (left index, vis 0.65) | 1 / 1 | 6.8, 1.4, 1.6 |

Hand scores (ours): 0.985 to 0.997 where a hand exists. Hands_17 fists are now detected (they failed in the bench).

## Limb numbers: tracked versus rendered

Elevation is the angle above the horizontal (negative = pointing down); bend is the angle between upper arm and forearm. "Tracked" is the direction from the pose world landmarks, "avatar" is read from the engine bone `GlobalTransform` after the frame (line `RENDERED` in the logs). Identical numbers on both models and in all 8 runs. Left/right here are the avatar's own bones (the person's right arm in the mirrored preview).

| Photo | Avatar-left: upper arm / forearm / bend, tracked = avatar | Avatar-right: upper arm / forearm / bend, tracked = avatar |
|---|---|---|
| hands_15 | -54 / 21 / 76 | -59 / 28 / 92 |
| hands_16 | -83 / -87 / 9 | -21 / 23 / 53 |
| hands_17 | 15 / 47 / 35 | 0 / 28 / 38 |
| hands_19 | -19 / 19 / 73 | -69 / -81 / 12 |

Error between tracked and rendered direction: 0.0 degrees on every limb (the solver gets the direction exactly; the check is that the engine really renders it).

Caveat: the numbers prove the avatar follows the tracked 3D direction. They do not prove the 3D direction is right: depth comes from the model's world landmarks and I only checked it against the 2D picture by eye (hands_17 in-plane bend about 62 and 42 degrees against 35 and 38 tracked, so depth reduces the bend; plausible, not verified).

## Results table (viewed with the Read tool, composites listed below)

| Photo | Model | Hands detected | Handedness raw (geometric side) | Limbs on the avatar | Fingers (PROVISIONAL) | Verdict |
|---|---|---|---|---|---|---|
| hands_15 (two open palms out) | twist | 2 (0.99, 1.00) | 0.05 (left), 0.87 (right) | Both arms right: upper arms down and out, forearms up, hands at shoulder height, correct sides | Open palms, fingers extended | Good |
| hands_15 | seed | 2 | same | Same angles. Seed-san's avatar-left arm is a long mechanical arm model part and runs off the right edge of the crop; the other arm is fine | Open, hard to see on the mech hand | Good, framing cuts the mech arm |
| hands_16 (one palm raised, low confidence in the bench) | twist | 1 (0.996) | 0.12 (left hand of person) | Raised arm on the screen-left side, forearm up and toward camera, hand open; other arm hangs. No twist or flip | Open palm | Good (bench failure was the crop, not the photo) |
| hands_16 | seed | 1 | same | Same: raised arm screen-left, other hangs along the mech arm; hand tilted | Open | Good |
| hands_17 (fists up, failed in the bench) | twist | 2 (0.99, 0.99) | 0.006, 0.98 | Arms out and slightly up, forearms raised, fists; sides correct | Closed, thumb pointing up on one hand; curl visible but shallow | Good limbs, fingers plausible |
| hands_17 | seed | 2 | same | Same pose; the mech arm leaves the frame at top right | Closed | Good limbs, framing cuts the mech arm |
| hands_19 (palm toward camera, other arm down) | twist | 1 (0.997) | 0.92 (right) | Raised arm folds back toward the neck (hand toward camera, 73 degree bend), other arm hangs | Open palm, small in frame | Plausible; depth ambiguity |
| hands_19 | seed | 1 | same | Same: mech arm folds up and back over the shoulder, normal arm hangs | Open | Plausible; the mech arm is large |

Failures and gaps seen:
- Seed-san's avatar-left arm is a much longer mechanical arm than the other side; in several renders it leaves the crop. This is a model property, not a retarget error. The camera framing is the default one.
- Depth (z) is the weakest signal: hands_19 relies on a forearm pointing at the camera. Not checked against ground truth.
- Fingers: curl only (no splay, no thumb opposition). Thumb is damped. The hand model is provisional.
- Forearm twist is not driven (hand roll takes it all, clamped to 90 degrees). Shoulders are only a small shrug when the arm is above horizontal. Torso is roll only from the shoulder line.
- Face tracking ran on the photos too (face found on 15 and 16, not on 17 and 19 because the faces are small or tilted); head pose is applied as before.
- Hip visibility from this ONNX disagrees with official (see above).
- Debug build only; no timing was taken.

Composites (photo with landmarks, mirrored | avatar): `fixtures/private/out/photo_to_avatar/composite_{seed,twist}_{hands_15,hands_16,hands_17,hands_19}.png`. Raw renders are `{model}_{photo}.png` in the same folder.

## NOT done

- Live camera integration, `BodyOutput` in `TrackingOutput`, `--no-body` switch, tracker step.
- Latency gate (30 ms with face), 30 FPS gate, memory gate; no second inference thread (the 2026-10 bench says the serial full pipeline misses the gate; this slice adds a palm detector and a pose detector run, so it is slower still). Nothing was measured on this slice.
- Task 1.4 (crop source comparison, jitter), 1.5 (a 2026-dated hand model), 1.6 decision.
- `BodyLost` hold/ease, render-rate `smooth_body`, One Euro on landmarks (a still needs none).
- Lower-body sway.
- Godette and Devil (VRM 0.x, loader blocks). No model without upperChest was available, covered by a synthetic test only.
- Golden_take check of the new stage (2.1 asks for one frame against the bench), a hand check on golden_take frames (the official reference finds hands there that the old bench missed).
- Bench `--body-bench` still uses the old pipeline (wrong crop, sigmoid on probabilities).

## Remaining work as small subtasks

Each one is a single change with one verification command. Order matters only where noted. Hand model choice is open (do not lock one): subtasks 6 and 7 wait for the user.

1. Bench uses the new stage.
   - Files: `crates/bench/src/body_bench.rs` (replace `run_pose`, `run_hand`, `Ctx` pose/hand with `infer::upper_body::UpperBody`; remove the two sigmoids).
   - Input: `fixtures/private/golden_take/f*.jpg`, `models/upper_body/*.onnx`.
   - Expected: `bench.exe --body-bench full 4` prints one `RESULT` line with median_ms, p95_ms, fps, peak_ws_mb; same columns as `docs/notes/ub-benchmark.md` section 4.
   - Verify: `$env:CARGO_BUILD_JOBS='4'; cargo build --release -p bench; .\target\release\bench.exe --body-bench full 4` (not the live camera; reads files only).
2. Per-stage timing in `UpperBody`.
   - Files: `crates/infer/src/upper_body.rs` (add `pub struct BodyTimes {pose_det_ms, pose_lm_ms, palm_ms, hand_ms}` filled in `run`, returned next to the output or stored in a field).
   - Expected: a unit test with fake times is not possible, so test only that the struct has four fields and defaults to 0; real values appear in the subtask 1 RESULT line.
   - Verify: `cargo test -p infer upper_body`.
3. Check the new stage on golden_take (task 2.1 and 2.2 visual check).
   - Files: none; run `APP_STILL_BODY=fixtures\private\golden_take\f0100.jpg` through `scripts\photo_to_avatar.ps1` after adding `-PhotoPath` support (edit the script to accept a path), then `scripts/photo_to_avatar_compare.py` with the `f0100` reference at `fixtures/private/out/mp_hands_ref/f0100.json`.
   - Expected: mean pose error under 15 px on shoulders and wrists, hand found (official: Right 0.99, wrist about (48,362)).
   - Verify: the compare script prints the table; paste into this file.
4. Hip visibility mismatch.
   - Files: none (investigation). Compare `pose[23].visibility` from `*.json` dumps with `mp_hands_ref/*.json` for all four photos; check whether the ONNX file is the lite or heavy variant (`docs/notes/ub-models.md`) and whether multiplying visibility by presence (output `Identity` column 4) matches.
   - Expected: one paragraph in this file with the cause or "unknown", plus a rule: hips stay unused.
   - Verify: python -I one-off script printing the two columns.
5. Clamp table and rig-without-upperChest test (task 4.2).
   - Files: `crates/avatar/src/body.rs`: replace the `ELBOW_MAX`, `WRIST_ROLL_MAX`, `TORSO_ROLL_MAX` constants by one `fn clamp_for(bone: &str) -> f32` table keyed by humanoid name; tests already cover rigs without upperChest and shoulders.
   - Expected: same test results; new test that `clamp_for("leftLowerArm")` is 150 degrees.
   - Verify: `cargo test -p avatar --lib body`.
6. (Wait for user) Implement `HandStage` for the chosen replacement hand model.
   - Files: `crates/infer/src/upper_body.rs` (new struct implementing `HandStage`, own decode function with unit tests like `decode_mp2023`), `crates/app/src/main.rs` (one constructor line).
   - Expected: `scripts\photo_to_avatar.ps1` on all four photos gives hands detected (2, 1, 2, 1) and the compare script reports hand mean error under 5 px against official; label no longer says PROVISIONAL only if the model is 2026-dated.
   - Verify: the compare script output.
7. (Wait for user) Add the hand model to `models/`, `models/SHA256SUMS`, `docs/notes/licenses.md`.
   - Verify: `sha256sum -c models/SHA256SUMS`.
8. Tasks 3.1 and 3.2: `BodyOutput` and tracker step.
   - Files: `crates/core/src/frame.rs` (add `body: Option<UpperBodyOutput>` to a new sibling struct, not to `TrackingOutput`), `crates/infer/src/tracker.rs` (optional body stage; with `--no-body` no session is built).
   - Expected: existing tracker tests unchanged and green; one new test that with the body stage off the output is `None` and the fake runners are not called.
   - Verify: `cargo test -p infer -p core`.
9. Task 3.4: `BodyLost` in `crates/smoothing` (five parts, hold 0.5 s, ease 1.0 s), modelled on `face_lost.rs` tests.
   - Verify: `cargo test -p smoothing`.
10. Task 5.2: `smooth_body` in `crates/avatar/src/window.rs`: slerp of the solved local rotations with `follow_factor(dt, 0.03)` for hands and `0.08` for torso; no per-frame step above 15 degrees when a hand leaves.
    - Verify: `cargo test -p avatar --lib`.
11. Task 3.5 and 7.1: latency gate with the body stage. Needs subtasks 1, 8. Run the release bench (files only), then the live runs only when the user is at the laptop (they use the camera).
    - Expected: median at most 30 ms with face; otherwise subtask 12.
12. If 11 fails: move the body stages to a second inference thread (`crates/bench/src/pipeline.rs` `spawn_inference`), re-measure with subtask 1.
13. Task 8.2: A-pose rig check. Run `scripts\photo_to_avatar.ps1` on a second VRM 1.0 with A-pose from `samples/` if one exists; the log `RENDERED` lines must equal `LIMBS` lines within 1 degree.
14. Task 6: lower-body sway (separate, design.md D5).
15. Task 9.1 to 9.3: dates, licences, `openspec validate add-upper-body-tracking --strict`.
