# VRM1_Constraint_Twist_Sample does not load in APP_REPLAY

Status: ROOT CAUSE CONFIRMED and fix verified (2026-10-17). It is not a stall or deadlock. The VRM is silently rejected at parse time. The older sections below ("Symptom" onward) are the previous worker's investigation, superseded by this block.

## Cause

bevy_vrm1 0.10.0 `src/vrm/gltf/extensions/vrmc_vrm.rs`, struct `VrmPreset`, requires `isBinary`, `overrideBlink`, `overrideLookAt`, `overrideMouth` (no `#[serde(default)]`). In the VRM 1.0 spec these are optional (defaults: `false`, `"none"`, `"none"`, `"none"`). The Twist sample omits them in every preset (Seed-san writes them, so it loads).

Chain:
1. `initialize.rs::spawn_vrm` (line ~58) calls `VrmExtensions::from_gltf`; serde fails with `missing field overrideBlink`.
2. The match arm calls `vrm_error!("Failed to load VRM extensions", e)` then `continue`. `vrm_error!` (`src/error.rs`) only logs with the crate feature `log`, which our app does not enable, so nothing is printed.
3. `VrmHandle` was already removed one line earlier, so the entity never gets `Vrm`, `HumanoidBoneRegistry`, etc. `request_initialize` never matches it, `ExpressionEntityMap` is never inserted, and the replay waits forever (window.rs gates on `With<ExpressionEntityMap>`).

Why the earlier bisect pointed at a non-empty preset map: an empty `preset` or no `expressions` block has nothing to deserialize into `VrmPreset`. Any single preset without the override keys fails. The earlier `nooverride` variant removed keys that were already absent, so it proved nothing. Morph, constraint, spring and texture strips were irrelevant.

## Evidence

- cdb attach at 20 s (`fixtures/private/spikes/twist-tools/sample_stacks.sh`, output `fixtures/private/out/cdb/stacks.txt`): no thread computes or waits on a lock. Main thread is in the winit event loop (`about_to_wait` -> `create_monitors`), the pipelined render thread is parked, all IO and compute pool threads idle on futex. The app is alive; the model never initializes. No deadlock.
- JSON diff: Twist presets are `{"isBinary":false,"morphTargetBinds":[...]}` (no override keys); Seed-san presets carry all three override keys. `twist-tools/expr_dump.py` prints both.
- Data-only check: `twist-tools/try_variant.sh addoverride` (strip.py mode `addoverride` adds `isBinary` and the three overrides = "none" to each preset, BIN chunk untouched) -> exit 0, 475 log lines, 466 "Screenshot saved".
- Code check: patched bevy_vrm1 (below) with the ORIGINAL unmodified Twist file -> exit 0, 475 lines, 466 screenshots. Face region changes between frames (`twist-tools/framediff.py`, run with `python -P` because the folder holds an `inspect.py`: 20167 px differ from frame 0, bbox 461,186 to 750,352), so expressions drive the morphs.

## Patch (minimal, upstream-style)

File: `fixtures/private/spikes/bevy_vrm1_default_overrides.patch` (patched crate copy: `fixtures/private/spikes/bevy_vrm1_patched`).

```diff
-    #[serde(rename = "isBinary")]
+    #[serde(rename = "isBinary", default)]
     pub is_binary: bool,
-    #[serde(rename = "overrideBlink")]
+    #[serde(rename = "overrideBlink", default = "default_override")]
     pub override_blink: String,
-    #[serde(rename = "overrideLookAt")]
+    #[serde(rename = "overrideLookAt", default = "default_override")]
     pub override_look_at: String,
-    #[serde(rename = "overrideMouth")]
+    #[serde(rename = "overrideMouth", default = "default_override")]
     pub override_mouth: String,
 }
+
+/// VRM 1.0 spec: overrideBlink / overrideLookAt / overrideMouth default to "none".
+fn default_override() -> String {
+    "none".to_string()
+}
```

Suggested extra upstream hardening: do not remove `VrmHandle` before the extension parse succeeds, and log the parse error even without the `log` feature.

## Upstream

github.com/not-elm/bevy_vrm1 issues checked via WebFetch (searches overrideBlink; expressions/load): no issue for this bug. Related only: PR #48 (override system, merged), #38 (closed, expressions API), #64 (open, VRM 1.0/VRMA spec-compliance gaps; probably the place to report it). Nothing filed by us.

## Can the app carry the patch without forking?

Yes: vendored crate plus `[patch.crates-io]`, no upstream fork needed.
1. Copy the patched crate to a tracked path such as `vendor/bevy_vrm1` (`fixtures/private/` is gitignored).
2. Root `Cargo.toml`: `[patch.crates-io] bevy_vrm1 = { path = "vendor/bevy_vrm1" }`. `crates/avatar/Cargo.toml` stays `bevy_vrm1 = "0.10"`.
3. Remove the patch when upstream ships the fix.
Build proof: `cargo build --release -p app --bin app --config "patch.crates-io.bevy_vrm1.path='fixtures/private/spikes/bevy_vrm1_patched'"` rebuilt only bevy_vrm1, avatar and app (2m42s, CARGO_BUILD_JOBS=4). Root Cargo.toml was not edited; Cargo.lock restored from backup; `target/release/app.exe` restored to the pre-spike binary (patched one kept at `fixtures/private/spikes/app.exe.patched`).
No-vendor alternative: inject the defaults into the VRM JSON chunk before load (what strip.py `addoverride` does); heavier, copies a 10 MB file.

## Side effect

Replay runs write `av_NNNN.png` into `fixtures/private/golden_take` (466 frames per run; overwritten by the runs above). Re-check reference frames there.

---

# Earlier investigation (superseded)

Status then: root cause NOT found. Trigger narrowed to the presence of a non-empty `VRMC_vrm.expressions.preset` map.

## Symptom

`samples/VRM1_Constraint_Twist_Sample/VRM1_Constraint_Twist_Sample.vrm` (valid VRM 1.0) under `APP_REPLAY`: 0 frames, low CPU, no error, never reaches `ExpressionEntityMap`. `samples/Seed-san.vrm` completes (exit 0, screenshots written).

## Evidence

Method: `run_replay.sh`, RUST_LOG `bevy_vrm1=debug,bevy_gltf=debug,bevy_asset=debug,wgpu=warn`, `timeout 60`. Exit 124 = killed at 60 s (stall), exit 0 = completed. Copies built with `strip.py` (JSON chunk rewritten, BIN chunk byte-identical).

| Variant | Exit | Result |
|---|---|---|
| Original | 124 | stall after 3rd tangent log line (14:34:03.63) |
| Seed-san (control) | 0 | completes, 6 tangent lines |
| morph (targets dropped) | 124 | stall |
| constraints (VRMC_node_constraint dropped) | 124 | stall |
| springs (VRMC_springBone dropped) | 124 | stall |
| images (textures dropped) | 124 | stall |
| nobinds (all presets, `morphTargetBinds` = []) | 124 | stall |
| nooverride (overrideBlink/LookAt/Mouth dropped) | 124 | stall |
| keepA (9 presets) | 124 | stall |
| keepB (9 presets) | 124 | stall |
| expressions (whole `expressions` block removed) | 0 | completes, 475 log lines, 466 "Screenshot saved" |
| keep0 (preset map emptied) | 0 | completes, 475 log lines, 466 "Screenshot saved" |
| keepN (only `neutral`) | 124 | stall after 3rd tangent line, same as original |

Static facts (read-only, `python -I`):
- No TANGENT attribute on any of 13 primitives (mesh 0: 5, mesh 1: 7 with 57 morph targets each, mesh 2: 1). Tangent computation is logged but is not the stall: Seed-san logs the same message and completes.
- Twist has leftEye and rightEye bones (nodes 26, 27). Not yet confirmed whether these bones are the reason the humanoid bone gate (`ChildSearcher::has_been_spawned_all_bones`) never passes.

## Interpretation

- Stall happens before the first frame, with no log output for 56+ s and low CPU. This looks like a wait or deadlock in load/init, not a slow render or CPU loop.
- Removing the whole expressions block, or emptying the preset map, makes the file load. Keeping even one preset (`neutral`) re-triggers the stall. Removing binds or overrides does not fix it, so the trigger is not the bind contents.
- Before ruling out the trigger in bevy_vrm1 code, I did not step through `ExpressionEntityMap::new` or `apply_initialize_expressions` for the non-empty path. The `neutral` preset also has not been inspected for its own content (binds, `isBinary`, `overrideBlink`).
- Hypotheses from the brief: H3 (constraints) ruled out. H5 (slow load) ruled out, since the stall is not slow progress. H1 (morph or image content) ruled out, since morph and images variants still stall. H2 (observer rejects binds) weakened, since nobinds still stalls. H4 (bone gate) unconfirmed.

## Workaround (not a fix)

Strip `VRMC_vrm.expressions` from a copy of the file. The avatar then loads and renders, but the model has no facial expressions. Do not ship this as a fix without the user's agreement.

## Unknowns / next steps

1. Confirm whether the non-empty map alone is the trigger with a preset that has no binds at all (for example `neutral` with `morphTargetBinds` removed and no overrides).
2. Test a `custom`-only expressions block.
3. Step through `ExpressionEntityMap::new` (bevy_vrm1 `vrm/expressions.rs`, ~lines 330 to 370) and `apply_initialize_expressions` for the non-empty path, with a debugger or added trace logs in a copy under `fixtures/private/spikes` (not in `crates/`).
4. Check whether the bone gate is also blocked (leftEye and rightEye).

## Side effect to check

The screenshot runs write `av_NNNN.png` into `fixtures/private/golden_take`, the same folder the Seed-san control used. The completed Twist runs may have overwritten existing golden frames there. Check before using that folder as a reference.

## Files

- Tools: `fixtures/private/spikes/twist-tools/` (`inspect.py`, `prims.py`, `strip.py`, `run_replay.sh`, `bisect_expr.sh`, `bisect_expr2.sh`)
- Logs: `fixtures/private/out/twist_r1/twist_r1.log`, `fixtures/private/out/seed_r1/seed_r1.log`, `fixtures/private/out/run_<variant>/<variant>.log`
- Copies: `fixtures/private/out/twist_<variant>.vrm`
