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

## Upstream

Not reported yet. Suggested place: issue #64 (VRM 1.0 / VRMA spec-compliance gaps). Suggested extra hardening there: do not remove `VrmHandle` before the extension parse succeeds. Remove this vendor directory and the `[patch.crates-io]` entry once upstream releases the fix.
