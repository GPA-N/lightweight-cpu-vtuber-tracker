# Task 2.1: bare wgpu window floor (lean-floor)

Machine: AMD Ryzen 5 7430U with Radeon Graphics (12 logical cores), integrated GPU, Vulkan, Windows 11, laptop plugged in.
Spike: fixtures/private/spikes/wgpu-floor (wgpu 30.0.1, winit 0.30.13, pollster 1). Measure: fixtures/private/spikes/measure.ps1.
Reference (bare Bevy window, from the user): 98 percent of one core, 314 MB peak working set, 446 MB peak private at 60 fps; about 70 percent at 30 fps.

## Hypothesis

- H1 Bevy overhead (scheduler, task pools, render graph) is the main cost. Then a bare wgpu window is far below Bevy's 98 percent.
- H2 wgpu plus the Vulkan driver is the main cost. Then a bare wgpu window is close to Bevy's 98 percent and at 30 fps near 70 percent, so any wgpu library has the same floor.
- H3 The GPU memory allocator and pipeline cache hold most of the memory. Then the bare wgpu window already uses a large share of the 314 MB, and private commit is dominated by driver/allocator allocations rather than by Bevy's ECS and asset stores.

## Prediction (numbers, before any build)

- Run A (--fifo, 60 fps vsync): CPU 5 to 15 percent of one core (range 2 to 25), peak working set 110 MB (range 70 to 180), peak private 140 MB (range 90 to 230).
- Run B (--fifo --cap30): CPU 2 to 8 percent of one core, memory within 10 percent of A.
- Verdict rule: if A is at or above 60 percent of one core, H2 is supported and H1 is rejected. If A is below 25 percent, H1 is supported (Bevy adds about 75 points). If the bare wgpu working set is above 200 MB, H3 gets support for memory.
- What would prove me wrong: A above 60 percent (the wgpu/Vulkan floor is the real cost), or a bare working set below 150 MB while Bevy sits at 314 MB (then the extra memory is Bevy's).

## Result

Build: `cargo build --release`, wgpu 30.0.1, winit 0.30.13, one process, CARGO_BUILD_JOBS=4, one rebuild after two wgpu 30 API fixes (`apply_limit_buckets`, `Queue::present`). Runs: 15 s each, sampled every 1 s with `fixtures/private/spikes/measure.ps1`. CPU one-core percent = TotalProcessorTime delta / wall time. Total = one-core / 12.

| Run | Mode | CPU % (one core) | CPU % (total, /12) | Peak WS MB | Peak private MB |
|---|---|---|---|---|---|
| 1 | A --fifo | 9.0 | 0.8 | 133.7 | 297.9 |
| 2 | B --fifo --cap30 | 6.1 | 0.5 | 107.6 | 282.1 |
| 3 | A --fifo | 11.1 | 0.9 | 116.0 | 290.4 |
| 4 | B --fifo --cap30 | 6.3 | 0.5 | 107.5 | 281.9 |
| 5 | A --fifo | 9.6 | 0.8 | 119.0 | 290.2 |
| 6 | B --fifo --cap30 | 5.5 | 0.5 | 110.8 | 281.8 |

Means: A = 9.9 % one core (0.8 total), peak WS 122.9 MB, peak private 292.8 MB. B = 6.0 % one core (0.5 total), peak WS 108.6 MB, peak private 281.9 MB.

Prediction check: CPU in range for A (9.9 vs 5 to 15) and B (6.0 vs 2 to 8). Working set in range for A (122.9 vs 70 to 180). Private missed: 292.8 MB vs predicted 140 (range 90 to 230), so the prediction was wrong. B vs A memory: WS differs by -11.6 percent (just outside the 10 percent bound), private by -3.7 percent.

Caveats: plugged-in state is the user's statement (battery query returned nothing). Display refresh rate not recorded, so Fifo fps is whatever the panel runs at. Noise is about 10 to 15 percent; the three A runs span 9.0 to 11.1.

## Verdict

- H1 (Bevy engine overhead is the main cost): supported. Bare wgpu uses 9.9 % of one core at vsync against Bevy's 98 %, so about 88 points of the Bevy CPU cost sit above the wgpu/Vulkan floor.
- H2 (wgpu plus Vulkan is the main cost): rejected for CPU. The floor is 9.9 %, far below the 60 % threshold.
- H3 (allocator and pipeline cache explain memory): not supported by the pre-set rule. Bare working set is 123 MB, below the 200 MB threshold. But bare private commit is 293 MB, which is 66 % of Bevy's 446 MB. Most of the private memory is present before any engine code, so the driver/GPU-side commit is the largest share. This spike does not separate allocator from driver, so the H3 split is unresolved.
- For choosing a library: any wgpu-based library starts near 10 % CPU and about 120 MB working set, so switching away from Bevy can remove most CPU cost. Memory savings are smaller: Bevy's extra private commit over the wgpu floor is about 150 MB, and the floor itself is about 290 MB.
