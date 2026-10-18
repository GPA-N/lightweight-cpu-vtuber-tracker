# Local patches to bevy_vrm1

Upstream: https://github.com/not-elm/bevy_vrm1, crate `bevy_vrm1` version 0.10.0 (crates.io). Licences kept as shipped: `LICENSE-MIT`, `LICENSE-APACHE2`. This directory holds the crate source, `Cargo.toml`, `README.md` and `examples/` (cargo needs the example files that `Cargo.toml` lists). It is wired in through `[patch.crates-io]` in the root `Cargo.toml`; `crates/avatar/Cargo.toml` still depends on `bevy_vrm1 = "0.10"`.

## Patch 1: optional expression keys get VRM 1.0 defaults

File: `src/vrm/gltf/extensions/vrmc_vrm.rs`, struct `VrmPreset`.

In the VRM 1.0 spec `isBinary`, `overrideBlink`, `overrideLookAt` and `overrideMouth` are optional (defaults `false`, `"none"`, `"none"`, `"none"`). Upstream 0.10.0 requires them with no serde default. A model that omits them (VRM1_Constraint_Twist_Sample) fails with `missing field overrideBlink`, and `spawn_vrm` drops the model without any message, so it never gets `ExpressionEntityMap`.

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

Test: `crates/avatar/tests/vrm_expression_defaults.rs` (the crate's own lib tests do not build from the published package because the `bevy_test_helper` dev-dependency is missing, so the test lives in `avatar`). It failed with `missing field isBinary` before the patch and passes after.

## Parse failures are logged

No source change. `crates/avatar/Cargo.toml` enables the crate feature `log` (`log = ["bevy/bevy_log"]`), which makes the `vrm_error!` macro in `src/error.rs` print "Failed to load VRM extensions: ..." in `spawn_vrm`. Without it the error was swallowed. The app still waits for `ExpressionEntityMap` without a timeout; a failed model is now reported but the replay does not exit by itself.

## Patch 2: constraints that share a source are merged

File: `src/vrm/node_constraint/initialize.rs`. Each constraint was registered by looking up the source's `...ConstraintDestinations` component and either pushing to it or inserting a new one through `Commands`. Commands are applied later, so when two constraints had the same source in one pass (the VRM1 Constraint Twist sample has `J_Aim_L_Shoulder` and `J_Aim_L_TopsUpperArm`, both aimed at the same bone), the second insert replaced the first and one of the two helper bones never followed the arm: the sleeve stayed in the T pose while the arm moved. The patch gathers the destinations per source first and inserts each component once. Seen in `fixtures/private/out/photo_to_avatar/` renders of the Twist sample (the left sleeve floating beside the raised arm).

## Upstream

Not reported yet. Suggested place: issue #64 (VRM 1.0 / VRMA spec-compliance gaps). Suggested extra hardening there: do not remove `VrmHandle` before the extension parse succeeds. Remove this vendor directory and the `[patch.crates-io]` entry once upstream releases the fix.
