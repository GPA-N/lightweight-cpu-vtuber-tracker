# Lean threads: where the bare Bevy window CPU goes (task 2.2)

Status: predictions written before any measurement. Results section added after runs.

## Setup

- Bare window: `fixtures/private/spikes/bevy-bare` (throwaway spike, `crates/` untouched).
  Bevy 0.19.1 (same lock version as the workspace), `DefaultPlugins` with `features = ["3d"]`,
  one `Camera3d` at (0, 1.35, 1.8) looking at head height, one `DirectionalLight`, `ClearColor`.
  No VRM, no mesh, no preview, no face, no camera capture thread.
- Build: `CARGO_BUILD_JOBS=4`, `CARGO_TARGET_DIR` = root `target/`, `cargo build --release` (one process).
- Default present mode (Bevy `AutoVsync`, Fifo on this PC), no WinitSettings unless a toggle is set.
- Runs: 15 s each, one at a time, alternating order (base, a, b, c, d, then reverse).
- Toggles (flags on the spike binary):
  - a `--no-pipelined`: `DefaultPlugins.disable::<PipelinedRenderingPlugin>()`
  - b `--tasks`: TaskPoolPlugin 4 total (compute 2 / io 1 / async 1)
  - c `--cap33`: `WinitSettings` reactive 33 ms, focused and unfocused
  - d `a + b + c`
- Per-thread CPU: `Process.Threads` `TotalProcessorTime` sampled at 4 s and 14 s into each run.
  Thread % = delta / 10 s of one core. Also WS and private bytes at each sample.
- Reference (from notes, not re-measured here): subtask 07 variant D mean 97.7% one core,
  WS 313.7 MB, private 446.3 MB. Task 2.1 wgpu floor, Fifo: 9.9%.

## Predictions (before measurement)

1. Base bare spike lands 90-100% one core, close to D (97.7%). Confidence: high.
2. Per-thread: the top one or two threads own most of the CPU, ~60-80 points combined.
   The top thread is the main (winit/app) thread or the render submit thread, not a task-pool worker.
   Task-pool workers (compute/io/async) stay below 5 points each when idle.
3. Thread count 20-35.
4. a (no pipelined): within 10 points of base. Pipelining only moves the render work to another
   thread; it does not remove work. Confidence: medium.
5. b (task pools reduced): within 5 points of base. Idle pool threads do not spin much. Confidence: medium.
6. c (33 ms reactive cap): the big drop. Predict 15-35% one core, i.e. >=30% reduction. Confidence: medium-high.
   Reason: the wgpu floor test showed frame-rate-limited Fifo costs far less than uncapped busy frames.
7. d (all three): about the same as c (within 5 points of c). Predict 15-30%, so d does NOT reach 10-20%
   with confidence. Low confidence on the exact floor.
8. Memory: WS 280-330 MB, private 380-460 MB for the bare spike. Toggles change memory little (under 10%).

## Results

Raw: `fixtures/private/out/threads/summary.txt` and `run*_threads.txt` (top 8 threads per run).
Window = CPU delta between 4 s and 14 s of a 15 s run, one core = 100%. Life = whole run.

| Mode | Runs | Window % (1 core) | Life % | Threads | WS MB | Private MB |
|---|---|---|---|---|---|---|
| base | 2 | 45.7, 43.6 (mean 44.7) | 49.3, 47.5 | 32-34 | 279.7, 266.1 (273) | 418.1, 411.9 (415) |
| a no pipelined | 2 | 44.0, 48.7 (mean 46.4) | 45.6, 51.6 | 31-33 | 253.9, 253.5 (254) | 399.5, 399.1 (399) |
| b tasks 2/1/1 | 2 | 36.5, 38.2 (mean 37.4) | 38.3, 41.0 | 24-26 | 254.6, 251.5 (253) | 399.7, 396.6 (398) |
| c cap 33 ms | 2 | 24.9, 25.4 (mean 25.2) | 29.3, 29.0 | 32-34 | 266.4, 265.3 (266) | 411.8, 410.6 (411) |
| d a+b+c | 2 | 20.8, 19.1 (mean 20.0) | 24.7, 23.5 | 23-25 | 250.5, 250.3 (250) | 395.7, 395.8 (396) |

Order was base, a, b, c, d, d, c, b, a, base (alternating, reverse second pass). Run-to-run spread is
within about 5 points for CPU, so the means are fair. Effect vs base mean 44.7: a +4%, b -16%, c -44%, d -55%.

### Per-thread (base, run 1, window %)

Top 8: 10.0, 8.7, 5.4, 4.8, 4.6, 4.4, 4.4, 2.0. Sum of top 8 is about 44 of 45.7.
No single hot thread. The load is spread over about 8 threads at 4-10% each. The thread count
(32) suggests Bevy task-pool workers (about 11 on 12 logical cores) plus render and driver threads.
Thread names and start addresses are not resolvable from PowerShell `Process.Threads` without elevation
(start address reads 0x0), so the thread-to-role mapping is NOT measured. Only the count change is measured:
b cuts threads from 32 to 24 and cuts CPU.

### Verdict

- The bare camera-plus-light Bevy window costs about 45% of one core here, not the 98% in the
  subtask 07 reference. The load is spread across threads, not one hot thread.
- Biggest cheap toggle: 33 ms reactive cap (c), -44%. That clears the 30% bar. Cap plus the other two (d) gives -55% (mean 20.0).
- Task pools (b) -16%. Disabling pipelining (a) does nothing (+4%, within noise).
- Toggles reach about 20%: d lands at 19-21% on the window measure and does not go under 10%.
  "Reach 10-20%" holds only at the edge (d = 20.0).
- Memory: toggles move WS by about -5 to -9% (273 to 250-266 MB) and private by about -4% (415 to 396-411).
  No toggle reduces memory much.

### Predictions vs results

1. Base 90-100%: MISSED (44.7%).
2. Top thread owns 60-80 points for top 1-2: MISSED. Spread over about 8 threads.
3. Thread count 20-35: held (23-34).
4. a within 10 points of base: held (+1.7).
5. b within 5 points of base: MISSED (-7.3, -16%). Reduced pools cost more than predicted.
6. c 15-35% and at least 30% cut: held (25.2%, -44%).
7. d 15-30% and close to c: held (20.0%), within 5 points of c.
8. Memory WS 280-330, private 380-460: private held (415); WS low by about 7 (273).

### Disagreement with other notes

- subtask 07 and results.md give bare window 97.7% (variant D, mean of two runs). This spike gives 44.7%
  for camera plus light at the same vsync. The spike has no VRM, no preview, no face, no camera capture,
  and no bevy_vrm1 feature unification. Those are likely causes, but NOT measured. Until a bare build of the
  avatar binary reproduces about 98%, treat the 98% number as the avatar binary's figure, not the bare Bevy floor.
- The current `crates/avatar/src/window.rs` build_app applies a 33 ms reactive cap to all modes. If subtask 07 ran on a
  build with this cap, 98% conflicts with it. The cap's commit date was not resolved. Check before reusing 98%.
- debate/08 says a 30 fps cap saves 20-30 points. Here it saves about 20 points on the 45% base (44.7 to 25.2). That agrees
  in absolute points, but not as a fraction of the 98% baseline cited there.
- lean-floor.md (raw wgpu, Fifo) = 9.9%. Bevy base is 44.7%, so Bevy's stack adds about 35 points over the raw wgpu floor at the same vsync. Consistent with lean-cause H1.
- eff-bevy.md says reactive mode was tested and reverted. This spike shows the cap is the largest cheap lever (-44%), which argues against leaving it reverted for CPU reasons.
