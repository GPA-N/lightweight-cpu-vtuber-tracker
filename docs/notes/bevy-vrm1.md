# bevy_vrm1 notes (task 1.2, cpu-face-tracking-spike)

Citation base: commit 671b71fca7c24f905f71d1be7698a350da262f0e of https://github.com/not-elm/bevy_vrm1 (committed 2026-10-04T01:32:05+09:00). File paths below are relative to that repo's `src/` directory unless marked otherwise (e.g. `Cargo.toml`, `README.md`, `examples/` are at repo root; `src/...` is under `src/`).

## Version and Bevy requirement

- Crate 0.10.0: `Cargo.toml` line 3. Crates.io publish date: NOT FOUND (crates.io API fetch returned no JSON in this session).
- Bevy 0.19: `Cargo.toml` line 15. README table (0.8.0+ = Bevy 0.19): `README.md` ~lines 123-126.
- Non-Bevy deps (`Cargo.toml` lines 15-29): serde, serde_json, anyhow, bitflags, paste, indexmap. No `ort`, no `nalgebra`, no onnx/ML crate (grep for ort|nalgebra|onnx: no match). Windows-only: `windows = "0.62"` under `[target.'cfg(target_os = "windows")'.dependencies]` (Win32_UI_WindowsAndMessaging, Win32_Foundation). Conflict with an ONNX runtime or nalgebra in our project: none from this crate's manifest; transitive Bevy deps not checked.

## Expression weights at runtime (YES, public API; morph fallback not needed)

- Introduced by PR #48, merged 2026-02-14 (`feat(expressions): add expression control API with VRM 1.0 spec-compliant override system`), commit 47a7128e6: https://github.com/not-elm/bevy_vrm1/pull/48 (found via `gh api repos/not-elm/bevy_vrm1/commits?path=src/vrm/expressions.rs`).
- `SetExpressions` `src/vrm/expressions.rs` line 139; `single(` 147; `from_iter(` 159.
- `ModifyExpressions` line 195; `single` 206; `from_iter` 218; `mouth(` 249; `mouth_weights(` 283.
- `ClearExpressions { #[event_target] pub entity: Entity }` line 303.
- Doc example `commands.trigger(SetExpressions::single(vrm, "happy", 1.0));` line 134. Example: `examples/expressions.rs` (`SetExpressions::single`, `ModifyExpressions::mouth(vrm, "aa", 1.0)`, `ClearExpressions { entity: vrm }`).
- Applied by observers `apply_set_expressions` (line 569), `apply_modify_expressions` (602), `apply_clear_expressions` (628), registered at lines 388-391.
- Per-frame write: `bind_expressions` (line 464) is added in PostUpdate, `.in_set(VrmSystemSets::Expressions).after(VrmSystemSets::GazeControl)` (lines 392-397). No `run_if`, so it runs every frame over all expression entities. Morph write at line 563 (`morph_weights.weights_mut()[index] += final_weight * bind_weight;`).
- Cost: Set/Modify/Clear are one-shot observers. Per-frame cost is `bind_expressions` iterating all rig expression entities (query at lines 466-471) and writing morph weights. Not benchmarked (out of scope).
- Default with no override: `None => tf.translation.x` (line 499). Override via `ExpressionOverride(pub f32)` (line 115). Binary expressions threshold at `raw_weight > 0.5` (line 503). Non-binary output clamped to 0..1 (line 505; doc line 120).
- Components: `EffectiveExpressionWeight` line 104; `ExpressionEntityMap` line 109.
- Name handling: `SetExpressions` looks up `map.0.get(expression)` (line 591); missing name logs `warn!("SetExpressions: expression '{}' not found", expression)` and skips (line 593). `ModifyExpressions` does the same at lines 617-619. Missing map: warn at line 578 (Set) and 611 (Modify).
- Preset names: `"aa" | "ih" | "ou" | "ee" | "oh"` → Mouth, `"blink" | "blinkLeft" | "blinkRight"` → Blink (`src/vrm/expressions.rs` lines 25-26). `MOUTH_EXPRESSIONS` line 202. Other preset names (happy etc.) appear in tests and docs.
- ARKit names (jawOpen, eyeBlinkLeft, ...): NOT FOUND. Grep for `arkit|jawOpen|eyeBlinkLeft` over `src/`: no match.

## Loading a VRM

- `VrmHandle(pub Handle<VrmAsset>)`: `src/vrm/loader.rs` line 87. Doc ~lines 62-75 lists auto-inserted components.
- Example: `examples/simple.rs`: `commands.spawn(VrmHandle(asset_server.load("vrm/Elmer.vrm")));`
- `VrmPlugin` `src/vrm.rs` lines 142-159.

## Head bone and Transform

- `Head` variant: `src/vrm/humanoid_bone/bones.rs` line 83; `.register_type::<Head>()` line 147.
- `HeadBoneEntity` is defined by the `entity_component!` macro invocation in `src/vrm/humanoid_bone/bones.rs` lines 17-25 (doc: "automatically inserted into the entity of VRM or VRMA"). `bones.rs` line 202 is only `.register_type::<HeadBoneEntity>()`.
- Insertion onto VRM root: `src/vrm/humanoid_bone.rs` line 80: `$commands.entity($vrm_entity).insert([<$bone BoneEntity>]($bone_entity));`
- Transform read: `track_looking_target` in `src/vrm/look_at.rs` takes `&HeadBoneEntity` (line 68) and `transforms: Query<&Transform>` (line 72); head read at line 83 (`transforms.get(head.0)`). Head Transform as a component on the head bone entity: inferred from these lines, not stated in a doc.

## Spring bones, LookAt, body tracking, schedule

- All of these run in PostUpdate. Ordering (`src/vrm.rs` lines 165-178): PropagateAfterConstraints → GazeControl → Expressions → PropagateAfterExpressions → SpringBone. Set enum: `src/system_set.rs` lines 4-24.
- Spring bones: `VrmSpringBonePlugin` added by `VrmPlugin` (`src/vrm.rs` line 151). `update_spring_bones` in PostUpdate (`src/vrm/spring_bone/update.rs` lines 16-21), set SpringBone. Runs automatically; no component needed. Joint Transform/GlobalTransform query at `update.rs` line 26; it does not write the head bone by name.
- LookAt: `LookAtPlugin` (`src/vrm.rs` line 156); `track_looking_target` in PostUpdate, `.in_set(VrmSystemSets::GazeControl)` (`src/vrm/look_at.rs` lines 54-57). Opt-in: doc `src/vrm/look_at.rs` lines 10-11 ("should be inserted into the root entity of the VRM"). It does not write the head bone's rotation (line 90 writes a local temp transform only).
- Body tracking: `src/vrm/body_tracking.rs` lines 122-128: `track_body_tracking` in PostUpdate, `.in_set(VrmSystemSets::GazeControl)`, `.run_if(any_with_component::<BodyTracking>)`. Fn at line 252; chain takes `HeadBoneEntity` (line 258) and optional Neck/Spine (259, 261). Bone get_mut guard at line 405 (`let Ok((mut tf, mut gtf)) = transforms.get_mut(bone.entity) else {`); the rotation assignment is `tf.rotation = ...` at line 427 (additive gaze delta with slerp smoothing when `output_smoothing > 0.0`). So body tracking DOES write head rotation when a `BodyTracking` component is present on the VRM. Not present by default.
- First-person: `src/vrm/first_person.rs` lines 255, 279, 360 read `HeadBoneEntity` for auto-split logic in Update (line 54). Does not write head rotation (no Transform write found in the lines read).
- Overwrite answer for our own head-rotation system: bevy_vrm1 writes head rotation only in body tracking (opt-in, PostUpdate, line 427). Spring bones and expressions do not write the head bone. LookAt writes no head rotation. If our system runs in Update, bevy_vrm1 PostUpdate systems run after it and body tracking (if enabled) would overwrite/offset. If it runs in PostUpdate, order it after GazeControl and before PropagateAfterExpressions, or disable BodyTracking.

## Issue #38

- Sources: https://github.com/not-elm/bevy_vrm1/issues/38 ; API: https://api.github.com/repos/not-elm/bevy_vrm1/issues/38 and https://api.github.com/repos/not-elm/bevy_vrm1/issues/38/comments
- State: CLOSED, closed_at 2025-12-19T12:52:35Z, state_reason "completed". Opened by terapoon.
- Maintainer (not-elm) comment 2025-09-26T10:04:06Z, verbatim sentence: "From v0.2.2 through to the current main branch, there is no API offered for specifying and controlling expressions." (The comment is a reply to a question about crate 0.2.2.)
- Maintainer closing comment 2025-12-19T12:52:35Z: "This has been resolved with the following PR, so it will be closed." followed by "- #40".
- PR #40: https://github.com/not-elm/bevy_vrm1/pull/40 . Title "Fix: Implement VRM specification-compliant system execution order" (confirmed via `gh api repos/not-elm/bevy_vrm1/pulls/40`). merged_at 2025-12-13T10:39:25Z (UTC). Merge commit bbfddd91ae019df0d0e375b313aeda6778dd6a30. Its changed files include `src/system_set.rs`, `src/vrm.rs`, `src/vrm/look_at.rs`, `src/vrm/spring_bone/update.rs`, and no `src/vrm/expressions.rs`. So "resolved by PR #40" is unsupported for the expression API.
- The expression API came from PR #48 (merged 2026-02-14), after the issue was closed. The closing reference to #40 looks wrong.

## Per-frame and environment notes

- Examples use `DefaultPlugins` (e.g. `examples/expressions.rs` line 19, `examples/body_tracking.rs` line 6), so they need a window and a GPU/render backend. Body tracking also queries `Window` (`src/vrm/body_tracking.rs` line 268).
- Unit test harness `src/lib.rs` lines 19-40 uses `MinimalPlugins` plus `WindowPlugin::default()`. Whether the tests need a GPU: NOT FOUND (not run, per brief).

## NOT FOUND

- docs.rs pages for bevy_vrm1 0.10.0: NOT FOUND. Not attempted.
- ARKit blendshape names accepted by SetExpressions: NOT FOUND (no match in `src/`).
- Explicit doc statement that the head entity carries `Transform`: NOT FOUND. Inferred from `src/vrm/look_at.rs` lines 68-83 only.
- Whether any transitive Bevy dependency pulls ort/nalgebra: NOT FOUND (only the crate's direct manifest checked).
- Test GPU requirement: NOT FOUND (tests not run).
- Crates.io publish date for 0.10.0: NOT FOUND (API fetch failed).
