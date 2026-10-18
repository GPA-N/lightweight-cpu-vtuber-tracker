# 11 Try the DX12 graphics backend instead of Vulkan

Read `00-rules.md` first (Debate protocol, no-face rule). Run this FIRST of the render subtasks: it needs no code change.

Fact: Bevy logs `backend: Vulkan` for this AMD integrated GPU. The research (docs/notes/eff-bevy.md, eff-tips.md) found no evidence either way on whether DX12 uses less CPU or memory on this GPU; the reports it found (bevy #24722) were NVIDIA. It must be measured. Bevy reads `WGPU_BACKEND` through `WgpuSettings::default()` (verify in `~/.cargo/registry/src/*/bevy_render-0.19*/src/settings.rs`: look for `Backends::from_env`).

Steps:
1. Use the existing `target\release\live.exe` (no build). Run a short check that the env var is honoured: `$env:WGPU_BACKEND = 'dx12'; .\target\release\live.exe --seconds 6 --out fixtures\private\out\live_s11_probe 2>&1 | Select-String 'backend|AdapterInfo'` must show `backend: Dx12`. If it still shows Vulkan, the variable is not honoured: STOP, report, and do not edit code (the code route is a separate subtask).
2. Alternating runs, no face, 15 s each, 3 pairs: Vulkan (env unset), DX12, Vulkan, DX12, Vulkan, DX12 (6 runs; the cap is raised to 6). Clear the variable with `Remove-Item Env:WGPU_BACKEND` for the Vulkan runs. Tags `s11_vk_<n>`, `s11_dx_<n>`.
3. Record for each run: CPU average, peak working set, peak private bytes, render fps, `tracker ready -> applied` median. Also look at the avatar once with `AVATAR_SHOT` under DX12 (Read the png): it must render correctly (not black, not flickering; wgpu has had DX12 and Vulkan visual bugs on AMD).

Debate: Hypothesis and Prediction (a number for CPU and for MB) BEFORE the runs; Result and Verdict after.

KEEP rule: DX12 is KEPT as a finding only if all three DX12 runs beat all three Vulkan runs on CPU average OR peak working set by more than 15 percent, render fps stays at least 55 and the screenshot is correct. Kept means: report the exact setting; do NOT change code (a later subtask will set `WgpuSettings` in code if it wins). Otherwise REVERT: nothing to revert, say DX12 not better.
