# Lean rendering libraries for a VRM avatar (task 1.1)

Scope: find an existing library (Rust first) that can show a skinned glTF or VRM avatar, set one head bone rotation and about 12 expression (morph) weights per frame at 30 fps, with the lowest CPU and RAM on an integrated GPU. No custom renderer. Research only: no builds, no benchmarks. Every claim carries a URL. Items not checked against source or a measurement are marked UNPROVEN. Dates are as of 2026-10-17.

## Short answer

Nothing checked so far beats the Bevy baseline on a published measurement. No candidate has a published CPU or RAM figure for this workload, so the comparison is open. The one Rust library with verified skin and morph code is kiss3d, but it has no VRM loader, no MToon, and no measurements. Godot 4 with godot-vrm is the most complete ready-made avatar stack but its memory figures are anecdotal and not clearly lower than Bevy. The measurement spikes (tasks 2.1 and 2.2) are still needed before any switch.

## Baseline (already measured in this project)

- Bare Bevy window: about 98 percent CPU, 314 MB at 60 fps. Camera plus tracker without a window: 13 percent CPU, 55 MB. Source: `docs/notes/eff-vrm.md`, `openspec/changes/evaluate-lean-renderer/design.md` (local, measured by the project, not a public figure).
- Live app: 70 to 100 percent of one core, 440 to 530 MB (`openspec/changes/evaluate-lean-renderer/proposal.md`).
- bevy_vrm1 `include_source` is hard-coded true at `src/vrm/loader.rs:109`, keeps CPU copies of meshes, and only supports preset expressions (aa, ih, ou, ee, oh, blink) (`docs/notes/eff-vrm.md`, local).

## Re-checks of earlier dismissals

- rend3 (https://github.com/BVE-Reborn/rend3): archived, Apache-2.0, last push 2024-07-08, README says maintenance mode. Dismissal stands. The archive date 2025-06-07 from the old note is NOT re-verified.
- three-d (https://github.com/asny/three-d): MIT (sidebar licence shown on the repo page), last push 2026-06-24, 1681 stars. The README does not mention glTF, skinning, or morphs. A code search for morph or skin returned nothing, which is weak evidence only. Skin and morph support is UNVERIFIED. Dismissal is reopened only as "unknown", pending a source read.
- macroquad (https://github.com/not-fl3/macroquad): Apache-2.0, last push 2026-08-18, 4668 stars. Skin and morph hits are UI skins only. No 3D skinning evidence found. Dismissal stands (UNVERIFIED for 3D skinning).

## Candidates

Fields: licence / last push (GitHub, checked 2026-10-17) / skinned / morph / toon, MToon, unlit / VRM loader / Windows / published CPU and RAM / integration effort.

### 1. kiss3d (https://github.com/dimforge/kiss3d)

- Licence: BSD-3-Clause (repo page link label). Last push 2026-10-04. 1789 stars. No GitHub release shown; README gives versions 0.37 and 0.40 in install snippets.
- Skinned: yes, verified in source. `src/loader/gltf.rs` has `build_skin_parts` (joints, inverse bind matrices) and `build_primitive` (joint and weight attributes, `set_skin_vertices`). Source: https://github.com/dimforge/kiss3d/blob/master/src/loader/gltf.rs
- Morph: yes, verified in source. `src/loader/gltf.rs` `build_primitive` (`set_morph_targets`), `build_model` (default weights), `build_clip` (weight animation channels). Limit: a fixed maximum number of morph targets is kept, extra targets are ignored (same file). Blending is on the GPU: `src/builtin/deform.rs` defines `DeformControl` with `weights` packed four per vec4, `MAX_MORPH_TARGETS = 64`, `set_weights`, `DeformGpu::update`. The WGSL shader is not read, so the GPU behaviour is from module docs only. Source: https://github.com/dimforge/kiss3d/blob/master/src/builtin/deform.rs
- Toon, MToon, unlit: UNPROVEN. README lists only basic primitives, colour and texture, and post-processing (https://github.com/dimforge/kiss3d). MToon would need custom shading work.
- VRM loader: none found. The VRM 1.0 expression data would need to be read from the glTF JSON extension (`VRMC_vrm`) by our code. UNPROVEN as a plan.
- Windows: not stated in the README beyond FFmpeg install steps (https://github.com/dimforge/kiss3d). Backend (wgpu or other) not stated in the README. UNVERIFIED.
- Published CPU or RAM: none found. UNPROVEN.
- Integration effort: UNPROVEN estimate of several days for a skinned load plus 12 morph weights, plus more for MToon-like shading and the VRM extension reader. Not measured.
- Maturity risk: README is sparse, the examples list has no skin, morph, glTF, or VRM example (https://github.com/dimforge/kiss3d/tree/master/examples, checked only via the index in the candidate listing).

### 2. three-d (https://github.com/asny/three-d)

- Licence: MIT (sidebar). Last push 2026-06-24. 1681 stars. No release shown.
- Skinned, morph: UNVERIFIED. README has no feature list for these. The examples index was not returned in full; only an "animation" example name was seen. Needs a source read of `three-d-asset` and the animation module.
- Toon, MToon, unlit: UNVERIFIED.
- VRM loader: none found.
- Windows: UNVERIFIED.
- CPU or RAM: none. Integration effort: not estimated until skin and morph are verified.

### 3. Bevy with bevy_vrm1 (baseline, https://github.com/bevyengine/bevy and https://github.com/not-elm/bevy_vrm1)

- Bevy: Apache-2.0, last push 2026-10-16, 49084 stars (https://github.com/bevyengine/bevy).
- bevy_vrm1: Apache-2.0 (gh SPDX), last push 2026-10-03, 39 stars (https://github.com/not-elm/bevy_vrm1). Features and limits as in the baseline section above.
- CPU and RAM: the project's own measurement (above). No published figure.
- Integration effort: zero for the current app. Lean alternative would be Bevy with fewer plugins (UNPROVEN, not measured).

### 4. unavi-xyz/bevy_vrm (https://github.com/unavi-xyz/bevy_vrm)

- Licence: MIT OR Apache-2.0. Last push 2026-10-13. 67 stars. v0.5.0, Bevy 0.20.0 (repo README).
- README: humanoid bones, MToon, first person, spring bones. Not yet: expressions, look-at, node constraints, spring colliders (repo README). So it lacks the expression path we need.
- CPU or RAM: none published.

### 5. Godot 4 with godot-vrm (https://github.com/V-Sekai/godot-vrm)

- Licence: the LICENSE file is MIT (Copyright 2020-2021 V-Sekai Contributors; 2020 VRM Consortium). GitHub shows "Other" because the sample assets have separate licences (vrm_samples/LICENSE_SAMPLES.txt). Last push 2026-07-08, 477 stars.
- Godot: 4.7.2-stable released 2026-08-18 (https://github.com/godotengine/godot/releases). Godot is MIT.
- Skinned, VRM: yes, VRM 0.0 and 1.0 import, VRM 1.0 export, VRMC_materials_mtoon implemented, VRMC_springBone implemented "but needs optimization", VRMC_node_constraint buggy with retargeting (godot-vrm README).
- Expressions: blend shapes and material colour or UV implemented as animation tracks meant for a BlendTree Add2. Look-at creates animation tracks only; the app must build a BlendSpace2D (godot-vrm README). So driving 12 weights per frame means writing to animation or blend-shape properties from our code. UNPROVEN cost.
- Toon, MToon: MToon material is implemented (godot-vrm README). Runtime cost not measured.
- Windows: Godot 4 supports Windows (anecdotal Windows reports below).
- CPU or RAM: anecdotal only. Forum post on Godot 4.6 beta1 Windows 11 (https://forum.godotengine.org/t/default-memory-usage-concerns-200mb-400mb-for-empty-project/129801): exported empty project and scene use about 200 MB with d3d12 and about 400 MB with Vulkan. Debug monitor memory about 65 MiB. No idle CPU figure on that page. Mitigations suggested by a maintainer there: Dummy audio and physics servers, and the Compatibility renderer (same page). An older 100 MB report: https://forum.godotengine.org/t/why-new-empty-project-takes-100-mb-of-memory-to-run-windows/26291. Uncapped frame rate and vsync issue: https://github.com/godotengine/godot/issues/87020. Controlled integrated-GPU benchmark: UNPROVEN, none found.
- Driving from Rust: godot-rust/gdext (https://github.com/godot-rust/gdext) is MPL-2.0, last push 2026-10-04, 5298 stars, latest release v0.5.5 on 2026-08-09. A gdext extension runs inside the Godot process. A separate Rust process driving Godot over UDP or IPC is possible but its latency and CPU cost are UNPROVEN. Export size: UNPROVEN (not measured).
- Integration effort: UNPROVEN estimate of about a week for a Godot project that loads VRM, drives the head bone and 12 weights, and exposes the tracker data, plus added language and tooling cost if the app stays Rust.

### 6. WebView2 with three.js and @pixiv/three-vrm (fallback only)

The user requires a native non-browser app, so this is listed for completeness and is not ranked.

- three-vrm: MIT (https://github.com/pixiv/three-vrm). Last push 2026-10-16, 2220 stars. Latest release v3.5.5 published 2026-07-09 (repo releases). Package version 3.5.5 in `packages/three-vrm/package.json`. Peer three >= 0.137, dev dependency three ^0.180.0.
- WebGPU: the 2023 discussion says there was no WebGPU plan at the time: https://github.com/pixiv/three-vrm/discussions/1278 (maintainer 0b5vr, 2023-08-29: "There is no plan to support WebGPU at the moment."). The package `three-vrm-materials-mtoon` has a `src/nodes` directory (listing seen), so a node-material path exists in the current code, but the version that added it is UNVERIFIED.
- WebView2 process model: browser process, renderer processes, GPU and audio helper processes per user data folder (https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model). That page gives no memory figures.
- WebView2 memory: no official figure found (UNPROVEN). Anecdotes of about 300 MB idle versus about 18 MB native for a comparable app (https://windowsforum.com/news/whatsapp-windows-switches-to-webview2-memory-use-soars-and-windows-11-polish-drops.389415/). Anecdotal only.
- Verdict: likely no RAM win over the 440 to 530 MB baseline, and it breaks the native-app requirement. Fallback only.

### 7. Other Rust options checked only at metadata level

- gfx-rs/wgpu (https://github.com/gfx-rs/wgpu): Apache-2.0, last push 2026-10-17, 18313 stars. It is the layer under Bevy. No skin or morph code of its own. Its floor cost is the subject of task 2.1.
- gltf crate (https://github.com/gltf-rs/gltf): Apache-2.0, last push 2026-05-11, 644 stars. Parser only: no renderer. Morph and skin data parsing is UNVERIFIED for this run; use it if a custom consumer were ever needed (user said no custom renderer, so not a candidate).
- raylib (https://github.com/raysan5/raylib): code search shows `src/rmodels.c`, `src/external/cgltf.h`, and a glTF example, but no "blendshape" hit. Morph support UNPROVEN. Not a checked candidate.
- Fyrox (https://github.com/FyroxEngine/Fyrox): MIT, last push 2026-10-17, 9598 stars. A full engine, heavy by design. CPU and RAM UNPROVEN, not a lean candidate.
- rend3: see re-check above.
- VRM4U (https://github.com/ruyo/VRM4U): Unreal plugin, last push 2026-10-17, 1994 stars. Not a Rust option. The owner in the earlier note (VRM4U/VRM4U) returned 404; ruyo/VRM4U is the repo that exists. Licence not checked.
- VRMMetalKit (https://github.com/arkavo-org/VRMMetalKit): Swift, Apache-2.0, last push 2026-10-14, 7 stars. Apple Metal only, so not usable on Windows. The owner cacaoplay/VRMMetalKit returned 404 (UNVERIFIED that it exists).
- KAGRA (https://github.com/EMMA019/KAGRA): MIT, last push 2026-09-12, 0 stars. Python and Rust. Not checked in detail; MToon and expressions claim UNVERIFIED.
- A.R.I.A (https://github.com/NekoUnix/A.R.I.A): MIT, last push 2026-10-13, 10 stars. Live2D, VRM, GLB, OBS output. Not checked in detail; UNVERIFIED for render cost.
- Brioche-Puppet (https://github.com/HanetakaChou/Brioche-Puppet): LGPL-3.0, last push 2026-07-21, 14 stars. Not checked in detail.

## Ranked top 3 (honest verdict)

1. kiss3d as a lighter core, with our own VRM extension reader and MToon-like shading. Verified: skin and morph code paths in source (URLs above). Unknown: MToon, Windows, CPU and RAM, real effort. Risk: sparse docs, no VRM support, small community. Effort: UNPROVEN, several days to first render, more for toon look. This is a library, not a custom renderer, but it is still new code on top of it.
2. Stay on Bevy and reduce cost inside it (bevy_vrm1 with include_source off if the crate allows, fewer plugins, frame cap). Verified: current feature set and the 314 MB baseline (local measurement). Unknown: how much a trimmed Bevy saves. Effort: low, but the expression limits remain.
3. Godot 4 with godot-vrm. Verified: MIT addon, VRM 1.0 expressions and spring bones, MToon implemented. Unknown: memory (anecdotes say 200 to 400 MB for an empty scene, not clearly below Bevy's 314 MB bare), idle CPU, export size, and the Rust control path. Effort: UNPROVEN, about a week. Largest change in language and runtime.

WebView2 is excluded from the ranking (fallback only, see above).

Verdict: nothing proven beats Bevy. kiss3d is the only lean Rust library with verified skinning and morph code, but it has no VRM path and no measurements. Godot is the only full avatar stack, but its published memory is not lower. The decision needs the wgpu floor spike (task 2.1) and per-thread attribution (task 2.2) before any switch.

## Risks

- No candidate has a CPU or RAM figure for this workload. All comparisons above are open.
- kiss3d: sparse documentation, unverified Windows backend and toon support, no VRM loader.
- three-d: skin and morph unverified; if absent, it is out.
- godot-vrm: "needs optimization" for spring bones; constraint retargeting buggy; expression path is animation-track based, which may not suit 30 fps per-frame writes.
- Godot memory: anecdotal, mostly on Windows 11 with beta builds.
- Licence: godot-vrm sample assets have separate licences; check before shipping any bundled sample.

## Ten-line summary

1. Nothing checked beats Bevy on published numbers; no candidate has a CPU or RAM figure for this workload.
2. kiss3d (BSD-3-Clause) has verified skin and morph code in source (src/loader/gltf.rs, src/builtin/deform.rs).
3. kiss3d has no VRM loader, no MToon, and no documented Windows backend (UNVERIFIED).
4. three-d (MIT) skin and morph support is UNVERIFIED; README is silent.
5. rend3 is archived (maintenance mode); macroquad shows no 3D skinning evidence.
6. Godot 4 + godot-vrm (MIT) has VRM 1.0, expressions as animation tracks, spring bones, MToon; memory is anecdotal (200 to 400 MB empty scene).
7. unavi bevy_vrm (MIT OR Apache-2.0) lacks expressions and look-at.
8. WebView2 + three-vrm v3.5.5 is fallback only; WebGPU support status conflicts between 2023 discussion and later code.
9. Verdict: keep Bevy until task 2.1 and 2.2 measurements show where the cost sits.
10. Next: run the wgpu floor spike and per-thread attribution; then re-rank kiss3d and Godot on measured data.
