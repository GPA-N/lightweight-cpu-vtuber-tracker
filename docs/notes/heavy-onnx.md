# Heavy ONNX CPU at low duty cycle

Research only. No edits to `crates/`, no `cargo`, no camera, no benchmark run. Other agents are measuring.

Source base paths used below:
- ort: `ort-rc14/` = `C:\Users\Lenovo\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\ort-2.0.0-rc.14\`
- project: relative to `C:\Users\Lenovo\Documents\Cool Virtual Project\`

Tool note: context7 was unavailable (ToolSearch returned only WebFetch and WebSearch). Sources are WebFetch/WebSearch plus local source reads. Background Haiku agents were spawned for Bevy and pipeline threading; their output files were empty when read, so nothing from them is used here.

## Measured problem (from the task, not re-measured here)

- Process averages about 200% of one core during tracking.
- Tracking takes about 17 ms per frame at about 15 fps, so expected duty cycle is about 25% of one core.
- Peak private bytes 904 MB, working set 519 MB with Bevy loaded. 55 to 64 MB with tracking alone (prior run: `openspec/changes/fix-head-fit-and-expressions/design.md` "Live bench" line: peak working set 59.9 MB, ONNX threads 5).

## Code facts

- `crates/infer/src/session.rs:70-85` `build_session`: one global `ort::init().build()` (cached, lines 36-44), CPU EP via `CPU::default().build()`, `with_intra_threads(threads)`, `commit_from_file`. No spinning, no global pool, no memory-pattern or arena setting.
- `session.rs:53-61` `threads_for` / `default_threads`: physical estimate = logical / 2, intra = physical - 1. Caller passes 5 in the live runs (per prior bench notes).
- `crates/infer/src/tracker.rs:127-175` `step`: detector runs only when `self.sched.should_detect()` (tracker.rs:138). Landmarks `self.landmarks.run` (tracker.rs:164), then blendshapes `self.blend.run` (tracker.rs:175). All on the calling thread, one session at a time. The three sessions never run concurrently.
- Session call sites: `crates/infer/src/detector.rs:295`, `landmarks.rs:113`, `blendshapes.rs:119`.
- Model sizes (`models/`): `face_detector.onnx` 418,437 B; `face_landmarks.onnx` 4,920,995 B (256x256 input, about 12 ms per run per prior notes); `face_blendshapes.onnx` 1,094,706 B.

## Question 1: Does intra-op spinning explain the CPU?

**Plausible, not proven.** Spinning makes idle pool threads busy-wait for work instead of blocking. That burns CPU between inferences, which matches "high average, low work".

Evidence for the default:
- ORT v1.31.0 `include/onnxruntime/core/session/onnxruntime_session_options_config_keys.h` (fetched from GitHub raw, no line numbers available): comment says the default is "0" when built with `ORT_CLIENT_PACKAGE_BUILD` and "1" otherwise, and that spinning is disabled by default for client/on-device workloads. Key names `session.intra_op.allow_spinning` and `session.inter_op.allow_spinning`.
- ORT threading docs, https://onnxruntime.ai/docs/performance/tune-performance/threading.html: "Default: 1 (Enabled)" for spinning. "Provides faster inference but consumes more CPU cycles, resources, and power." Spin duration default "not set (uses the legacy fixed iteration count)".
- Third-party report (search result, not official): ORT intra and inter pools spin-wait between inferences by default; disabling fixed idle CPU. https://upd.dev/chopratejas/headroom/commit/5c561bd (commit description, not release docs).

Conflict: the header comment and the docs page disagree on the default for client-style builds. Whether the pyke `ort` prebuilt for rc.14 defines `ORT_CLIENT_PACKAGE_BUILD` is **UNVERIFIED**. Measure instead of guessing.

Exact rc.14 setter API (session level, the one that matters here):
- `SessionBuilder::with_intra_op_spinning(self, enable: bool)`: `ort-rc14/src/session/builder/impl_config_keys.rs:135`, implemented via `with_config_entry` (`src/session/builder/mod.rs:182`).
- `SessionBuilder::with_inter_op_spinning(self, enable: bool)`: `impl_config_keys.rs:128`.
- Global pool spin control: `GlobalThreadPoolOptions::with_spin_control(self, bool) -> Result<Self>`: `ort-rc14/src/environment.rs:260` (calls `SetGlobalSpinControl`). Only affects the global pool, which this code does not use. So it has no effect on the current build.

Spin constants in the ORT thread-pool source (iteration count per spin) were **UNVERIFIED**: the path `onnxruntime/core/common/threadpool.h` at tag v1.31.0 returned 404, and was not retried with the correct path.

## Question 2: Is intra_threads=5 on three sessions wasteful? Recommended count?

- Each `build_session` creates its own intra pool (no global pool; `session.rs:77-82` sets only intra threads). Three sessions with 5 threads each means up to 15 pool threads. Each pool is created per session and only one session runs at a time, so most of those threads sit idle.
- The ORT docs give no thread count for small models. The threading page only says "test a few thread settings" for NUMA. Recommended count for these three models: **UNVERIFIED**. The data points are: landmarks about 12 ms at 5 threads (prior notes); the detector and blendshapes timings were not in the notes read.
- Wasted CPU from idle threads only occurs if spinning is on (Q1). Without spinning, idle threads block, so the cost is memory (thread stacks), not CPU.
- Global pool vs independent pool:
  - Global pool: `EnvironmentBuilder::with_global_thread_pool(GlobalThreadPoolOptions)` (`environment.rs:489`), with `GlobalThreadPoolOptions::with_intra_threads` (`environment.rs:250`) and `with_inter_threads` (`environment.rs:242`).
  - When the environment has a global pool, `pre_commit` sets `DisablePerSessionThreads` for each session (`src/session/builder/impl_commit.rs:167-170`), unless `no_global_thread_pool` is set. So sessions share one pool and no per-session pool is created.
  - `SessionBuilder::with_independent_thread_pool(self)` (`impl_options.rs:273`) keeps a per-session pool even when a global pool exists.
  - Our code has no global pool, so sessions use per-session pools, which is the current state.
  - Docs: global pool "to prevent overheated contentions among session thread pools" (threading page, custom threading callbacks section).

## Question 3: Do arena, memory pattern, or arena extend strategy reduce peak private bytes?

- **Arena is already off.** `ort-rc14/src/ep/cpu.rs:7-9` `CPU { use_arena: bool }`, default false. `cpu.rs:38-42` calls `DisableCpuMemArena` when false. Our `CPU::default()` therefore disables the arena. Turning it on would raise memory retention, not reduce it.
- Memory pattern: `with_memory_pattern(self, enable: bool)` at `impl_options.rs:128`. Default is on in ORT (**UNVERIFIED** for rc.14 default; ORT C API default is enabled). Turning it off may reduce cached allocation plans; latency cost **UNVERIFIED**. Expected to be small for fixed input shapes, since the plan is reused.
- Arena extend strategy: `cpu.rs` exposes only `with_arena_allocator(bool)` (line 24). No extend-strategy setter was found for the CPU EP in rc.14 (search of `cpu.rs:7-42`; wider search **UNVERIFIED**).
- Attribution: 55 to 64 MB with tracking alone is the ORT-side footprint seen in prior runs, versus 904 MB peak with Bevy loaded. ORT is not shown to be the source of the 904 MB. **UNVERIFIED.**

## Question 4: Concrete changes to `build_session` (ordered, at most 5)

1. **Turn off intra-op and inter-op spinning.**
   - Calls: `builder.with_intra_op_spinning(false)` and `builder.with_inter_op_spinning(false)` (`impl_config_keys.rs:135`, `:128`), added after `with_intra_threads` in `session.rs:81`. Each returns `BuilderResult`, so map errors as `load_err`.
   - Expected effect: average CPU drops toward the duty cycle if spinning is the cause. Tracking latency unchanged or slightly higher on wake-up.
   - Risk: low. Only wake-up latency changes. Reversible by removing the two calls.
   - Measure: `BENCH_REPEAT=300 target\release\bench.exe --check-image fixtures\private\test10.png --models models`, once with and once without the calls. Record median and p95 latency and CPU seconds (process CPU time delta over the run, e.g. via PowerShell `Get-Process`). Expected per-frame CPU drop.
2. **Sweep intra-op thread count.**
   - Call: `with_intra_threads(n)` already exists (`session.rs:81`). Run with n = 1, 2, 5 per model. Decide per model.
   - Expected effect: fewer pool threads, possibly no latency change for these small models. Landmarks at 256x256 may slow down with fewer threads.
   - Risk: landmarks latency increase (must stay under the 17 ms frame budget with margin).
   - Measure: same bench command, per thread count, median and p95.
3. **One global thread pool instead of three per-session pools.**
   - Calls: in `environment()` (`session.rs:41`), `ort::init().with_global_thread_pool(GlobalThreadPoolOptions::default().with_intra_threads(n)?.with_spin_control(false)?).build()`. `with_global_thread_pool` is at `environment.rs:489`; `with_spin_control` at `environment.rs:260`; `with_intra_threads` at `environment.rs:250`. Sessions then get `DisablePerSessionThreads` automatically (`impl_commit.rs:167-170`).
   - Expected effect: one pool of n threads shared by all three sessions. Lower thread count and less idle-thread overhead.
   - Risk: the global pool must be configured before the first `environment()` call (the env is a process singleton, `environment.rs` build rejects a second env). Changing it affects every ORT session in the process, including any other crate using ORT.
   - Measure: bench command with the global pool on and off; compare median, p95, and CPU seconds.
4. **Memory pattern off (experiment only).**
   - Call: `builder.with_memory_pattern(false)` (`impl_options.rs:128`).
   - Expected effect: unknown. May reduce retained allocation plans. Could slow inference.
   - Risk: latency cost **UNVERIFIED**. Only test if peak private bytes stays high after items 1 to 3.
   - Measure: bench command, check `BENCH_REPEAT=300` latency and peak working set, plus process private bytes.
5. **Leave the arena alone.** It is already off (`cpu.rs:38-42`). Do not enable `with_arena_allocator(true)` to cut memory: it would raise memory use.

Order rationale: item 1 is the cheapest test of the main hypothesis. Items 2 and 3 only matter if item 1 does not bring CPU near the duty cycle.

## 10-line summary

1. Measured: about 200% of one core at about 25% expected duty cycle; peak private 904 MB with Bevy, 55 to 64 MB tracking-only.
2. Code: three ORT sessions, each with 5 intra threads and its own pool; no spinning, global pool, or memory settings (`session.rs:70-85`).
3. Tracker runs the three sessions one at a time on one thread (`tracker.rs:164`, `:175`, detector `:138` scheduled).
4. Spinning is the leading hypothesis; the default is documented as enabled (threading docs page) but the header comment says it depends on client build; our build flag is UNVERIFIED.
5. Fix to test first: `with_intra_op_spinning(false)` and `with_inter_op_spinning(false)` (`impl_config_keys.rs:135`, `:128`).
6. Thread count 5 per session is not recommended by ORT docs; sweep 1, 2, 5 per model. Recommended count UNVERIFIED.
7. Global pool (`environment.rs:489`) cuts 15 threads to one shared pool; sessions get `DisablePerSessionThreads` (`impl_commit.rs:167-170`).
8. Arena already disabled (`ep/cpu.rs:38-42`); enabling it would raise memory. Memory pattern off is an experiment with UNVERIFIED latency cost.
9. Memory attribution to ORT for the 904 MB peak is UNVERIFIED; Bevy is the likely large share.
10. Measure with `BENCH_REPEAT=300 target\release\bench.exe --check-image fixtures\private\test10.png --models models`, comparing CPU seconds and median/p95 latency per change.
