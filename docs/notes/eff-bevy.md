# Bevy 0.19 live CPU and memory: research notes

Scope: research only. No code, cargo, benchmark or camera runs. Measured baseline comes from openspec/changes/reduce-live-cpu-and-memory/design.md: full live window about 113% CPU average (per-second max 142%), peak working set 466 MB, peak private 858 MB, render 60 fps. Without the window (camera plus tracker) about 13% CPU and 55 MB. Noise between identical runs is up to 10 CPU points and 67 MB.

Sources used: local registry source (bevy_winit-0.19.1, bevy_render-0.19.1, bevy_internal-0.19.1, wgpu-types-29.0.4, bevy_vrm1-0.10.0); ctx7 CLI (`/websites/rs_bevy`, `/websites/rs_wgpu_wgpu`, `/websites/rs_bevy_framepace_0_21_0_bevy_framepace`); docs.rs pages cited inline; GitHub issues via the GitHub search API and WebFetch. Reddit could not be read: plain fetch returned a bot-check page and no scrapling tool is available in this session, so Reddit is UNPROVEN as a source (not searched). Bevy Discord was not reachable and is UNPROVEN as a source.

## Q1. Known reports and fixes for high idle or low-load CPU in Bevy 0.13 to 0.19

- No issue found that matches this symptom exactly (plain Bevy window on Windows, high CPU with vsync, 0.13 to 0.19). The closest reports are on macOS.
- Bevy issue #10261, "Very high CPU usage on MacOS" (Bevy 0.11.3, Apple M1). A blank DefaultPlugins app used about 50% CPU and about 90% with a couple of meshes. Hiding the window roughly doubled CPU. Windows and Linux were under 1%. No root cause confirmed; no merged fix; no workaround. https://github.com/bevyengine/bevy/issues/10261 . Relevance to this Windows AMD case: LOW to MEDIUM, the mechanism (render Prepare cost, command buffers per frame) is not shown to apply on Windows.
- Bevy issue #11285, "Frame rate permanently drops overtime" (Bevy 0.12.1, Windows 10). Frame rate fell over time and multiple windows made it worse; a memory rise was seen; still open, labeled S-Needs-Investigation; no workaround. https://github.com/bevyengine/bevy/issues/11285 . The reporter tested with PipelinedRenderingPlugin commented out, but the thread never confirms it as a cause or a fix. Relevance: MEDIUM (Windows, render window, memory rise), but the 0.12 issue may not match 0.19.
- Bevy issue #4718, "High CPU overhead when using ParallelExecutor in a minimal throttled example." Relevance: the task-pool executor can burn CPU when throttled. Fetched only as a title in the search results; contents UNPROVEN for 0.19. https://github.com/bevyengine/bevy/issues/4718
- Bevy issue #4161, "Parallel runner deadlocks with idling threads in ComputeTaskPool" (title only, not read). Relevance: UNPROVEN.
- Bevy #12612 (closed, web-only) and PR #12669 (winit loop rewrite) were checked in an earlier session. They are not the Windows symptom.
- The bevy.org low_power example and the WinitSettings docs show the intended way to cut idle CPU is WinitSettings reactive mode. The docs say reactive_low_power "ignores events that don't come from interacting with a window ... This can greatly reduce power consumption." https://docs.rs/bevy/latest/bevy/winit/enum.UpdateMode.html . This app is not idle (it renders every frame), so reactive mode only helps when the scene is still.

Conclusion for Q1: no Bevy issue reports the exact symptom on Windows with an iGPU. The one Windows report (#11285) is an open 0.12 leak-and-slowdown issue. The reactive-mode lever is already tested and reverted in this project (design.md row 3).

## Q2. Vulkan vs DX12 on an AMD iGPU on Windows; the Bevy 0.19 backend and power API

API (verified in local source and ctx7 docs):
- `bevy::render::RenderPlugin { render_creation, synchronous_pipeline_compilation, debug_flags }` (bevy_render-0.19.1/src/lib.rs lines 131 to 137).
- `RenderCreation::Automatic(Box<WgpuSettings>)` (bevy_render-0.19.1/src/settings.rs).
- `WgpuSettings` fields include `backends: Option<Backends>`, `power_preference: PowerPreference`, `adapter_name: Option<String>`, `force_fallback_adapter`, `memory_hints`, `instance_flags`, `priority`, `limits`, `features` (bevy_render-0.19.1/src/settings.rs lines about 41 to 61). Doc: DX12, METAL and VULKAN are enabled by default on non-web and auto-selected.
- `wgpu::PowerPreference { None = 0, LowPower = 1, HighPerformance = 2 }` (wgpu-types-29.0.4/src/adapter.rs lines 66 to 74; docs.rs https://docs.rs/wgpu/latest/wgpu/enum.PowerPreference.html ).
- `wgpu::MemoryHints { Performance (default), MemoryUsage, Manual { suballocated_device_memory_block_size } }`; "some backends may choose to ignore them" (https://docs.rs/wgpu/latest/wgpu/enum.MemoryHints.html ; wgpu-types-29.0.4/src/device.rs line 57).
- Environment overrides exist in wgpu-types 29.0.4: `WGPU_BACKEND` (backend.rs line 157) and `WGPU_POWER_PREF` (adapter.rs line 79). Whether Bevy's default WgpuSettings reads them is UNPROVEN (not checked in bevy_render source).

Vulkan vs DX12 on AMD iGPU: no wgpu-specific evidence found. The WebSearch hits were gaming forums; one PPSSPP forum post (an AMD A8 APU, Windows 10) reported Vulkan slower than D3D11 on an AMD APU, but that is a different engine and API. Source: https://forums.ppsspp.org/showthread.php?tid=23826 . Verdict: UNPROVEN which backend is faster or cheaper in CPU for this app. Only a measurement can decide. Two wgpu issues mention DX12 and Vulkan on Windows and AMD: https://github.com/gfx-rs/wgpu/issues/7922 (Vulkan flicker on AMD GPUs; a correctness issue, not CPU) and https://github.com/gfx-rs/wgpu/issues/1259 (old DX12 discrete GPU rendering problem). Neither reports CPU cost.

Power preference: on a laptop with one iGPU, `LowPower` and `HighPerformance` should select the same adapter, so no CPU change is expected. UNPROVEN.

## Q3. What causes 400+ MB in a minimal 3D window, and what cuts it

Measured: tracker-only process 53 to 55 MB; full window process 457 to 532 MB working set, 849 to 916 MB private. Design.md notes that the DXGI reading returned 0.0 MB and the cause is not yet attributed. Subtask 07 showed the avatar is the memory cost and the bare window is the CPU cost.

Known mechanisms (from source and docs):
- Assets keep a CPU copy by default. `RenderAssetUsages::default()` is `MAIN_WORLD | RENDER_WORLD`. `RENDER_WORLD` alone drops the CPU data after extraction. Docs: https://docs.rs/bevy/latest/bevy/asset/struct.RenderAssetUsages.html ; PR #11399 introduced the flags. Trade-off: the CPU data cannot be read from `Assets<T>` after that.
- Bevy issue #22969 (fixed in PR #22992, closed 2026-02-17): `RenderAssetUsages` from `.meta` was ignored for KTX2, Basis and DDS, so their CPU data stayed in memory. This shows that loader-specific paths can keep CPU copies. Source: https://github.com/bevyengine/bevy/issues/22969 . Relevance: the VRM avatar loads glTF textures, not KTX2, so this is UNPROVEN as a cause here.
- `bevy_vrm1` 0.10.0 keeps `Handle<Image>` fields for each MToon texture (bevy_vrm1-0.10.0/src/vrm/mtoon/material.rs lines 45 to 75) and does not set RenderAssetUsages itself (grep found no RenderAssetUsages in its src). So textures use the default `MAIN_WORLD | RENDER_WORLD`.
- The preview image in crates/avatar/src/window.rs line 445 uses `RenderAssetUsages::default()`. It is written from the CPU each frame, so `MAIN_WORLD` must stay. Dropping it is not an option. Its size is 640 x 480 x 4 = 1.2 MB, so it is small.
- wgpu allocator: `MemoryHints::MemoryUsage` may shrink GPU sub-allocation blocks. On an iGPU, GPU memory is shared with system RAM, so the working set may count it. UNPROVEN for this app; no source found that ties working set to wgpu allocation on Windows.
- Pipelined rendering keeps a second frame of extracted data alive. PipelinedRenderingPlugin is in DefaultPlugins under cfg(multi_threaded) (bevy_internal-0.19.1/src/default_plugins.rs lines 55 to 57). It may add one copy of the extracted render data. UNPROVEN size.

Verdict: the 400 MB is most likely the avatar's GPU-uploaded data plus its CPU copies, the wgpu/D3D12 or Vulkan runtime, and the Bevy render world. The exact split is UNPROVEN until measured. The one clear cut is dropping CPU copies of the avatar's textures and meshes with `RENDER_WORLD`-only, but that needs a change to how bevy_vrm1 loads assets (a fork or a patch). Size of the gain is UNPROVEN.

## Q4. Existing crates or examples for "low CPU, update only on data change"

- Bevy's own low_power example: WinitSettings `focused_mode` and `unfocused_mode`, with `UpdateMode::reactive_low_power(Duration)`. https://docs.rs/bevy/latest/bevy/winit/enum.UpdateMode.html . Already tested: reverted in this project (design.md row 3).
- `bevy_framepace` (frame limiter, "sleeping the app immediately before the event loop starts"; context7 `/websites/rs_bevy_framepace_0_21_0_bevy_framepace`). Version 0.21.0 depends on bevy 0.18 (checked in the crate's Cargo.toml from static.crates.io), so it does not match Bevy 0.19. A Bevy 0.19 release was not found in the crates.io lookup (the max_version query returned no output). UNPROVEN whether a 0.19-compatible version exists.
- Non-crate lever: `bevy_render` ignores events that do not change the scene only in reactive modes. Continuous mode always renders. Nothing else in the Bevy 0.19 source changes that.

## Candidates (ranked, at most 8)

Each test is `live.exe --seconds 15` run twice, reporting the CPU average line and the peak working set line. Keep a change only if both runs beat the baseline by more than 10% CPU or 60 MB working set, as in design.md.

1. **Frame cap at 30 fps by the app itself** (not reactive mode). Expected: fewer render frames per second, so less CPU per second. Citation for the lever: the WinitSettings docs above. Expected size UNPROVEN. Risk: fps drops to 30, hand-off latency. Test: `live.exe --seconds 15`, CPU avg and peak WS lines. (Subtask 08 already plans this.)
2. **`WGPU_BACKEND=dx12` environment override, no code change** (test only). Expected: different CPU per frame on the AMD iGPU. UNPROVEN; wgpu env var at wgpu-types-29.0.4 backend.rs:157; whether Bevy honors it is UNPROVEN. Risk: none to code. Test: `live.exe --seconds 15` with and without the variable, two runs each.
3. **`WGPU_BACKEND=vulkan` (current) vs `dx12` via `WgpuSettings { backends: Some(Backends::DX12) }`** in `RenderPlugin`. Same test as candidate 2 but in code. Risk: a possible driver difference; Vulkan has the flicker issue on AMD (wgpu #7922, correctness only).
4. **`PipelinedRenderingPlugin` disabled** via `DefaultPlugins.build().disable::<PipelinedRenderingPlugin>()`. Expected: one fewer thread and one less frame of extracted data. UNPROVEN size. Citation for existence: bevy_internal-0.19.1 default_plugins.rs:55 to 57. Risk: may lower fps (the render thread no longer overlaps). Test: `live.exe --seconds 15`, both lines. (Subtask 10.)
5. **Limit task pool threads** (`TaskPoolPlugin` with fewer threads). Expected: less spinning. Related report: #4718 (throttled ParallelExecutor overhead). UNPROVEN for 0.19. Risk: slower CPU-side systems. Test: `live.exe --seconds 15`. (Subtask 09.)
6. **`RenderAssetUsages::RENDER_WORLD` on the avatar's textures** (needs a patch or fork of bevy_vrm1 or a custom loader hook). Expected: removes the CPU copy of the avatar textures after upload. Citations: docs.rs RenderAssetUsages; bevy_vrm1 material.rs. Size UNPROVEN. Risk: the avatar's CPU data cannot be re-read, and bevy_vrm1 may need it for later edits. Test: `live.exe --seconds 15`, peak WS line first.
7. **`WgpuSettings { memory_hints: MemoryHints::MemoryUsage }`**. Expected: smaller GPU allocator blocks, so a lower working set on the shared iGPU memory. UNPROVEN. Citation: docs.rs MemoryHints. Risk: may slow allocations. Test: `live.exe --seconds 15`, peak WS line.
8. **`WinitSettings::reactive_low_power`** (already tested, reverted). Listed only for completeness. Do not repeat without a scene that is idle.

Not listed as a candidate: bevy_framepace (no Bevy 0.19 build found); power preference `LowPower` (same iGPU adapter; no CPU effect expected).

## 10-line summary

1. No Bevy issue matches this exact Windows iGPU symptom; the closest are #10261 (macOS) and #11285 (Windows, 0.12, open).
2. The 0.19 API is `RenderPlugin { render_creation: RenderCreation::Automatic(WgpuSettings { backends, power_preference, memory_hints, ... }) }`; sources cited above.
3. Vulkan vs DX12 on an AMD iGPU for wgpu: no evidence found; UNPROVEN either way; measure it.
4. `PowerPreference` on one iGPU selects the same adapter; no CPU gain expected.
5. 400 MB: CPU copies of assets (default `MAIN_WORLD | RENDER_WORLD`), GPU upload, render world; split UNPROVEN until attributed.
6. `RENDER_WORLD`-only on avatar textures is the cleanest memory cut but needs a bevy_vrm1 patch; gain UNPROVEN.
7. bevy_framepace 0.21.0 targets Bevy 0.18, not 0.19; not usable as-is.
8. Reactive WinitSettings is the Bevy example for low CPU; already tested and reverted here (only helps idle scenes).
9. Best next steps: 30 fps app cap (subtask 08), backend env test (no code), pipelined off (subtask 10); two `live.exe --seconds 15` runs each.
10. Reddit and Discord were not reachable; no scrapling tool; those sources are UNPROVEN.
