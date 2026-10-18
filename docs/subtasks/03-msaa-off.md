# 03 Turn MSAA off on the avatar camera

Read `00-rules.md` first. File: `crates/avatar/src/window.rs`, in `build_app`, the Startup closure line `commands.spawn((Camera3d::default(), camera));`.

Problem: Bevy 0.19 cameras default to `Msaa::Sample4` (4x multisampled render targets: more GPU memory, more GPU time, mapped memory in the process). A single anime avatar does not need it.

Change (exactly this): add the `Msaa::Off` component to that spawn:
`commands.spawn((Camera3d::default(), Msaa::Off, camera));`
First check the 0.19 name and import with ctx7: `npx -y ctx7@latest library bevy "Msaa component camera"` then `docs`. If `Msaa` is not in `bevy::prelude`, import it from the path ctx7 or `~/.cargo/registry/src/*/bevy_render-0.19*/src/view/mod.rs` shows. If the component needs a different form in 0.19 (for example a field on `Camera3d`), use what the source shows and say so in your report.

Measure: two live runs BEFORE (tags `s03_before_1`, `_2`; baseline numbers in 00-rules.md may be used as a third reference) and two AFTER (`s03_after_1`, `_2`). Compare peak working set, peak private, CPU average, render fps. Take a screenshot of the layout to check the avatar still looks fine: `$env:AVATAR_SHOT = 'fixtures\private\out\s03_shot.png'; .\target\release\live.exe --seconds 14 --out fixtures\private\out\s03_shot` then look at the png with the Read tool; jagged edges are acceptable, a broken or empty scene is not.

KEEP rule: KEEP if both AFTER runs have peak private bytes OR CPU average lower than both BEFORE runs by more than 10 percent, render fps stays at least 55, and the screenshot shows the avatar and the camera panel. Otherwise REVERT (the change is one token).
