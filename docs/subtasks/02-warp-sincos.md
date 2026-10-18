# 02 Hoist sin/cos out of the per-pixel crop loop

Read `00-rules.md` first. File: `crates/facelogic/src/crop.rs`, function `warp_to_tensor` (about line 89).

Problem: for each of the 256x256 output pixels `warp_to_tensor` calls `crop_to_frame`, and `crop_to_frame` calls `rect.angle.sin_cos()` every time (65,536 sin_cos calls per frame).

Change (exactly this): compute `let (s, c) = rect.angle.sin_cos();` once at the top of `warp_to_tensor` (after the assert), and replace the call `crop_to_frame(u as f32, v as f32, rect, out_size)` with the same math written inline using `s` and `c`:
```rust
let n = out_size as f32;
let (s, c) = rect.angle.sin_cos();
for v in 0..out_size {
    for u in 0..out_size {
        let lx = (u as f32 / n - 0.5) * rect.width;
        let ly = (v as f32 / n - 0.5) * rect.height;
        let x = rect.cx + c * lx - s * ly;
        let y = rect.cy + s * lx + c * ly;
        out.extend(bilinear(frame_rgb, wi, hi, x, y).map(|c| c / 255.0));
    }
}
```
(rename the closure variable so it does not shadow `c`). Leave `crop_to_frame` and `frame_to_crop` unchanged (they are used elsewhere and tested).

Measure (saturated, three runs before and three after, same session):
`$env:BENCH_REPEAT = 300; .\target\release\bench.exe --check-image fixtures\private\test10.png --models models | Select-String 'latency median|throughput'`
Take the median of the three runs for each side. Run the "before" three runs first, before editing.

Tests: add one unit test in `crop.rs` `mod tests`, named `warp_to_tensor_matches_crop_to_frame_math`, that builds a rotated `RotatedRect` (angle 0.3), a 64x48 gradient frame, calls `warp_to_tensor(.., out_size 16)`, and checks the first three values equal `bilinear(...)` at `crop_to_frame(0.0, 0.0, ..)` divided by 255 within 1e-6. Existing crop tests must still pass.

KEEP rule: KEEP if all tests pass, the accuracy guard (rule 5) is unchanged within 1e-4, and the after median latency is not higher than before by more than 0.2 ms. (The change strictly removes work, so a tie is fine.)
