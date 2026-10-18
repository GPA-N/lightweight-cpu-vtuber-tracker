# 04 Disable Bevy plugins the app does not use

Read `00-rules.md` first. File: `crates/avatar/src/window.rs`, `build_app`, the `app.add_plugins(DefaultPlugins.set(AssetPlugin{..}).set(WindowPlugin{..}))` call.

Problem: DefaultPlugins starts gizmo, gamepad (gilrs), clipboard and picking plugins that a one-window avatar viewer never uses; each has background work or state.

Change: after the two `.set(...)` calls, chain `.disable::<...>()` for each plugin type that exists in this Bevy 0.19 build and is part of DefaultPlugins with the features in `crates/avatar/Cargo.toml` (`default-features = false, features = ["3d"]`). The research candidates are `bevy::gizmos::GizmoPlugin`, `bevy::gizmos::GizmoRenderPlugin`? (check), `bevy::input::gilrs::GilrsPlugin`, `bevy::winit::...Clipboard`, `bevy::picking::DefaultPickingPlugins`. Many may not even be compiled in with this feature set. Method: for each candidate, grep the DefaultPlugins group source `~/.cargo/registry/src/*/bevy_internal-0.19*/src/default_plugins.rs` to see whether it is present under `#[cfg(feature = ...)]` for our features; only disable plugins that are really present. Check API with `npx -y ctx7@latest library bevy "PluginGroupBuilder disable"`. Report the list of plugins you found present and the list you disabled. If none are present, REVERT/skip, tick the task, and write `none present` in results.md.

Measure like subtask 03 (two live runs before: tags `s04_before_1`, `_2`; two after: `s04_after_1`, `_2`) and take the same screenshot check (`s04_shot`). The avatar and camera panel must still render and the window must still close after 15 s.

KEEP rule: KEEP if both AFTER runs beat both BEFORE runs on CPU average or peak private by more than 10 percent, render fps stays at least 55, and the screenshot is fine. Otherwise REVERT.
