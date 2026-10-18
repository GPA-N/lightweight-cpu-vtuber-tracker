# Avatar window cost: lighter rendering options (research only)

Scope: research only. No code edits, no cargo, no benchmarks, no camera. Baseline numbers come from openspec/changes/reduce-live-cpu-and-memory/design.md and tasks.md (local files, not re-measured here). Every external claim cites a URL. Anything not proven by a source or a measurement is marked UNPROVEN.

## Starting facts (local, from the project's own measurements)

- Tracking only (camera + tracker, no window): 12.6 to 14.4 percent CPU, 55 MB working set (tasks.md section 3).
- Full live.exe with window, avatar and preview, no face: 85 to 93 percent CPU, 457 to 532 MB working set. The window therefore costs about 75 CPU points and about 400 MB (tasks.md section 3).
- Run-to-run noise on identical builds reaches 67 MB and about 10 CPU points (tasks.md section 3).
- Subtask 07 attribution: avatar is mainly a memory cost, the bare window is mainly the CPU cost (tasks.md section 1, item 7).
- Already tried and reverted or skipped: Msaa off (-8 MB, -6 CPU points, inside noise), WinitSettings reactive_low_power 25 ms (inside noise, adds 2 to 4 ms hand-off), ORT spinning off (kept, but it does not touch the window cost) (design.md results table).

## Question 1: bevy_vrm1 (version 0.10.0, Bevy 0.19)

Performance and memory facts, all from the crate source or docs:

- The crate is dual-licensed Apache-2.0 and MIT. Source: https://github.com/not-elm/bevy_vrm1 (repo page, "Apache-2.0, MIT licenses found").
- It targets Bevy 0.19 from crate 0.8.0 onward. Source: https://github.com/not-elm/bevy_vrm1 (README version table).
- The README warns: "This crate is in an early stage of development and may undergo breaking changes." Same source.
- The README lists five supported features: Spring Bone, Look At, Animation (VRMA), Node Constraint, First Person. MToon and expressions are not named in the README feature list. Same source.
- The README says nothing about performance, memory or CPU. Same source.
- Loader memory: `VrmLoader` calls the glTF loader with `include_source: true` (local registry file `bevy_vrm1-0.10.0/src/vrm/loader.rs`, line 109). The VRMA loader does the same (`src/vrma/loader.rs`, line 98). The bevy_gltf defaults keep CPU copies of meshes and materials (`load_meshes` and `load_materials` default to RenderAssetUsages MAIN|RENDER, per bevy_gltf-0.19.1 `src/loader/mod.rs` lines 185, 220-228, 1205-1212; source: https://github.com/bevyengine/bevy/tree/v0.19.1/crates/bevy_gltf). So the VRM's CPU-side copies are retained. Size of those copies for Seed-san: UNPROVEN (not measured).
- Toggles: none. Cargo features are only `default = []`, `develop`, `log`, `serde` (bevy_vrm1-0.10.0 `Cargo.toml` lines 36-40). Spring bones run automatically in PostUpdate with no component needed (`src/vrm/spring_bone/update.rs` lines 16-21). Look At is opt-in: it runs only when the component is on the VRM root (`src/vrm/look_at.rs` lines 10-11, 54-57). Body tracking is opt-in and is the only system that writes head rotation (`src/vrm/body_tracking.rs` lines 122-128, 427). MToon has no off switch.
- Expression API: the public API is `SetExpressions`, `ModifyExpressions` and `ClearExpressions` (`src/vrm/expressions.rs` lines 139, 195, 303), introduced by PR #48 merged 2026-02-14 (https://github.com/not-elm/bevy_vrm1/pull/48). Per frame, `bind_expressions` runs in PostUpdate with no `run_if`, so it iterates every expression entity every frame (`src/vrm/expressions.rs` lines 392-397, 464-471). Preset names only: aa, ih, ou, ee, oh, blink, blinkLeft, blinkRight (lines 25-26). ARKit names such as jawOpen are NOT FOUND in `src/`. So the ARKit-to-VRM mapping must stay in our code.
- Known issue status: issue #38 (expression API missing) was closed citing PR #40, but PR #40 changed system ordering and not `expressions.rs` (issue https://github.com/not-elm/bevy_vrm1/issues/38; PR https://github.com/not-elm/bevy_vrm1/pull/40). The expression API came from PR #48.
- Open issues or PRs about memory or CPU: none found in a search of the repo (`gh api search/issues` for performance, memory, cpu, spring, mtoon). Source: https://github.com/not-elm/bevy_vrm1/issues.
- Dependencies: serde, serde_json, anyhow, bitflags, paste, indexmap, plus `windows` 0.62 on Windows. No ort, nalgebra or ONNX crate in the direct manifest (`Cargo.toml` lines 15-29). Transitive Bevy dependencies: not checked.

Conclusion for question 1: bevy_vrm1 offers no toggle that would reduce the VRM's CPU-side copies or the cost of spring bones. The memory reduction needs a patch (set `include_source` to false, and ensure the render-only usage for glTF meshes), and a patch must also keep the expression path working.

## Question 2: lighter Rust options

Verified facts only. Skinning and morph support are the hard requirements (the VRM has joint skinning and morph-based expressions).

| Option | License | Skinning | Morph targets | Backend | Status (verified) | Source |
|---|---|---|---|---|---|---|
| three-d | MIT | Not stated on the repo page | Not stated | OpenGL 3.3, WebGL 2 | Releases section empty on the page, "0.9 release" mentioned in passing | https://github.com/asny/three-d |
| kiss3d (dimforge) | BSD-3-Clause | Not stated on page; v0.47.0 release notes mention skinning and morph support, disabled on devices with fewer than 5 vertex storage buffers | Same release notes | Not named on the page | Release notes mention `Context::supports_deform` | https://github.com/dimforge/kiss3d ; https://github.com/dimforge/kiss3d/pull/414 |
| rend3 | MIT OR Apache-2.0 OR Zlib | Not stated | Not stated; rend3-anim is "tied to rend3-gltf" | wgpu | ARCHIVED 2025-06-07, read-only | https://github.com/BVE-Reborn/rend3 |
| macroquad | Apache-2.0 / MIT | Not mentioned | Not mentioned | Not named (OpenGL implied) | 2D focused, no 3D model loading documented | https://github.com/not-fl3/macroquad |
| Fyrox | MIT | Not stated | Not stated | wgpu and GL crates exist (fyrox-graphics-wgpu) | Full game engine, not a library renderer | https://github.com/FyroxEngine/Fyrox |
| gltf crate | Apache-2.0 / MIT | Parses skins: `Skin::joints()` and `inverse_bind_matrices()` exist | Not in the Skin doc page; morph support NOT CONFIRMED here | None (parser only) | Parser, no rendering | https://github.com/gltf-rs/gltf ; https://docs.rs/gltf/latest/gltf/skin/struct.Skin.html |
| wgpu | MIT / Apache-2.0 | n/a | n/a | Vulkan, Metal, D3D12, GL; WebGPU on web | v30 latest | https://github.com/gfx-rs/wgpu |
| takahirox/wgpu-rust-renderer | MIT | Not mentioned | Not mentioned | WebGPU | "a tiny WebGPU Renderer written in Rust", 140 commits, 151 stars, version 0.0.4, docs "T.B.D." | https://github.com/takahirox/wgpu-rust-renderer |
| KAGRA (EMMA019) | MIT | Not stated by name | Not stated (VRM expression presets are listed) | wgpu 30, Python game logic | README lists "Full MToon (2-step shade, rim, outline, matcap/normal)" and "VRM 0/1 expression presets, SpringBone with colliders". Rust part "only ticks the world and renders". 429 commits, 0 stars | https://github.com/EMMA019/KAGRA |

Notes on the table:

- No candidate in this list was confirmed to support both skinning and morph targets with a documented API. kiss3d has the only confirmed skinning and morph release-note claim, but the page itself does not show an API example.
- The gltf crate is the only confirmed parser that exposes skins; it does not render.
- Any of these would still need a VRM 1.0 layer (VRMC_vrm extension, expressions, MToon material) written in our code, except KAGRA, which already does this in Python.

## Question 3: open-source native VTuber or face-to-avatar apps

- A.R.I.A (NekoUnix/A.R.I.A), MIT, "Cross-platform Avatar Studio (Alpha) in Rust", v0.37.0-alpha.1, 64 commits. Loads VRM 0.x and 1.0 with expressions, spring physics and gestures. Says "incomplete VRM shader compatibility". Renderer crate not named. Lightness tactics on its page: "Reduce canvas resolution, close unused outputs and check large avatar textures and competing apps." No benchmark figures. Source: https://github.com/NekoUnix/A.R.I.A
- Brioche-Puppet (HanetakaChou), LGPL-3.0, C++ with D3D12, Vulkan and Metal backends. Lists VRM as an input format. Does not mention skinning, morph or MToon. MediaPipe tracking. Rendering "implemented from scratch". No performance notes. Source: https://github.com/HanetakaChou/Brioche-Puppet
- openseeface-rs (ricky26), MIT and Apache-2.0 for the code, BSD-2 models from OpenSeeFace. A tracker with a Bevy debug frontend. Output endpoint hinted at 127.0.0.1:11573 (transport not documented). No CPU claim. "just a hobby project". Source: https://github.com/ricky26/openseeface-rs
- KAGRA (above): Python game logic, Rust rendering, MToon and VRM expressions, headless verification mode. Source: https://github.com/EMMA019/KAGRA
- VSeeFace: the official FAQ says CPU load is mainly from the separate tracker process facetracker.exe, that the default camera cap is 30 fps, and that raising it to 60 fps doubles tracker CPU. Phone tracking with no webcam removes most CPU load. Source: https://www.vseeface.icu/ (FAQ, via search result). License and renderer details: UNPROVEN. A user report (community, not a benchmark) says CPU dropped from about 25 percent to 15 percent after settings changes. Source: https://www.vseeface.icu/ (search summary); the figure is anecdotal.
- VTube Studio: a VTube Studio user reports its face tracker runs about 17 to 22 percent CPU on an i7-7700K. Source: https://steamcommunity.com/app/1325860/discussions/0/4768721849825975572/ (anecdotal).
- Warudo: the repo URL I tried returned 404; not verified. UNPROVEN for everything about Warudo.
- Reddit: r/bevy search was blocked (login wall) for plain fetch and for scrapling (`scrapling extract get`). No Reddit threads found. NOT FOUND.

Lightness tactics seen across these projects: cap the tracker frame rate (VSeeFace default 30 fps), reduce the canvas resolution, close unused outputs, use phone tracking to offload the webcam path. None of these reduce the renderer's own memory, which is the main cost here.

## Question 4: effort for a minimal wgpu VRM renderer

Scope: skinned mesh, MToon-like unlit material, about 12 morph expressions, no spring bones, no physics, no LookAt, window and camera included.

Estimate (UNPROVEN, an engineering estimate, not measured):

- glTF and VRM 1.0 loading via the gltf crate (skins, morph targets, materials, VRMC_vrm extension for expression names): 600 to 900 lines.
- wgpu setup, window (winit), surface, depth, one pipeline: 300 to 400 lines.
- Skinning: joint matrix buffer and vertex skinning shader: 200 to 300 lines.
- Morph targets: 12 expression weights mapped to morph deltas in a storage buffer, with the ARKit-to-VRM mapping kept in our code: 200 to 300 lines.
- MToon-like unlit material (base colour, texture, alpha mode, simple toon ramp): 200 to 300 lines.
- Camera, head rotation hook, render loop with a frame cap: 150 to 250 lines.

Total: about 1,700 to 2,400 lines of Rust plus shaders. For one developer already fluent in wgpu, that is roughly 4 to 7 weeks including debugging. For someone learning wgpu, double it. The largest risk is the VRM 1.0 spec details (expression override rules, MToon parameter handling) which would need to match what the avatar expects.

Reference points that help the estimate: kiss3d shows that a Rust library can carry skinning and morph support in a single renderer, but its API is not shown in the sources here. KAGRA shows that MToon plus VRM expressions in wgpu is reachable, but its Rust side is a small part of a Python project.

## Ranked candidates (at most 6)

Expected effects are UNPROVEN unless a measured figure is cited. Measured figures come from the project's own runs.

1. Keep Bevy, patch bevy_vrm1 (fork): set `include_source` to false in `src/vrm/loader.rs:109` and check that the render path still works without the source copies. Expected: some of the 410 MB non-tracker memory (VRM CPU copies plus glTF CPU copies); size UNPROVEN. CPU effect: none expected. Risk: medium (fork maintenance, early-stage crate, expression API churn). License: Apache-2.0 or MIT, fine.
2. Keep Bevy, keep bevy_vrm1, reduce the avatar's window cost in Bevy config: keep Msaa off, keep the frame cap question open, and attribute memory per asset (subtask 05). Expected: small, already measured as inside noise for Msaa. Risk: low. License: Bevy MIT or Apache-2.0.
3. Write a custom wgpu renderer on the gltf crate (the estimate above). Expected: largest reduction in both CPU and memory since Bevy's render graph and bevy_vrm1 are removed; the size is UNPROVEN until measured. Risk: high (4 to 7 weeks, spec fidelity). License: wgpu and gltf MIT or Apache-2.0.
4. Switch to kiss3d (BSD-3-Clause) for rendering. Expected: unknown. Risk: high, since skinning and morph are in release notes only and VRM and MToon are not supported. Needs a VRM layer anyway.
5. Adopt an external VTuber app (VSeeFace or VTube Studio) and drop the in-process avatar window. Expected: removes the 400 MB and most of the CPU if the external app is light; UNPROVEN. Risk: changes the product and the integration path; VSeeFace license and renderer: UNPROVEN. Licenses: not verified for either.
6. Fork KAGRA or A.R.I.A as a base. Expected: UNPROVEN. Risk: high (Python plus Rust mix for KAGRA; alpha status and "incomplete VRM shader compatibility" for A.R.I.A). License: MIT for both.

## Ten-line summary

1. The window costs about 75 CPU points and about 400 MB; tracking alone is about 13 percent and 55 MB (project measurements).
2. bevy_vrm1 0.10.0 has no toggle for its VRM CPU copies: `include_source` is hard-coded to true (src/vrm/loader.rs:109).
3. Spring bones always run; Look At and body tracking are opt-in; MToon cannot be turned off.
4. Expressions are preset-only (aa, ih, ou, ee, oh, blink); ARKit names must be mapped in our code.
5. No mature Rust crate confirmed to support skinning and morph targets with MToon in a small footprint.
6. rend3 is archived (2025-06-07); three-d is OpenGL; macroquad is 2D; kiss3d has release-note skinning and morph support only.
7. KAGRA (MIT) and A.R.I.A (MIT) have MToon or VRM support but are not proven light.
8. A minimal wgpu renderer is an estimated 1,700 to 2,400 lines and 4 to 7 weeks (UNPROVEN estimate).
9. Best cheap next step: fork bevy_vrm1 and measure the effect of `include_source: false` on memory.
10. Reddit and Warudo: NOT FOUND or blocked; no performance evidence from community sources.
