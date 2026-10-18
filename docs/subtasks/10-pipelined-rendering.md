# 10 Try turning off pipelined rendering

Read `00-rules.md` first (Debate protocol, no-face rule, alternate before/after binaries). Run AFTER subtask 07.

Fact: Bevy's PipelinedRenderingPlugin runs the render world on its own thread so the app and render overlap. That raises throughput but also CPU use, and our avatar scene is tiny. Verify the API: `npx -y ctx7@latest library bevy "PipelinedRenderingPlugin disable"` and read `~/.cargo/registry/src/*/bevy_render-0.19*/src/pipelined_rendering.rs`.

Change: in `crates/avatar/src/window.rs` `build_app`, after the two `.set(...)` calls on DefaultPlugins add `.disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()` (use the exact path and name you find in 0.19; if the plugin is not in the DefaultPlugins group for our features, report that and stop with REVERT/skip).

Measure: no-face live runs, 15 s, alternating baseline binary (V0) and the new binary (V1): V0, V1, V0, V1 (4 runs). Record CPU average, peak working set and private, render fps, `tracker ready -> applied` median and p95. Also confirm in a screenshot (`AVATAR_SHOT`, subtask 03 method) that the scene still renders.

KEEP rule: KEEP only if BOTH V1 runs are more than 15 percent lower on CPU average than BOTH V0 runs, render fps stays at least 55, and `tracker ready -> applied` median rises by no more than 4 ms. Otherwise REVERT.
Hypothesis and Prediction first (a number). Then Result, Verdict. A challenger reviews if KEPT.
