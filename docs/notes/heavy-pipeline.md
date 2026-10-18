# Heavy pipeline: tracker performance research

Research only. No edits to `crates/`, no cargo, no camera or benchmark runs. All ms figures that are not user-measured are estimates and marked UNVERIFIED.

Sources used: repo files (file:line), docs.rs (nokhwa `Buffer`, ort `SessionBuilder`), and one web search result (Microsoft Learn, SetThreadPriority). context7 was not available in this session (ToolSearch returned no match), so Rust std and the `windows` crate were not checked through context7. The docs.rs page for `windows::Win32::System::Threading::SetThreadPriority` returned 404, so the Rust signature is UNVERIFIED.

Measured numbers (from the task brief, not re-measured here): tracker 17.2 ms median (landmark 12.2, blendshape 2.2, detector 4.1 on 29 of 142 frames, fit 0.2); about 12 ms alone; ready to applied 8.2 ms median, p95 16.6 ms; camera to ready 17.3 ms median, p95 26 ms; preview build 0.8 ms.

## 1. Avoidable ms and CPU per frame

Ranked by estimated cost. Only the first two are worth touching.

| Where | What it costs | Estimate | Avoidable |
|---|---|---|---|
| `crates/facelogic/src/crop.rs:89-114` `warp_to_tensor` | 65,536 output px per tracked frame; `crop_to_frame` calls `sin_cos` per pixel (lines 108-114); `bilinear` returns `[f32;3]` and `.map(c / 255.0)` per pixel (line 102); output Vec alloc (line 89-ish) | 0.3-1.0 ms, UNVERIFIED | Most of the per-pixel `sin_cos` is redundant: the rotation is constant per crop. Hoist it. |
| `crates/infer/src/landmarks.rs:110` | `tensor.to_vec()` copies 196,608 f32 (786 KB) per frame | about 0.1-0.2 ms, UNVERIFIED | Yes, pass ownership of the Vec instead of copying. |
| `crates/infer/src/landmarks.rs:92-95`, `60-79` | output copies via `extract` and `decode_outputs` | small, UNVERIFIED | Minor. |
| `crates/app/src/bin/live.rs:92-117` `build_preview` | full RGBA `vec![255u8; w*h*4]` per call plus per-pixel `copy_from_slice`; runs in the hook on the inference thread | 0.8 ms (user measurement) | Yes. Move to the render side, or reuse one buffer. |
| `crates/infer/src/detector.rs:93` and `82-124` | 128x128x3 buffer and per-pixel bilinear, on 29 of 142 frames; 4.1 ms detect cost, about 0.84 ms per frame amortized | 4.1 ms per detect frame (user measurement) | Only by changing the schedule (see change 6 below). Not recommended without an accuracy check. |
| `crates/infer/src/blendshapes.rs:76-87` `pick_subset` | `Vec::with_capacity(292)` per call | under 0.05 ms | Negligible. |
| `crates/capture/src/convert.rs:133-141` YUYV to RGB | per-pixel `extend_from_slice` over 307,200 px, new Vec per frame. Runs on the capture thread. | 0.5-1.5 ms, UNVERIFIED | Yes. Preallocate and write by index. It is not in the latency metric, because `arrived` is stamped after conversion (`crates/capture/src/thread.rs:90` then `:97`). |
| `crates/capture/src/thread.rs:160-163` | `buffer.buffer().to_vec()` copies 614,400 B (640x480 YUYV) per frame | about 0.1 ms, UNVERIFIED | Possible: nokhwa `Buffer::buffer()` returns a borrowed `&[u8]` (docs.rs, see sources). Converting from the borrow avoids the copy, but it changes the `FrameSource` trait shape. Low priority. |

Total avoidable CPU on the inference thread: roughly 1 to 2 ms per frame (UNVERIFIED sum of the estimates above). Landmark inference (12.2 ms) is the bulk of the cost and is not avoidable by code changes in this pipeline.

### The 8.2 ms "tracker ready to applied" wait

This is not CPU. It is a render-poll wait.

- `crates/avatar/src/window.rs:145-150`: `apply_tracking` does `slot.0.try_lock()` and `take()` once per Bevy Update.
- `window.rs:403`: `apply_tracking, smooth_targets` run in `Update`, so the consumer reads once per render frame.
- At 60 fps the expected wait is half a frame, 8.3 ms. That matches the measured 8.2 ms median, so the wait is explained by the poll rate.

Reducing it needs the render frame rate or the apply point to change, not a code fix in the tracker. Options (UNVERIFIED that any of them is available in this Bevy version): higher present rate, or running the apply step earlier in the frame. The `smooth_targets` 30 ms time constant (`window.rs:164-173`, `tasks.md` 6.3) adds on-screen lag on top of this and is a product choice, not a bug.

## 2. The 17 ms vs 12 ms gap and thread priority or affinity

Hypothesis (UNVERIFIED, not measured): when Bevy is running, the inference thread and ORT workers lose CPU to Bevy's render and task threads. The contention source is plausible but not proven. Two more facts constrain the fix:

- ORT's intra-op pool is separate from our thread. `SetThreadPriority` on the inference thread does not change ORT's worker threads. ORT's `with_independent_thread_pool` documents its own pool (docs.rs ort `SessionBuilder`, see sources). Whether ORT worker threads can be given a priority is UNVERIFIED.
- ORT spinning is on by default: `with_intra_op_spinning` and `with_inter_op_spinning` both "enabled by default" (docs.rs ort `SessionBuilder`). Spinning burns CPU while waiting, which competes with Bevy. Turning it off is a plausible fix, but its effect is UNVERIFIED.

Safe options:

1. Raise the inference thread priority. Call once at the top of the thread closure in `crates/bench/src/pipeline.rs:91` (inside `spawn_inference_hooked`), or in the live entry point:
   ```rust
   // windows crate; exact module path and feature flags UNVERIFIED
   unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL) };
   ```
   Microsoft Learn describes `THREAD_PRIORITY_ABOVE_NORMAL` as one point above the process priority class and says a nonzero return means success (Microsoft Learn `SetThreadPriority`, see sources, from a search snippet, not a full page fetch). Do not use `THREAD_PRIORITY_HIGHEST` or `TIME_CRITICAL`: they can starve the render thread. The render side uses `try_lock` (`window.rs:145`), so a brief inference-side lock does not block it.
2. Affinity: do not change it. `SetThreadAffinityMask` pins the thread to cores and can make things worse on hybrid CPUs. Core count and CPU model were not checked (UNVERIFIED).
3. ORT spinning off, for the landmark session: `SessionBuilder::with_intra_op_spinning(false)`. Its effect is UNVERIFIED and needs a benchmark by the owner of the measurement agents.

## 3. Does the slot drop stale frames, and is the live hook a risk?

Yes. `crates/capture/src/slot.rs:25-31` `put` overwrites the single slot and counts drops; `wait_take` (`slot.rs:39-45`) returns the newest. The tests (`slot.rs:73-110`) confirm newest-frame semantics. Capture-side latency is therefore bounded: at most one in-flight inference plus one frame age. It cannot grow without limit.

The live hook is a bounded risk, not an unbounded one. `crates/app/src/bin/live.rs:209-226` passes the hook to `spawn_inference_hooked`. The hook runs after `publish` (`crates/bench/src/pipeline.rs:116-118`), so its time delays the next `wait_take`. If a frame arrives during `build_preview` (0.8 ms), that frame's `ready - arrived` includes the hook time. Effect: up to about 0.8 ms of added latency per frame. It does not compound, because the next frame's wait is still bounded by the slot. Risk: low.

## 4. Ordered change list (max 6)

1. **`crates/facelogic/src/crop.rs` `warp_to_tensor` (lines 89-114)**: compute the rotation's `sin` and `cos` once per crop, outside the per-pixel loop, and pass them to `crop_to_frame`. Expected: about 0.3-1.0 ms per tracked frame (UNVERIFIED). Risk: low; add a test that the warped tensor is byte-identical before and after.
2. **`crates/app/src/bin/live.rs` `build_preview` (lines 92-117) and the hook at 209-226**: reuse one RGBA buffer across frames (or build the preview on the render side). Expected: removes up to 0.8 ms from the inference thread and from the next frame's latency. Risk: low.
3. **`crates/infer/src/landmarks.rs:110` `run`**: change `Tensor::from_array((shape, tensor.to_vec()))` to take an owned `Vec<f32>` so the 786 KB copy is avoided. Expected: about 0.1-0.2 ms per frame (UNVERIFIED). Risk: low; a signature change at the call site in `tracker.rs`.
4. **`crates/capture/src/convert.rs:133-141` YUYV path, and `crates/capture/src/thread.rs:90`**: write into a preallocated `Vec` by index instead of per-pixel `extend_from_slice`. Expected: about 0.5-1.5 ms on the capture thread (UNVERIFIED), which reduces contention with Bevy, not the latency metric. Risk: low; the existing YUYV and NV12 tests in `convert.rs:200-212` cover it.
5. **`crates/bench/src/pipeline.rs:91` (thread closure in `spawn_inference_hooked`)**: set the inference thread to `THREAD_PRIORITY_ABOVE_NORMAL` with `SetThreadPriority(GetCurrentThread(), ...)`. Expected: less preemption by Bevy threads; the 17 vs 12 ms effect is UNVERIFIED. Risk: low at ABOVE_NORMAL; do not go higher. Needs the `windows` crate with the Threading feature (UNVERIFIED signature).
6. **`crates/infer/src/landmarks.rs` session builder (ORT `SessionBuilder`)**: test `with_intra_op_spinning(false)` against the default. Expected: less CPU burned while waiting, possibly closing part of the 5 ms gap (UNVERIFIED). Risk: possibly slightly higher per-frame latency when idle. Requires a benchmark by the owner of the measurement agents, not by this research task.

Not in the list, deliberately: detector schedule changes (`crates/facelogic/src/schedule.rs:14, 26-38`). Forced re-detects happen on 29 of 142 frames, and loosening them risks losing the face. Also not in the list: the render poll (8.2 ms), which is a render-side change and is covered in section 1.

## 5. Summary

1. Inference-side avoidable CPU is small: about 1 to 2 ms per frame (UNVERIFIED sum).
2. Biggest single hotspot: `crop.rs` `warp_to_tensor` per-pixel `sin_cos`, about 0.3-1.0 ms (UNVERIFIED).
3. `build_preview` costs 0.8 ms (measured) on the inference thread and adds to the next frame's latency.
4. The 8.2 ms ready-to-applied wait is the render poll (`window.rs:145`, Update schedule), about half a 60 fps frame. It is not CPU.
5. The 17 vs 12 ms gap is probably CPU contention with Bevy (UNVERIFIED). ORT spinning is on by default.
6. Safe priority change: `SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL)` on the inference thread. Do not use higher priority. Do not set affinity.
7. ORT worker threads are not covered by the thread priority call (UNVERIFIED how to set them).
8. Slot is newest-frame with drop counting (`slot.rs:25-45`); latency is bounded and cannot grow.
9. The live hook adds up to 0.8 ms to the next frame's latency. Low risk.
10. Top 6 changes listed in section 4. Nothing edited, nothing run.

## Sources

- Repo: `crates/facelogic/src/crop.rs`, `crates/infer/src/landmarks.rs`, `crates/infer/src/detector.rs`, `crates/infer/src/blendshapes.rs`, `crates/infer/src/tracker.rs`, `crates/facelogic/src/schedule.rs`, `crates/app/src/bin/live.rs`, `crates/bench/src/pipeline.rs`, `crates/capture/src/slot.rs`, `crates/capture/src/thread.rs`, `crates/capture/src/convert.rs`, `crates/avatar/src/window.rs`, `openspec/changes/fix-head-fit-and-expressions/tasks.md` and `design.md`.
- nokhwa `Buffer` (borrowed `buffer()` and owned `buffer_bytes()`): https://docs.rs/nokhwa/latest/nokhwa/struct.Buffer.html
- ort `SessionBuilder` (`with_intra_threads`, `with_inter_threads`, `with_intra_op_spinning` and `with_inter_op_spinning` enabled by default, `with_independent_thread_pool`): https://docs.rs/ort/latest/ort/session/builder/struct.SessionBuilder.html
- Win32 SetThreadPriority (search snippet only): https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setthreadpriority (and the `tr-tr` mirror in the search result)
- Not checked: context7 (unavailable), windows-rs docs (404), Rust std docs.

Background agent `ac3e68bf9c7bf4e58` (ORT CPU/memory) finished with an empty output file, so it adds nothing here. Background agent `ab4593c9d87bbc968` (Bevy render CPU/memory) was still running when this was written and its results are NOT incorporated.
