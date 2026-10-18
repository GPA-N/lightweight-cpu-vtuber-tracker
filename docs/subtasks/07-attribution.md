# 07 Find where the window's CPU and memory go (single-variable attribution)

Read `00-rules.md` first (including the Debate protocol and the no-face note). This subtask is measurement only. Temporary env switches go in, the numbers come out, then the switches are removed.

Known: camera + tracker without a window (`bench --seconds 15`) is 12.6 to 14.4 percent CPU and 55 MB. The full `live.exe` with no face is 85 to 93 percent CPU and 457 to 532 MB. So the window costs about 75 points and 400 MB. We need to know which part: the avatar (VRM, MToon shaders, spring bones), the preview panel, or the bare Bevy window and render loop.

Edit `crates/avatar/src/window.rs`, `build_app`, in the Startup closure (search `live_layout` and `VrmHandle`). Add three temporary switches read with `std::env::var_os`:
- `AVATAR_NO_VRM`: skip the line `commands.spawn((VrmHandle(...), Transform::from_xyz(avatar_x, 0.0, 0.0)));`
- `AVATAR_NO_PREVIEW`: skip the whole `if live_layout { ... }` block that creates the preview image, material and Rectangle mesh (the resources the other systems read, `PreviewImage`, must then not be required: guard `update_preview` with `Option<Res<PreviewImage>>` and return early when it is None).
- Because `live_clock` waits for the VRM (`ExpressionEntityMap`) before it starts counting and exits, with `AVATAR_NO_VRM` the window would never close: when `AVATAR_NO_VRM` is set make `live_clock` start its clock immediately (treat the model as ready at once).
Keep every change small. These switches are removed at the end (restore the file to its state before you started; `fixtures\private\out` keeps your numbers).

Variants to measure, each TWICE, 15 s live runs with NO face, alternating the order (A, B, C, D, A, B, C, D) and writing each run to its own `--out fixtures\private\out\live_s07_<variant>_<n>`:
- A: no switches (full app)
- B: `AVATAR_NO_PREVIEW=1`
- C: `AVATAR_NO_VRM=1`
- D: `AVATAR_NO_VRM=1` and `AVATAR_NO_PREVIEW=1` (bare Bevy window with a camera, a light and nothing else)
Set the env var with `$env:AVATAR_NO_VRM = '1'` and clear it with `Remove-Item Env:AVATAR_NO_VRM` between variants. One build only (all four variants use the same binary). That is 8 live runs; the cap of 4 in the rules is raised to 8 for this subtask only.

Write a table in `docs/subtasks/debate/07.md` with CPU average, peak working set, peak private bytes and render fps for every run, and a one-paragraph conclusion: how many CPU points and MB the avatar costs (A minus B and C), the preview costs, and the bare window costs (D). Hypothesis and Prediction first (rule: write them before measuring), as numbers.

KEEP rule: not applicable. After the numbers are in, REMOVE the switches and restore `window.rs` exactly, rebuild once, run `cargo test --workspace -q` and clippy. Tick subtask 7 in `tasks.md` and add one summary row to `results.md`.
