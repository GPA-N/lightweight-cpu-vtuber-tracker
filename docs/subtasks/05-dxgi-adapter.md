# 05 Fix the DXGI video memory reading (0.0 MB)

Read `00-rules.md` first. File: `crates/avatar/src/render_report.rs`, function `query_dxgi_memory` (about line 93). It is called once in `crates/avatar/src/window.rs` after `app.run()` returns (search `query_dxgi_memory`).

Problem: the live report prints `DXGI local video memory: 0.0 MB`. The function reads adapter 0 only, and it runs after the Bevy app (and its GPU resources) has been torn down, so CurrentUsage is probably 0 either way. Bevy renders on `AMD Radeon (TM) Graphics`, vendor 4098 (0x1002), device 5607.

Do this in order, one step per build:
1. Diagnose first: temporarily add a debug `eprintln!` in `query_dxgi_memory` that, for every adapter `EnumAdapters1(i)` until it fails, prints `i`, the description (`adapter.GetDesc1()`: `VendorId`, `DeviceId`, `Description` as UTF-16), and the Local and NonLocal `CurrentUsage` and `Budget` in MB. Build and run `live.exe --seconds 6`. Report the printed table.
2. Fix part A (adapter choice): replace `EnumAdapters1(0)` with a loop that picks the adapter whose `VendorId == 0x1002 && DeviceId == 5607`; if none matches, pick the adapter with the largest dedicated plus shared memory; if the loop finds nothing, return None. Needs the windows feature `Win32_Graphics_Dxgi` (already enabled) and `GetDesc1` (type `DXGI_ADAPTER_DESC1`); check names with `npx -y ctx7@latest library windows-rs "IDXGIAdapter1 GetDesc1"`.
3. Fix part B (timing): query while the app is still running. Add a small Bevy system in `window.rs` that, when the `live_clock` system is about to exit (or on the `AppExit` message), calls `query_dxgi_memory()` and stores the result in a `Mutex<Option<DxgiMemory>>` inside `LiveConfig`/`RenderSamples` (add a field `dxgi: Option<DxgiMemory>` to `RenderSamples` in `crates/avatar/src/live.rs`), and make `crates/app/src/bin/live.rs` print that value instead of `summary.memory` when it is Some. Keep `RunSummary.memory` as it is for the other binaries.
4. Remove the debug `eprintln!`.

Tests: the existing render_report tests must pass; add a test that `query_dxgi_memory()` returns either None or values that are finite and not negative (it may already exist; do not duplicate).

KEEP rule: KEEP if `live.exe --seconds 15` prints a DXGI local or non-local value above 0.0 MB (not n/a). If after step 3 it is still 0.0, report the table from step 1 and REVERT steps 2 and 3, and write the table in `results.md` (this is information the project manager needs).
