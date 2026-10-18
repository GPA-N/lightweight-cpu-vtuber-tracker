# 13 wgpu memory hints and render settings for lower memory

Read `00-rules.md` first (Debate protocol, no-face rule, alternate before/after binaries). Run AFTER subtasks 08 to 12.

Fact: Bevy 0.19 builds the GPU device from `RenderPlugin { render_creation: RenderCreation::Automatic(WgpuSettings { .. }) }`. `WgpuSettings` has a `memory_hints` field (wgpu `MemoryHints`: `Performance`, `MemoryUsage`, or manual thresholds). The research (docs/notes/eff-bevy.md) says `MemoryHints::MemoryUsage` may lower GPU allocator reservation, which is the most likely source of the large private/committed memory (RenderPlugin alone is reported at about 466 MiB in a minimal Bevy app, bevy #21465, where a maintainer suspects gpu-allocator). Unproven; measure.

Change (file `crates/avatar/src/window.rs`, `build_app`, on `DefaultPlugins`): add `.set(RenderPlugin { render_creation: RenderCreation::Automatic(WgpuSettings { memory_hints: MemoryHints::MemoryUsage, ..default() }), ..default() })`. Check exact 0.19 and wgpu 29 names and imports with `npx -y ctx7@latest library bevy "WgpuSettings memory_hints RenderPlugin"` and the source `~/.cargo/registry/src/*/bevy_render-0.19*/src/settings.rs`. If the best backend from subtask 11 was DX12 and KEPT, set `backends: Some(Backends::DX12)` in the same struct for BOTH variants of this test (otherwise keep the default).

Measure: no-face live runs, 15 s, alternating baseline binary (V0) and the new binary (V1): V0, V1, V0, V1 (4 runs). Record peak working set and especially peak private bytes, CPU average, render fps. Screenshot check (AVATAR_SHOT, Read the png).

Debate: Hypothesis and Prediction first (MB number); Result and Verdict after.
KEEP rule: KEEP if both V1 runs have peak private bytes lower than both V0 runs by more than 60 MB (the measured noise is about 60 MB) or CPU average lower by more than 15 percent, render fps stays at least 55, and the screenshot is correct. Otherwise REVERT.
