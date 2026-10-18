# 09 Limit Bevy's thread pools

Read `00-rules.md` first (Debate protocol, no-face rule, alternate before/after binaries). Run AFTER subtask 07.

Fact: Bevy 0.19 spawns task-pool threads for every logical core (12 here) plus the render thread and winit, and ONNX adds 5. Extra threads can burn CPU on spin and context switches. Verify the API: `npx -y ctx7@latest library bevy "TaskPoolPlugin TaskPoolOptions with_num_threads"`; also read `~/.cargo/registry/src/*/bevy_app-0.19*/src/task_pool_plugin.rs`.

Change: in `crates/avatar/src/window.rs` `build_app`, set the task pool options on DefaultPlugins:
`.set(TaskPoolPlugin { task_pool_options: TaskPoolOptions::with_num_threads(N) })`
(use the exact 0.19 names and imports you find). Variants: N = 4 and N = 2. Build each, copy to `target\release\live_n4.exe` and `live_n2.exe`; V0 is the baseline binary.

Measure: no-face live runs, 15 s, alternating V0, N4, N2, V0, N4, N2 (6 runs; the cap of 4 is raised to 6 here). Record CPU average, peak working set and private, render fps, `tracker ready -> applied` median and p95, and the process thread count if the report shows it (otherwise count with `(Get-Process live).Threads.Count` once during a run).

KEEP rule: KEEP the best variant only if BOTH of its runs are more than 15 percent lower on CPU average than BOTH V0 runs, render fps stays at least 55 (no frame cap in this subtask), and the avatar still loads (a screenshot with `AVATAR_SHOT` as in subtask 03, look at it with the Read tool). Otherwise REVERT.
Hypothesis and Prediction first (numbers). Then Result, Verdict. A challenger reviews if something is KEPT.
