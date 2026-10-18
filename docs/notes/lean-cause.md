# Lean cause: evidence for H1, H2, H3

Scope: research only. No builds, no benchmarks, no camera. Target: bare Bevy 0.19 / wgpu 29 window, Windows 11, AMD Ryzen 5 7430U iGPU, Vulkan.

Measured by the user (not published, not re-measured here): bare window about 98% of one core, 314 MB working set (peak private about 446 MB); 30 fps cap about 70%; camera plus ONNX tracker without window 13% and 55 MB.

Local sources read: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` for wgpu-hal 29.0.4, wgpu 29.0.4, bevy_render 0.19.1, gpu-allocator 0.28.0.

## Verdict summary

| Hypothesis | Verdict | Basis |
|---|---|---|
| H1 Bevy engine overhead is the main CPU cost | UNPROVEN, leaning against as sole cause | No published Windows iGPU number matches. No per-thread profile published. |
| H2 wgpu plus Vulkan driver is the main cost, so every wgpu library shares the floor | PARTIAL: the mechanism exists in wgpu-hal, its CPU size is UNPROVEN | Acquire-path fence wait and Windows Vulkan frame-pacing reports. No measured floor. |
| H3 gpu-allocator, shader cache and swapchain explain the memory | PARTIAL: block sizes can explain tens to hundreds of MB; split of the 314 MB is UNPROVEN | wgpu-hal sets block sizes from MemoryHints. Vulkan memory issues reported on NVIDIA. |

## H1: Bevy engine overhead

Evidence found:
- Bevy issue #21465 (https://github.com/bevyengine/bevy/issues/21465): memory jump from about 58 MiB to 466 MiB when the render plugin loads. Reported on Linux and Mint, not Windows. Maintainer attributes it only as a suspicion. No Windows numbers and no per-thread profile in the thread. Status: suspected, not confirmed.
- Local code (bevy_render 0.19.1, `src/renderer/render_device.rs:123`): `RenderDevice::poll` is a thin wrapper over `wgpu::Device::poll`. Bevy calls `poll(PollType::wait_indefinitely())` only in `src/diagnostic/tracy_gpu.rs:77`, which is the Tracy GPU path. The frame loop does not call a blocking poll in the code I read. So a Bevy-side blocking poll is not the cause in the default build. UNPROVEN for the full frame loop: I did not read the whole render schedule.
- The Bevy scheduler and task pools are not shown by any published measurement to consume a full core on an idle window. UNPROVEN.

Verdict: UNPROVEN. No evidence that Bevy scheduling is the main cost. Not ruled out either.

## H2: wgpu plus Vulkan driver

Evidence found:
- wgpu issue #8310 (https://github.com/gfx-rs/wgpu/issues/8310): with `PresentMode::Fifo` on Windows Vulkan, reported about 75 expected FPS, about 25 observed. Frame-pacing problem, not CPU cost. Cited from earlier fetch.
- wgpu issue #8354 (https://github.com/gfx-rs/wgpu/issues/8354): proposes a DXGI swapchain path on Windows for better pacing. Cited from earlier fetch.
- wgpu PR #8420 (https://github.com/gfx-rs/wgpu/pull/8420): title "[hal/vulkan] Wait for fence signalled in vkAcquireNextImage", stated purpose "This prevents frame pacing issues on vulkan/windows." Cited from earlier fetch. A search summary claimed this landed in wgpu 26.0.6 and 27.0.4 release notes. NOT CONFIRMED: I did not open those release notes.
- Local code (wgpu-hal 29.0.4, `src/vulkan/swapchain/native.rs` about lines 405-440): the acquire path waits on the previous submission's fence before `vkAcquireNextImageKHR`. The source comment says without this "the CPU could crank out frames much faster than the presentation engine can display them." This is a blocking point on the CPU thread that calls acquire. Whether it spins or sleeps: UNPROVEN.
- Local code (wgpu-hal 29.0.4, `src/vulkan/device.rs` about line 2747): `wait_for_fence`, with a timeline-semaphore path.
- Local code (wgpu 29.0.4, `src/api/surface.rs` lines 120-126): `get_current_texture` returns `CurrentSurfaceTexture::Timeout` on `SurfaceStatus::Timeout`. Blocking-with-timeout behaviour is consistent with the Fifo queue depth.

What the evidence does not show:
- CPU share of the acquire wait on AMD Vulkan Windows. UNPROVEN.
- AMD driver worker threads on Windows. UNPROVEN (not researched to a source in this run).
- That the floor is the same for every wgpu library. The mechanism lives in wgpu-hal, so any library that uses wgpu Vulkan on Windows inherits the acquire path. That is a code-level inference, not a measured floor. UNPROVEN as a number.

Verdict: PARTIAL SUPPORT. The mechanism is in the stack and has published pacing problems on Windows. Its CPU magnitude is not measured.

## H3: memory

Evidence found:
- Local code (wgpu-hal 29.0.4, `src/lib.rs` about lines 434-459): `AllocationSizes::from_memory_hints`. `Performance` uses device blocks of 128-256 MB and host blocks of 64-128 MB. `MemoryUsage` uses device blocks of 8-64 MB and host blocks of 4-32 MB.
- Local code (gpu-allocator 0.28.0, `src/lib.rs` about lines 310-380): `AllocationSizes::new(device, host)`. The doc example uses 256 MB device and 64 MB host maxima.
- On a shared-memory iGPU, device-local blocks are system RAM. A 128-256 MB block can be committed as private bytes once touched. This is a plausible source of the 446 MB peak private figure. Not measured.
- wgpu issue #24722 (https://github.com/bevyengine/bevy/issues/24722): Vulkan memory issues on NVIDIA, DX12 avoids them there. Cited from earlier fetch. Different vendor, so only partial relevance.
- Bevy issue #21465 suspects gpu-allocator pre-allocation. james7132 raised it. Unverified.

What the evidence does not show:
- Split of the 314 MB working set between driver, gpu-allocator blocks, shader cache, swapchain images and asset copies. UNPROVEN.
- Whether `MemoryHints::MemoryUsage` lowers the working set on this machine. UNPROVEN.
- Bevy `RenderAssetUsages` (MAIN_WORLD plus RENDER_WORLD) keeping CPU copies of textures: a candidate from `docs/notes/eff-bevy.md` for avatar textures, gain UNPROVEN.

Verdict: PARTIAL SUPPORT. Block sizing can explain a large share of committed memory. Not measured for this app.

## UNPROVEN items (collected)

1. CPU share of the wgpu acquire fence wait on AMD Vulkan Windows, and whether it spins or sleeps.
2. AMD Vulkan driver worker threads on Windows and their idle cost.
3. The bare wgpu floor versus the bare Bevy window (CPU and MB). This is task 2.1.
4. Per-thread CPU attribution of the bare Bevy window. This is task 2.2.
5. Split of the 314 MB working set.
6. Whether the wgpu 26.0.6 / 27.0.4 release notes include #8420 (not opened).
7. Effect of `RenderAssetUsages` on avatar texture memory (from prior notes, not measured).
8. Whether any published Bevy or wgpu minimal-window number exists for Windows iGPU (none found in this run).
9. Minimal hello-window CPU and RAM in SDL3, raw GL, Godot or raylib: not researched in this run.

## Proposed experiments

Each experiment changes one variable. Run on the same machine, same 30 fps cap, 60-second steady state after warm-up, record CPU as percent of one core and working set plus private bytes.

- E1 (H2, floor): bare wgpu window, clear colour only, `PresentMode::Fifo`, at 30 fps cap. Compare with bare Bevy window (same cap). If the wgpu floor is within about 10 points of Bevy, the cost is below Bevy (H2 holds). If much lower, H1 holds (Bevy-side cost).
- E2 (H2, thread attribution): Process Explorer thread view or Windows Performance Recorder on the bare Bevy window. Sort threads by CPU. Look for the thread that calls `vkAcquireNextImageKHR` and the Vulkan driver worker threads. Output: which thread owns the 98%.
- E3 (H2, present mode): bare Bevy window with `PresentMode::AutoNoVsync` or `Immediate` versus `Fifo`, uncapped. Result shows whether the cost tracks frame rate (pacing or spin) or stays flat (per-frame work).
- E4 (H2, backend): `WGPU_BACKEND=dx12` versus `WGPU_BACKEND=vulkan` on the bare Bevy window. Same cap. Compares CPU and RAM across backends on the same GPU.
- E5 (H3, memory): `WgpuSettings::memory_hints = MemoryHints::MemoryUsage` versus `Performance` on the bare Bevy window. Compare working set and private bytes.
- E6 (H1, pipelining): disable `PipelinedRenderingPlugin` on the bare Bevy window. Compare CPU and frame time. A change points at the render thread split.
- E7 (H3, shader cache): record the working set at startup before any frame is drawn, then after the first frame, then after 60 seconds. A step at the first frame points at pipeline or shader creation. A rise over time points at allocator growth.
- E8 (H2, poll cadence): count calls to `Device::poll` and `get_current_texture` per second. If the count tracks frame rate, the cost is per frame. If the wait is long per call, the cost is blocking.

Decision rule from design.md applies: if E1 shows wgpu floor close to Bevy, report the stack cost below any library and stop. If E1 shows a large gap, H1 is the candidate and the leanest library choice follows.

## Status

- Background research agents (Haiku, library survey and wgpu floor spike) were still running when this note was written. Their output files were empty at the time of reading. Their results are not included here.
- Not researched in this run: AMD driver threads, device.poll CPU, SDL3/GL/Godot/raylib hello-window numbers, Reddit reports.
