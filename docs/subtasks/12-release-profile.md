# 12 Release profile tuning (LTO, one codegen unit)

Read `00-rules.md` first (Debate protocol, no-face rule, alternate before/after binaries). Run AFTER the render subtasks 08 to 11 so it adds on top of any kept change.

Fact: the repo root `Cargo.toml` has no `[profile.release]`, so Cargo defaults apply (lto off, codegen-units 16, opt-level 3). Whole-program optimisation can cut CPU in the Rust parts (our tracker, Bevy systems) and shrink the exe. Cost: much longer release builds and more RAM while compiling (one build, `CARGO_BUILD_JOBS=4`, may take 10 or more minutes and will load the laptop; tell nobody to run anything else; never run two cargo processes).

Change: add to the root `Cargo.toml`:
```toml
[profile.release]
lto = "thin"
codegen-units = 1
```
Do NOT set `panic = "abort"` (the avatar window relies on catching a graphics panic with `catch_unwind` in `crates/avatar/src/window.rs`).
Before editing, copy the current `target\release\live.exe` and `bench.exe` to `live_before.exe` and `bench_before.exe` (they must be the latest build of the current source; if unsure, rebuild first with the current profile and then copy). Build the new profile with `cargo build --release -p bench -p app --bin live`, record the build time and the exe sizes (before and after).

Measure: (a) tracker only, saturated: `$env:BENCH_REPEAT = 300; .\target\release\bench.exe --check-image fixtures\private\test10.png --models models` before (bench_before.exe) and after, alternating, 3 runs each: latency median and CPU. (b) live no-face runs 15 s alternating before, after, before, after (4 runs): CPU average, peak working set and private, render fps. Accuracy guard (rule 5) must be unchanged.

Debate: Hypothesis and Prediction first (a number); Result and Verdict after.
KEEP rule: KEEP if the accuracy guard is unchanged, tests pass, and either the saturated bench median latency drops by more than 5 percent in all 3 pairs OR live CPU average drops by more than 10 percent in both pairs; the longer build time is accepted. Otherwise REVERT (delete the `[profile.release]` section) and rebuild once with the original profile.
