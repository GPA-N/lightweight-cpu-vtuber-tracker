# 06 Stop reallocating the preview texture buffer

Read `00-rules.md` first. File: `crates/avatar/src/window.rs`, function `update_preview` (search `fn update_preview`).

Problem: `update_preview` does `image.data = Some(p.rgba)`, which drops the previous 1.2 MB Vec and keeps the freshly allocated one, about 15 times a second (about 18 MB per second of allocator churn on the render thread).

Change (exactly this): copy into the existing buffer when it already has the right length:
```rust
if image.width() == p.width && image.height() == p.height {
    match image.data.as_mut() {
        Some(buf) if buf.len() == p.rgba.len() => buf.copy_from_slice(&p.rgba),
        _ => image.data = Some(p.rgba),
    }
}
```
Keep the timing code that pushes `upload_ms`. Do not change `crates/app/src/bin/live.rs`.

Measure: the report line `preview texture upload (render thread)` (median and p95) in `live_report.txt`, two live runs before (tags `s06_before_1`, `_2`) and two after (`s06_after_1`, `_2`), plus peak private bytes and CPU average.

KEEP rule: KEEP if tests and clippy pass and the AFTER upload median is not higher than the BEFORE upload median by more than 0.05 ms and the camera panel still updates (open `AVATAR_SHOT` screenshot as in subtask 03 and check the preview shows a camera picture, not grey). The change is a tidy-up; do not expect a visible CPU change. If upload time gets worse, REVERT.
