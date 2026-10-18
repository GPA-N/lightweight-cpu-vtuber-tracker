# 08 Cap the render loop at about 30 fps

Read `00-rules.md` first (Debate protocol, no-face rule, alternate before/after binaries). Run this subtask AFTER subtask 07 so its numbers show how much of the window cost is the render loop.

Fact: the window renders at an uncapped 60 fps (vsync) while the avatar changes only about 15 times a second. A previous attempt, `WinitSettings::reactive_low_power(25 ms)`, gave CPU between 97 and 111 percent against 113 percent, render 40 fps and +2 to +4 ms hand-off: inside the noise, so it was reverted. Verify the API with `npx -y ctx7@latest library bevy "WinitSettings UpdateMode reactive"` before coding.

Variants (one build each, copy each binary to `target\release\live_<variant>.exe`; file `crates/avatar/src/window.rs`, `build_app`, insert the resource before `app` is returned):
- V0: baseline binary (no change).
- V1: `WinitSettings { focused_mode: UpdateMode::reactive(Duration::from_millis(33)), unfocused_mode: UpdateMode::reactive(Duration::from_millis(33)) }`.
- V2: `UpdateMode::reactive_low_power(Duration::from_millis(33))` for both.
- V3: keep `WinitSettings::game()` but set `PresentMode::Fifo` explicitly on the primary window and `desired_maximum_frame_latency: Some(1)` in the `Window` settings (check the 0.19 field name with ctx7), to see whether lowering queued frames reduces CPU.

Measurement: no-face live runs, 15 s, alternating V0, V1, V2, V3, V0, V1, V2, V3 (8 runs; the cap of 4 is raised to 8 here). Record CPU average, peak working set and private, render fps, and from the report the line `tracker ready -> applied in renderer` (median and p95).

KEEP rule for a variant: KEEP only if BOTH its runs are more than 15 percent lower on CPU average than BOTH V0 runs, render fps stays at least 30 (read the `render FPS` line; a value below 30 fails), and `tracker ready -> applied` median rises by no more than 4 ms. Keep at most one variant (the best); restore the others. If none passes, REVERT everything and say so.
Hypothesis and Prediction first (a number per variant). Then Result, Verdict. A challenger reviews next.
