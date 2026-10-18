# Avatar window render load: research notes

Scope: research only. No edits to `crates/`, no cargo, no camera or benchmark runs. Other agents are measuring.

Observed (from the task brief, not re-measured here): avatar window renders uncapped near 60 fps (vsync), about 200% of one core, peak 904 MB private bytes (519 MB working set), against about 60 MB for tracking alone. Avatar updates arrive at about 15 Hz.

Citation conventions. `R` is `C:\Users\Lenovo\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\`. Bevy paths below are relative to `R`, for example `bevy_winit-0.19.1/src/winit_config.rs:127`. Project paths are relative to the repo root.

Context7 was unavailable in this session (ToolSearch returned no matching deferred tools). Bevy API claims rest on the 0.19.1 registry sources. Optional WebFetch or WebSearch checks against docs.rs and the migration guides were not run, so any claim that depends on documentation rather than source is tagged UNVERIFIED.

## Q1. Cap the render loop at 30 fps

Facts.
- `WinitSettings` is a resource with `focused_mode` and `unfocused_mode` (`bevy_winit-0.19.1/src/winit_config.rs:6`). `Default` returns `game()` (lines 72-76). `game()` sets the focused mode to `Continuous` and the unfocused mode to `reactive_low_power(1/60 s)` (lines 18-23).
- `Continuous` runs the update loop as fast as the presentation allows (`winit_config.rs:54-59`, `84-107`). The source documents that this is independent of vsync (`winit_config.rs:79-82`).
- `WinitSettings::reactive_low_power(wait: Duration)` is at `winit_config.rs:127-133`.
- Present mode defaults to `AutoVsync`, which falls back to `FifoRelaxed` and then `Fifo` (`bevy_window-0.19.1/src/window.rs:1219-1224`; `Window.present_mode` at line 166; `Window::default` at 480-492). The crate sets neither `WinitSettings` nor `PresentMode` (grep of `crates/` found no `WinitSettings`, `PresentMode`, `Msaa`, shadow, bloom or tonemapping setting).
- `WinitPlugin` calls `init_resource::<WinitSettings>()` (`bevy_winit-0.19.1/src/lib.rs:131`). `init_resource` does not overwrite a resource that was already inserted (standard Bevy behavior, UNVERIFIED in this session).
- The crate's `build_app` (`crates/avatar/src/window.rs:381-472`) adds `DefaultPlugins` at 390-397 and has no `WinitSettings` call.

Conclusion. The loop is unbounded in principle and paced by the `Fifo` vsync present, so it runs at the display rate (about 60 fps here). Nothing in the crate asks for less.

Exact change (not applied):
```rust
// in build_app, before app.run(), e.g. next to the other app.add_plugins calls (window.rs:398)
app.insert_resource(bevy::winit::WinitSettings::reactive_low_power(
    std::time::Duration::from_secs_f64(1.0 / 30.0),
));
```
Check that `bevy::winit` re-exports `WinitSettings` in this feature set (UNVERIFIED; `bevy_winit` is in `default_platform`, `bevy-0.19.1/Cargo.toml:2762-2769`, which `3d` pulls in).

Caveats.
- `reactive_low_power` is a wait timeout used when no events arrive. Whether it yields a steady 30 fps on Windows, and whether the animated avatar keeps waking, is UNVERIFIED. A test run is needed.
- Read the body at `winit_config.rs:127-133` to confirm what the unfocused mode is set to before relying on background behavior.
- `PresentMode` changes do not lower the loop rate. Switching to `AutoNoVsync` would raise it.
- Avatar motion at 30 fps with 15 Hz input relies on `smooth_targets` (`window.rs:164-186`, 30 ms follow). That is a reasonable match.

## Q2. Which DefaultPlugins features can go for one window plus one VRM

Facts.
- `DefaultPlugins` is built in `bevy_internal-0.19.1/src/default_plugins.rs:1-106`. The `3d` feature pulls `default_platform`, `3d_bevy_render` and `3d_api` (`bevy-0.19.1/Cargo.toml:2605-2611`). `default_platform` is at 2762-2769 and includes `bevy_gilrs`, `bevy_winit`, `bevy_clipboard`, `default_font`, `multi_threaded` and `webgl2`. `3d_bevy_render` (2623-2633) includes `bevy_anti_alias`, `bevy_gltf`, `bevy_pbr`, `bevy_post_process` and `bevy_gizmos_render`. `3d_api` (2612-2622) includes `bevy_light`, `ktx2`, `smaa_luts`, `tonemapping_luts` and `zstd_rust`. `picking` is at 2826-2831 and `scene` at 2840-2843.
- Plugins are removed with `PluginGroup::disable::<T: Plugin>()` (`bevy_app-0.19.1/src/plugin_group.rs:501`).
- `bevy_audio` sits under the `audio` feature (`bevy-0.19.1/Cargo.toml:2645-2648`, `2662`). The `3d` list does not include it (UNVERIFIED that it is absent from this build, so the feature tree should be checked with `cargo tree` on the user's machine).

Candidates. The crate uses neither gizmos, picking, UI, audio nor gilrs.

| Item | Action | Expected savings | Status |
|---|---|---|---|
| `GizmoPlugin`, `GizmoRenderPlugin` | `.disable::<GizmoPlugin>()`, `.disable::<GizmoRenderPlugin>()` | Small CPU and pipeline work | UNVERIFIED |
| `DefaultPickingPlugins` | `.disable::<DefaultPickingPlugins>()` | Small | UNVERIFIED: it may be a `PluginGroup`, which may not satisfy `disable`'s `Plugin` bound (`plugin_group.rs:501`) |
| `GilrsPlugin`, `ClipboardPlugin` | `.disable::<...>()` | Negligible | UNVERIFIED |
| `AudioPlugin` | none needed if not compiled | none | UNVERIFIED |
| `bevy_anti_alias`, `bevy_post_process`, `smaa_luts`, `tonemapping_luts` | Cargo feature change | Binary size, some GPU pipelines | UNVERIFIED; the crate adds no AA or tonemapping, so these are likely not active |
| `Msaa` on the camera | `Msaa::Off` (enum at `bevy_render-0.19.1/src/view/mod.rs:240-245`, default `Sample4`) | See below | Component use UNVERIFIED for 0.19 |

Do not disable `AnimationPlugin` or `TextPlugin` without checking, since `bevy_vrm1` may depend on animation (UNVERIFIED).

MSAA estimate (arithmetic, not measured). At 4 samples, a color target costs about 4 times its single-sample size, and depth costs the same again. For a 1280x720 window with an 8-byte-per-pixel HDR color format, the color target is about 29.5 MB and the depth target about 14.7 MB. The window size and format are UNVERIFIED. This is a VRAM cost. It is unlikely to be a large share of the 450 MB CPU-side difference.

Shadows are off by default (`bevy_light-0.19.1/src/directional_light.rs:147-159`). `DirectionalLightShadowMap` is 2048 (lines 202-205) and is inactive unless shadows are enabled. Camera3d requires only `Camera` and `Projection` (`bevy_camera-0.19.1/src/components.rs:20-40`), so bloom and tonemapping are not added.

Expected total savings from plugin removal alone are small. The biggest levers are in Q3.

## Q3. What makes up the roughly 450 MB over baseline

Facts.
- Preview path. `update_preview` (`crates/avatar/src/window.rs:319-330`) takes a `Vec<u8>` RGBA frame from `LiveRes.preview` (`crates/avatar/src/live.rs:13`) and assigns it to the 640x480 `Image` (`image.data = Some(p.rgba)`). The frame size is 640 x 480 x 4 = 1,228,800 bytes, about 1.2 MB. At about 15 Hz that is about 18 MB/s of allocation churn. Each new `Vec` replaces the old one, so steady-state retention is about 1.2 MB, but allocator fragmentation can push private bytes up over a long run (UNVERIFIED).
- The preview image is created with `RenderAssetUsages::default()` (`window.rs:435-446`), which keeps the CPU copy (`bevy_asset-0.19.1/src/render_asset.rs:7-44`; `bevy_image-0.19.1/src/image.rs:643`).
- VRM loading. `VrmLoader::load` uses `GltfLoaderSettings { include_source: true, ..default() }` (`bevy_vrm1-0.10.0/src/vrm/loader.rs:109`). The `GltfLoaderSettings` struct is at `bevy_gltf-0.19.1/src/loader/mod.rs:185`. Its `Default` is at 220-226: `load_meshes` and `load_materials` are `RenderAssetUsages::default()` (lines 223-224), which is MAIN|RENDER and keeps CPU copies. `include_source` defaults to false (line 228) and is set true by the VRM loader.
- Texture bytes. Images are built with `Image::from_buffer(..., settings.load_materials)` (`loader/mod.rs:1205-1212`), so every texture keeps its CPU copy. Path-based images are at line 1245.
- Source JSON. With `include_source`, the document is retained (`loader/mod.rs:1153`; `bevy_gltf-0.19.1/src/assets.rs:18` defines `Gltf`).
- Binary blob. The loader reads `gltf.blob` (`loader/mod.rs:1937-1938`). Whether the `Gltf` asset keeps the blob after load is UNVERIFIED.
- Texture handles are kept in `VrmAsset { gltf, images: Vec<Handle<Image>> }` (`bevy_vrm1-0.10.0/src/vrm/loader.rs`, around lines 89-93). Images are obtained with `load_context.get_label_handle(GltfAssetLabel::Texture(tex.index()))`. Format support comes from `CompressedImageFormatSupport` and `DefaultGltfImageSampler` (lines 29-38).
- Source dependencies. `bevy_vrm1` reads `gltf.source` in `src/vrm/gltf/extensions.rs:54`, `src/vrm/mtoon.rs:90` and `src/vrm/node_constraint/registry.rs:18` (let-else, so a `None` source is tolerated there). `src/vrm/initialize.rs:156` and `:169` call `gltf.source.as_ref().unwrap()` in a function that writes `./develop/{name}.json`. Whether that function is reachable in normal runs is UNVERIFIED.

What persists in CPU memory.
1. Texture pixel data for every VRM texture, retained because `load_materials` is MAIN|RENDER (`loader/mod.rs:223-224, 1205-1212`).
2. Mesh vertex and index data, retained for the same reason via `load_meshes` (`loader/mod.rs:223`; UNVERIFIED whether meshes are built or kept under the same usage).
3. The glTF source JSON (`loader/mod.rs:1153`), probably small.
4. The per-frame RGBA preview buffer, about 1.2 MB retained plus churn (`window.rs:319-330`).

GPU-side resources (uploaded textures, vertex buffers, MSAA targets, render pipelines) are not CPU copies, but on Windows some driver-side allocations can show in process memory (UNVERIFIED).

Attribution of the 450 MB is UNVERIFIED. It needs a measurement that varies one thing at a time: with the preview disabled, with `load_materials` set to render-world-only (not possible without changing `bevy_vrm1`'s settings, see Q5 item 6), and with Msaa off. The other agents own this measurement.

## Q4. Why `query_dxgi_memory` returns 0.0 MB for both local and non-local

Facts.
- `query_dxgi_memory` (`crates/avatar/src/render_report.rs:93-106`) creates a factory with `CreateDXGIFactory1`, takes `EnumAdapters1(0)`, casts to `IDXGIAdapter3`, and queries node 0 for the LOCAL and NON_LOCAL groups (`query_segment` at 85-89). Any failure returns `None` through `?`.
- A `None` result would print `n/a` (`crates/timing/src/report.rs:76-80`), not `0.0`. The app sets the `dxgi` fields only when the summary is `Some` (`crates/app/src/main.rs:127-129`, and `crates/app/src/bin/live.rs:327-329`). So the observed `0.0 MB` means the query succeeded and returned `CurrentUsage` = 0 for both groups on adapter 0. Budget values are not visible in the output.
- The avatar window queries memory inside `run()` after `app.run()` returns, while `app` is still in scope (`crates/avatar/src/window.rs:476-514`, `512`).
- Bevy's default power preference is `HighPerformance` (`bevy_render-0.19.1/src/settings.rs:86-87`), so wgpu prefers the discrete GPU on hybrid laptops.
- The `CurrentUsage` field of `DXGI_QUERY_VIDEO_MEMORY_INFO` is per-process usage on the queried adapter (Microsoft DXGI documentation, UNVERIFIED in this session because WebFetch was not run; URL to check: learn.microsoft.com, DXGI_QUERY_VIDEO_MEMORY_INFO).

Diagnosis (leading hypothesis, UNVERIFIED). DXGI adapter index 0 is not the adapter wgpu renders on. On a hybrid laptop it is likely the integrated GPU or the Microsoft Basic Render Driver, and it reports no usage by this process. The render device runs on the other adapter, so the query reads the wrong one.

Second possibility (UNVERIFIED): the wgpu device or render resources are torn down before the query, so usage is already 0 even on the right adapter. The source does not show teardown timing, and `app` being in scope does not prove the GPU resources are still alive.

Fix (sketch, not compiled; cargo is not permitted here). Enumerate all adapters, log their names, and query the one that matches the render adapter. Also query while the app is still running, for example in a system, so the reading is taken while resources are alive.

```rust
pub fn query_dxgi_memory_for(vendor_id: u32, device_id: u32) -> Option<DxgiMemory> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    let mut i = 0u32;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(i) } {
        i += 1;
        let mut desc = DXGI_ADAPTER_DESC1::default();
        unsafe { adapter.GetDesc1(&mut desc) }.ok()?;
        if desc.VendorId != vendor_id || desc.DeviceId != device_id {
            continue;
        }
        let adapter: IDXGIAdapter3 = adapter.cast().ok()?;
        let (local_mb, local_budget_mb) = query_segment(&adapter, DXGI_MEMORY_SEGMENT_GROUP_LOCAL)?;
        let (non_local_mb, non_local_budget_mb) =
            query_segment(&adapter, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL)?;
        return Some(DxgiMemory { local_mb, non_local_mb, local_budget_mb, non_local_budget_mb });
    }
    None
}
```
The vendor and device IDs must come from the adapter that wgpu selected. Bevy exposes adapter info through its render resources (name only is confirmed for this session? UNVERIFIED; the name is the safer key). Matching on `AdapterLuid` is more robust if it can be obtained. The `Win32_Graphics_Dxgi` feature is already enabled in `crates/avatar/Cargo.toml`. Note that `IDXGIAdapter1::GetDesc1` is available through the `IDXGIAdapter3` chain (UNVERIFIED for the `windows` 0.62 signatures).

Test impact. The existing `real_dxgi_query_does_not_panic_and_values_are_sane` test (`render_report.rs:173-183`) only asserts that values are finite and non-negative, so zeros pass it and it does not catch this. It should also assert that the matched adapter name equals the wgpu adapter name.

## Q5. Ordered changes (at most 6)

1. File `crates/avatar/src/window.rs`, function `build_app` (before `app.run()`, near 398). Code: `app.insert_resource(bevy::winit::WinitSettings::reactive_low_power(Duration::from_secs_f64(1.0 / 30.0)))`. Effect: loop drops from about 60 to about 30 fps, so render CPU should roughly halve (estimate, UNVERIFIED). Risk: wake cadence on Windows is UNVERIFIED; check the unfocused path and the animation smoothness. Verify with a user-run measurement, not here.
2. File `crates/avatar/src/render_report.rs`, function `query_dxgi_memory` (new `query_dxgi_memory_for`), plus the call at `window.rs:512`. Code: loop `EnumAdapters1`, match `DXGI_ADAPTER_DESC1` against the wgpu adapter, query there, and read it while the app runs. Effect: real VRAM numbers, needed to attribute the 450 MB. Risk: low (measurement only); the ID matching must be correct or it reports the wrong card again.
3. File `crates/avatar/src/window.rs`, function `update_preview` (319-330), and `crates/avatar/src/live.rs:13`. Code: copy the frame into the existing `image.data` buffer with `copy_from_slice`, instead of replacing the `Vec`. Effect: removes about 18 MB/s of allocation churn and allocator fragmentation pressure. Risk: low; `get_mut` still marks the asset changed, so the preview redraws. Check that the copy length matches.
4. File `crates/avatar/src/window.rs`, function `build_app` or the camera spawn at 425 (`Camera3d::default()`). Code: add `Msaa::Off` to the camera entity (component use UNVERIFIED for 0.19). Effect: removes 4x color and depth targets; estimate tens of MB of VRAM. Risk: avatar edges alias; compare screenshots.
5. File `crates/avatar/src/window.rs`, function `build_app` (390-397). Code: `.disable::<GizmoPlugin>()`, `.disable::<GizmoRenderPlugin>()`, `.disable::<GilrsPlugin>()`, `.disable::<ClipboardPlugin>()`, and `DefaultPickingPlugins` if it is a `Plugin`. Effect: small CPU and startup savings, UNVERIFIED. Risk: compile errors on group types; the user must confirm the build, since cargo is not run here.
6. File `bevy_vrm1-0.10.0/src/vrm/loader.rs:109` (in the registry, not `crates/`). Code: retain CPU copies only where needed, meaning `load_materials` RENDER_WORLD-only, and `include_source: false` if the `gltf.source` consumers are checked. Effect: potentially the largest CPU-side saving, UNVERIFIED in size. Risk: high. `bevy_vrm1` hardcodes these settings, so this needs a fork or a patched copy of the crate, and `initialize.rs:156,169` unwrap `source`. Do this last, and only after item 2 attributes the memory.

## Summary

1. The window renders uncapped because `WinitPlugin` uses `WinitSettings::game()` (Continuous focused), and present mode defaults to AutoVsync, which is `Fifo` at vsync (`winit_config.rs:18-23`, `window.rs:1219-1224`).
2. Cap to 30 fps with `WinitSettings::reactive_low_power(1/30 s)` inserted in `build_app` (`winit_config.rs:127`). Its effect is UNVERIFIED until run.
3. DefaultPlugins removal: gizmos, picking, gilrs and clipboard are candidates (`plugin_group.rs:501`). The savings are small and UNVERIFIED.
4. MSAA defaults to Sample4 (`view/mod.rs:240-245`), which is roughly tens of MB of VRAM (estimate).
5. CPU copies persist: bevy_vrm1 uses `include_source: true` and `load_materials` MAIN|RENDER (`loader.rs:109`, `loader/mod.rs:223-224, 1205-1212`).
6. The preview allocates a new 1.2 MB RGBA `Vec` about 15 times a second (`window.rs:319-330`).
7. Attribution of the 450 MB is UNVERIFIED and needs a single-variable measurement.
8. The DXGI 0.0 result is `Some(0.0)` from adapter 0 (`render_report.rs:93-106`, `timing/report.rs:76-80`). The leading cause is adapter mismatch with wgpu's HighPerformance pick (`settings.rs:86-87`), UNVERIFIED.
9. The fix is to enumerate adapters, match the render adapter, and query while the app is running (sketch above).
10. Change order: cap, DXGI fix, preview buffer reuse, Msaa off, plugin disables, then VRM loader copies last.
