# VRM compatibility check

Question: does the Bevy avatar app (`target/release/app.exe`, `APP_REPLAY` mode, `crates/avatar`) work with other VRM models?

Method: static inspection of each GLB with `python -I` (read-only JSON chunk parse), then replay of `fixtures/private/golden_take/track.txt` (236 rows, 227 found, 15.0 s) through the existing binary. No rebuild, no edits to `crates/`, no git actions. Downloaded models were never executed.

## Compatibility table

| Model | VRM version | Static inspection | Loads | Frames saved | Verdict |
|---|---|---|---|---|---|
| Seed-san (control) | 1.0 | Full humanoid, 18 expression presets, 9 springs, 10 MToon materials | Yes | 466 | Works |
| VRM1 Constraint Twist Sample | 1.0 | Full humanoid incl. eyes and upperChest, 22 springs, 13 colliders, node constraints | Yes, after the bevy_vrm1 patch (2026-10-17). Before: silently rejected, no `ExpressionEntityMap` | 466 | Loads, but head renders tilted (see Update below) |
| Godette | 0.x (title "Low-Poly Godette", CC_BY) | `extensions.VRM` only, 17 blendShapeGroups, no VRMC_vrm | Rejected at validation | 0 | Incompatible with this app |
| 100Avatars_031_Devil | 0.x (title "Devil", CC0) | `extensions.VRM` only, 6 blendShapeGroups (a,i,u,e,o,blink), no leftEye | Rejected at validation | 0 | Incompatible with this app |

## Update 2026-10-17: Twist fixed (load), head pose still wrong

- Cause of the "stall": bevy_vrm1 0.10.0 requires four optional VRM 1.0 expression keys, so the parse failed silently (`docs/notes/twist-stall.md`). Fixed by the vendored crate `vendor/bevy_vrm1` (`PATCHES.md`), OpenSpec change `fix-vrm-compat`. Test: `crates/avatar/tests/vrm_expression_defaults.rs`.
- Replay of the golden take with the new `app.exe`: Twist writes 466 frames, Seed-san still 466.
- Screenshots viewed for Twist (`fixtures/private/out/fix_twist/twist_{10,191,208,315}.png`): the model is on screen and the body framing is sane (arms, shirt, legs cut at the bottom edge), but the head is bent far back and sideways in every frame including neutral, with the hair covering the face. Mouth and eyelids cannot be judged from these frames. Seed-san frames (`fix_seed`) look correct.
- Likely cause, not yet fixed: `crates/avatar/src/head.rs` sets the head bone `rotation` to the tracking quaternion directly. Twist head node 25 has a local rest rotation of about (0.468, 0.530, 0.468, 0.530), about 120 degrees on a diagonal axis, so overwriting it flips the head. Seed-san head rest is about -9 degrees, so it hides the problem.
- Enabling the bevy_vrm1 `log` feature now shows MToon parse errors: Seed-san 1 (`missing field texCoord`), Twist 12 (`missing field extensions`). Rendering still works; not investigated.

## Update 2026-10-17 (2): head rest rotation fixed

- `crates/avatar/src/head.rs` now sets the head local rotation to `rest * rest_global^-1 * tracked * rest_global` (`local_from_delta`), the same formula bevy_vrm1 uses in `body_tracking.rs` `bone_rotation` and in the VRMA retarget. A neutral tracked rotation keeps the rest pose; any rest rotation gives the same world-space motion. Seed-san (rest about -9 degrees) is visually unchanged. Only the head bone is written directly by the app; neck and other bones are not touched.
- Tests (`head.rs`): `large_rest_rotation_gives_same_world_motion_as_identity_rest` and `system_keeps_rest_rotation_for_neutral_and_applies_delta`. Both failed with the old direct assignment (identity stub), pass now.
- Replay of the golden take, frames in `fixtures/private/out/fix_head_{twist,seed}`. Twist frames 10, 191, 208, 315: head upright, face visible, hair behind the face. Frame 208 (jawOpen peak) is clearly different from the old `fix_twist/twist_208.png`, which showed the back of the head with the hair flung sideways. A first Twist run wrote blank 72-byte PNGs after frame 10 (cause unknown, probably the window was throttled); a second identical run was fine.
- Head turn (frame 191, tracked yaw about +37 degrees about +y): both models turn the face toward screen-right, so Twist and Seed-san agree. Whether that is the user's left or right depends on the camera mirror, which was not checked here.

## Evidence

### Seed-san (control)
- Replay exited on its own after 17.3 s, 466 PNG frames written, no errors in stderr.
- Screenshots checked: `av_0010.png` (neutral, framing and T-pose visible), `av_0208.png` (jaw peak at t 6.94 s, mouth opens only slightly because the golden take peaks at jawOpen 0.30), `av_0315.png` (blink, eyeBlinkLeft 0.80 at t 10.5 s, clearly closed lids), `av_0191.png` (head turned, t 6.4 s).
- CPU average 6.9% of 12 logical processors, peak working set 448.7 MB. Both sampled once per second, so approximate.
- Frame count is 466, not the ~600 expected from 150 settle frames plus 15 s at 30 fps. Not resolved; the first frames may be skipped until the model is ready, or the screenshot pipeline lags.

### VRM1 Constraint Twist Sample
- Static: valid VRM 1.0, spec-complete expression presets. Morph binds exist for aa, ou, ih, ee, oh, blink, blinkLeft, blinkRight, happy, sad, angry, surprised, relaxed, neutral. lookUp/Down/Left/Right have no morph binds.
- Run 1 (150 s): window created on AMD Radeon Vulkan, no ERROR lines, 0 frames, CPU 3.1%, peak WS 308 MB.
- Run 2 (400 s, same binary): 0 frames, CPU 2.2%, peak WS 288 MB.
- The replay waits for bevy_vrm1 to insert `ExpressionEntityMap` on the model root. That never happened, while Seed-san reached it quickly. The low CPU points to a stall in load or initialise, not slow decoding.
- bevy_vrm1 0.10.0 does implement `VRMC_node_constraint` (`src/vrm/node_constraint*.rs`), so the constraints alone are not the explanation. Cause not identified. Next step: run with `RUST_LOG=bevy_vrm1=debug` (no rebuild needed if the env var is respected) to see which initialise step stops.

### Godette and Devil (VRM 0.x)
- Both exit in 1.1 s with exit code success and no frames.
- stderr: `error: not a valid VRM 1.0 file: ... (VRM 0.x is not supported, expected VRM 1.0)`, from `validate_vrm1` in `crates/avatar/src/errors.rs`.
- The bevy_vrm1 README states "This crate only supports VRM 1.0.", and the crate only parses the `VRMC_vrm` extension. The rejection is therefore correct for this library, not a bug in the app. It is an incompatibility by design.

## Predictions vs results

Predictions were not written down before the runs, so this is not a prediction test. Results against the expectations in the brief:
- Seed-san (control) works: matches.
- Twist (VRM 1.0, constraints, springs) expected to load: does not load. Surprise.
- Godette and Devil (VRM 0.x) expected to be rejected: rejected as predicted.

## Fix list

1. VRM 0.x support. bevy_vrm1 is VRM 1.0 only. Options: a 0.x loader (large), or an offline conversion to VRM 1.0 (for example with a 0.x to 1.0 converter) before the app loads the file. The app should keep the `NotVrm1` message, but the message should suggest conversion.
2. Twist stall. Find which bevy_vrm1 load step never completes. Add a timeout and a clear error when `ExpressionEntityMap` is not inserted, so the app does not sit silently.
3. ARKit-to-preset mapping for VRM 1.0. `data/blendshape_map.json` drives the preset names. Check that aa, ou, ih, blink, blinkLeft and blinkRight are mapped for models whose morph binds differ from Seed-san. lookUp/Down/Left/Right are unbound on Twist, so gaze mapping must fall back gracefully.
4. Camera framing. The fixed camera at (0, 1.35, 1.8) looking at (0, 1.2, 0) was tuned for Seed-san. Taller or shorter models need a fit to the head bone height, or the face may leave the frame.
5. Spring bones. Verify that spring wiring works on models without `VRMC_springBone` (Godette and Devil have none, but the check only matters once 0.x is supported) and on models with many colliders (Twist, 13 colliders).
6. Measurement. Use per-frame sampling or a longer capture to get reliable CPU and working-set numbers. Investigate the 466-frame count versus the ~600 expected.

## Summary

- Seed-san (VRM 1.0) runs end to end and renders all four target poses.
- Twist (VRM 1.0) stalls before `ExpressionEntityMap`, with no error, for 400 s.
- Godette and Devil (VRM 0.x) are rejected by `validate_vrm1` as designed, because bevy_vrm1 is VRM 1.0 only.
- Only one VRM 1.0 model works, so the app is not yet broadly compatible.
