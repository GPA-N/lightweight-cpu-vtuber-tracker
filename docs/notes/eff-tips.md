# Efficiency tips: CPU and RAM for the Bevy 0.19 + wgpu avatar window

Research only. No code, build, benchmark, or camera run was done for this note. Every claim has a URL. Anything not backed by a source or by a measurement from this project is marked UNPROVEN.

Measured baseline (from openspec/changes/reduce-live-cpu-and-memory/design.md): window plus avatar about 75 CPU points and about 400 MB; tracker alone about 13% CPU and 55 MB. Run-to-run noise is 10 to 15% CPU and up to 67 MB.

Lockfile versions (Cargo.lock): bevy 0.19.1, bevy_vrm1 0.10.0, wgpu 29.0.4.

## 1. General tricks for small real-time wgpu/game windows on Windows

- **Frame pacing.** Bevy's `PresentMode::Fifo` is the traditional vsync mode, capped at the display refresh rate; `Immediate` and `Mailbox` are not capped by refresh and may be unavailable on some platforms. Source: ctx7 `/websites/rs_bevy` (docs.rs PresentMode page returned HTTP 404 in this pass; the canonical URL is https://docs.rs/bevy/0.19.1/bevy/window/enum.PresentMode.html , not re-fetched). The Bevy low_power example uses `PresentMode::AutoNoVsync` with a comment about maximizing CPU/GPU usage, which is the opposite of a frame cap. Source: ctx7 `/websites/rs_bevy` low_power example (URL not re-fetched; UNPROVEN as a direct quote).
- **Reactive update mode.** `WinitSettings { focused_mode, unfocused_mode }` and `UpdateMode::reactive_low_power(Duration)` control how often the loop runs. Source: https://docs.rs/bevy/latest/bevy/winit/struct.WinitSettings.html . This project tried `reactive_low_power(25 ms)` and reverted it: fps fell to 40 and hand-off latency rose 2 to 4 ms (design.md row 3).
- **Eco QoS for background work.** Windows classifies a process as EcoQoS when it opts into `PROCESS_POWER_THROTTLING_EXECUTION_SPEED`, which lowers frequency or moves work to efficient cores. Microsoft's page says: "EcoQoS should not be used for performance critical or foreground user experiences." The sample sets `ControlMask` and `StateMask` to `PROCESS_POWER_THROTTLING_EXECUTION_SPEED` to enable EcoQoS, and sets `StateMask` to 0 with `ControlMask` set to disable it. Source: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation . It fits only when the window is unfocused or hidden, not while the avatar is being shown to the user.
- **Working-set trimming.** `EmptyWorkingSet` (or `SetProcessWorkingSetSize(-1, -1)`) "removes as many pages as possible from the working set." Source: https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-emptyworkingset . Trimmed pages stay cached as transition pages until reused, and touching them again causes page faults. Source: https://learn.microsoft.com/en-us/windows/win32/memory/working-set . It does not reduce committed memory.
- **Graphics backend.** On Windows, the wgpu backend choice may matter for memory. Issue #24722 reports Vulkan out-of-memory on Bevy 0.19 with wgpu 28/29, and the reporter says DX12 is unaffected: https://github.com/bevyengine/bevy/issues/24722 . The reporters used NVIDIA RTX 4070 Ti and RTX 3060 Ti, not AMD, so the effect on this AMD iGPU is UNPROVEN. wgpu PR #9462 is linked; its merge status is unknown.
- **Render plugin cost.** In Bevy v0.17.2, a minimal app measured about 58 MiB without `RenderPlugin` and about 466 MiB with it; `DefaultPlugins` about 590 MiB. (An earlier v0.16.x figure was "just over 400MiB".) A later reporter said usage dropped to about 220 MiB on an AMD GPU; the GPU model is not confirmed and the reporter also ran system updates, so the GPU effect is UNPROVEN. Maintainer james7132 said "We should profile this" and named gpu-allocator as a likely culprit. Source: https://github.com/bevyengine/bevy/issues/21465 .
- **Allocator.** mimalloc as `#[global_allocator]` raised committed memory in a Rust PR benchmark: committed +45.13%, physical +11.04%, runtime -7.6% (mimalloc v3, Rust and C, regex release; Zoxc comment, 28 Apr 2026). Source: https://github.com/rust-lang/rust/pull/138764 . Not a RAM lever for this app; effect here UNPROVEN, not tested.
- **Release profile.** Cargo's release defaults are opt-level 3, lto false (thin local LTO), codegen-units 16, panic unwind. Source: https://doc.rust-lang.org/cargo/reference/profiles.html . Tuning these is a standard CPU lever, but the size of the gain for this app is UNPROVEN.

## 2. Which apply here

The project has no `[profile.*]` at the workspace root (Cargo.toml). Mapping each item to this code:

- **Backend.** Bevy's render backend is set through `RenderPlugin`/`WgpuSettings`. The env var `WGPU_BACKEND=dx12` was used by the issue reporters and is the cheapest test. Effect on this laptop's private bytes and CPU: UNPROVEN, must be measured (one variable, two runs).
- **Release profile.** Missing entirely, so release builds use Cargo defaults. Adding thin LTO and one codegen unit can cut CPU in hot loops, at the cost of a slower build. Expected gain: UNPROVEN.
- **Frame cap (subtask 08).** The project already has `reactive_low_power(25 ms)`, which was reverted. A 30 fps cap via `Fifo` plus `UpdateMode::Continuous` may be a cleaner test. Effect: UNPROVEN here.
- **Task pool threads (subtask 09).** Limiting Bevy's task pools is a CPU cap that changes scheduling only. Exact API for Bevy 0.19 was not verified in this pass; UNPROVEN.
- **Pipelined rendering (subtask 10).** Turning it off trades a frame of latency for less thread overhead. Exact plugin name for 0.19 was not verified in this pass; UNPROVEN.
- **Eco QoS.** Useful only when the window is unfocused or hidden. Not for the foreground avatar.
- **Trimming.** Would lower the working set only. Re-touch page faults are UNPROVEN for this app.
- **mimalloc.** Not recommended here (see Section 1).

## 3. Private bytes (850 to 916 MB) versus working set (460 to 530 MB)

Working set is the set of pages "currently resident in physical memory." Source: https://learn.microsoft.com/en-us/windows/win32/memory/working-set . Private bytes is the commit charge, which includes memory that is committed but not resident (paged out, or never touched). The gap of about 400 MB is therefore memory that the process has committed but that is not in RAM at the sample time. Likely sources: Bevy and wgpu heaps, mapped VRM assets, and driver user-mode allocations. Which share is the driver on AMD Windows: UNPROVEN, no source found in this pass.

Does it matter for a 1 GB working-set target? The measured working set is already under 1 GB in every live run (peak 465 to 470 MB, design.md), so the target is met on working set today. Private bytes (peak 857 to 862 MB) is under 1 GB too, but it counts against the system commit limit, so trimming does not lower it. Whether trimming costs visible stutter: UNPROVEN.

## 4. Cargo.toml `[profile.release]`

Checked the workspace root Cargo.toml: there is no `[profile.release]` and no `[profile.*]` at all. The release build uses Cargo's defaults listed in Section 1. Checked only the root manifest; the crate manifests (crates/avatar, crates/infer) were read for dependencies, not for profile sections.

## 5. Ranked candidates (at most 8)

1. **Test the DX12 backend** (one variable, two runs). Expected: RAM down if the Vulkan memory issue applies here. Risk: none to code; a different driver path. Source: https://github.com/bevyengine/bevy/issues/24722

   ```powershell
   $env:WGPU_BACKEND = "dx12"; .\target\release\live.exe --seconds 15
   ```

2. **Release profile tuning.** Expected: CPU down, effect UNPROVEN. Risk: longer build times. Source: https://doc.rust-lang.org/cargo/reference/profiles.html

   ```toml
   [profile.release]
   lto = "thin"
   codegen-units = 1
   ```

3. **30 fps frame cap (subtask 08).** Expected: CPU down, effect UNPROVEN; hand-off latency up (earlier 25 ms test added 2 to 4 ms). Risk: visible latency. Source: https://docs.rs/bevy/latest/bevy/winit/struct.WinitSettings.html

   ```rust
   .insert_resource(WinitSettings {
       focused_mode: bevy::winit::UpdateMode::reactive_low_power(std::time::Duration::from_millis(33)),
       unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(std::time::Duration::from_millis(100)),
   })
   ```

4. **Task pool thread cap (subtask 09).** Expected: CPU down, UNPROVEN. Risk: slower systems if the cap is too low. Exact Bevy 0.19 API to be verified via ctx7 `/websites/rs_bevy` before use.

5. **Pipelined rendering off (subtask 10).** Expected: thread overhead down, UNPROVEN. Risk: one frame of latency. Exact plugin disable call to be verified for 0.19.

6. **Eco QoS while unfocused.** Expected: CPU down only when the window is in the background; no effect on the foreground measurement. Risk: none for focused use. Source: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation

   Sketch (field and constant names from memory of the Microsoft sample; NOT compiled, NOT verified against windows-rs in this pass, so UNPROVEN):

   ```rust
   // Windows only; call when the window loses focus. Pass the struct to SetProcessInformation.
   let state = PROCESS_POWER_THROTTLING_STATE {
       Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
       ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
       StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED, // set = EcoQoS on; 0 with ControlMask set = off
   };
   // SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, &state as *const _ as _, size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32)
   ```

7. **Periodic working-set trim.** Expected: working set down, commit unchanged; re-touch faults UNPROVEN. Risk: stutter on reuse. Source: https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-emptyworkingset . Only for the 1 GB working-set metric, not a real fix.

8. **Replace or patch the renderer stack** (open question 14 in tasks.md). Largest possible gain (Bevy's RenderPlugin is reported at about 466 MiB in a minimal v0.17.2 app), but a large change. Source: https://github.com/bevyengine/bevy/issues/21465 . Decision belongs to the user.

## 6. Not done in this pass (gaps)

- Reddit pass with scrapling: not run. WebSearch found no Reddit threads on Bevy memory.
- wgpu `PowerPreference` and `PresentMode` docs via ctx7 `/websites/rs_wgpu` or `/gfx-rs/wgpu`: queried, but the results were not recorded, so no wgpu-side claim is made here.
- Source for Windows driver or Vulkan share of private bytes on AMD: none found (UNPROVEN).
- Exact Bevy 0.19 API for task pool threads (subtask 09) and disabling pipelined rendering (subtask 10): not verified.
- Parallel notes: docs/notes/eff-vrm.md (VRM renderer alternatives, written by a separate agent) covers the bevy_vrm1 `include_source` question; its contents were not read in this pass. docs/notes/eff-bevy.md (Bevy/wgpu agent ab6811070f021a346) was read and agrees with this note: no Bevy issue matches the Windows iGPU symptom (closest #10261 macOS, #11285 Windows 0.12 open); it adds `RENDER_WORLD`-only avatar textures (candidate 6) and `memory_hints` (candidate 7), both UNPROVEN, and gives the Bevy 0.19 `RenderPlugin`/`WgpuSettings` API with local source line numbers.
- Windows rendering-tips agent (a24c4a1e050dd8d6d) output was not incorporated; its findings are a gap.

## 7. Ten-line summary

1. Baseline: window plus avatar about 75 CPU points and 400 MB; tracker alone 13% and 55 MB (design.md).
2. Working set is resident memory; private bytes is committed memory; the 400 MB gap is committed-but-not-resident (Microsoft working-set doc).
3. Working set is already under 1 GB in every live run (465 to 470 MB peak); the gap is a metric problem, not a limit problem.
4. Trimming (EmptyWorkingSet) lowers working set only; it does not lower private bytes.
5. Project has no `[profile.release]`; Cargo defaults apply (lto false, codegen-units 16).
6. A Bevy/wgpu Vulkan VRAM regression is reported with DX12 unaffected (issue #24722); testing DX12 here is UNPROVEN as a fix.
7. RenderPlugin: about 58 MiB without it, about 466 MiB with it, in a minimal Bevy v0.17.2 app (issue #21465); the maintainer said "We should profile this" and no profile is posted in the issue as read.
8. The best next test is DX12 (one env var) and then the release profile; both are cheap to revert.
9. Frame caps and reactive modes were tried; the 25 ms reactive mode was inside noise and added latency (design.md).
10. Every CPU/RAM gain in this note is UNPROVEN until measured twice on this laptop in one session.
