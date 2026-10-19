# Bevy lean spike 2: avatar window without camera

Spike: `fixtures/private/spikes/bevy-lean`. Driver calls `avatar::window::run` with `live: None`, model `samples/Seed-san.vrm`. Toggles are env vars in the spike copy only. Measure: one-core CPU % (process CPU delta / wall), peak WorkingSet64 (WS) and PrivateMemorySize64, 5 s warmup, 15 s window, A/B alternating.

Keep rule: CPU down about 4 points or WS down about 10 MB.

## Results

| change | runs | CPU % (vs base) | WS MB (vs base) | priv MB (vs base) | verdict |
|---|---|---|---|---|---|
| base | 6 (run 1 pooled, run 2) | 41.7 to 44.1 | 399 to 406 | 793 to 801 | reference |
| msaa_off | 3 | 41.5 (-0.2) | 412.8 (+6.6) | 807.8 (+7.2) | reject, no effect |
| low_power | 3 | 42.8 (+1.1) | 394.9 (-11.2) | 789.1 (-11.6) | reject, CPU up, WS in noise |
| mem_usage | 3 + 3 | 41.8 (+0.0), 43.0 (-1.1) | 348.7 (-57.4), 343.0 (-56.1) | 489.9 (-310.8), 483.5 (-309.7) | keep |
| no_source | 3 + 3 | 36.2 (-5.5), 34.0 (-10.2) | 290.6 (-115.6), 290.5 (-108.6) | 423.1 (-377.6), 423.2 (-370.0) | keep, visual check pending |
| vulkan | 1 one-off | n/a | n/a | n/a | not a real change, see below |
| dx12 | 3 (all invalid) | n/a | n/a | n/a | blocked, see below |
| combined (all four toggles) | 3 vs base 3 | 38.4 (-3.9) | 217.5 (-205.8) | 262.3 (-556.4) | WS passes, CPU just misses |

Combined = msaa_off + low_power + mem_usage + no_source. It includes two rejected toggles, so it is not the recommended set. The recommended pair (mem_usage + no_source) was not measured as a combo.

## Notes

- Base WS swings about 390 to 424 MB between runs. Deltas under about 15 MB are noise.
- `vulkan`: base one-off and `vulkan` one-off both log `backend: Vulkan`. The env var did not change the backend on this box. The -265 MB private drop in run 1 is unexplained and not reproduced as a backend change. Reject until re-verified.
- `dx12`: backend is Dx12 (AMD Radeon iGPU, driver 31.0.21923). Every run fails in wgpu shader translation: FXC `X4008` warning, then `internal error: argument pulled into unrelated predicate`, then a caught rendering validation error. Driver logs `ticks=0` and exits within about 12 s. This is a shader compile problem on this driver, not a spike bug. Not measured.
- `no_source` changes glTF source retention in the vendored loader. Visual correctness was not checked (no webcam, no live.exe). The base run also logs an mtoon parse error (`missing field texCoord`), so this is pre-existing and not caused by no_source.
- Not measured: explicit Bevy feature list without bevy_gilrs/x11/wayland/webgl2 (gilrs confirmed pulled in by the default feature), no shadows, texture downscale.

## Diffs to port (not applied to the main app)

`crates/app/src/bin/live.rs`, memory hint (mem_usage):

```rust
use bevy::render::settings::{MemoryHints, RenderCreation, WgpuSettings};

let mut wgpu = WgpuSettings::default();
wgpu.memory_hints = MemoryHints::MemoryUsage;

App::new()
    .add_plugins(DefaultPlugins.set(bevy::render::RenderPlugin {
        render_creation: RenderCreation::Automatic(Box::new(wgpu)),
        ..default()
    }))
```

`vendor/bevy_vrm1/src/vrm/loader.rs` around line 109 (no_source):

```diff
-            include_source: true,
+            include_source: false,
```

Root `Cargo.toml`: no change measured yet, so no diff.

## Next

1. Measure mem_usage + no_source together, base alternating, 3 rounds.
2. Check no_source visually (screenshot via existing AVATAR_SHOT harness) before porting.
3. Re-verify vulkan with an explicit backend log line.
4. Find the dx12 shader failure (FXC on this driver) before retrying dx12.

Maps to openspec `reduce-live-cpu-and-memory`. Openspec files not edited.
