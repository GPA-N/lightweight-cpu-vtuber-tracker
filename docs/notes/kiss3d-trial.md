# kiss3d 0.47 trial: Seed-san avatar

## Setup

- kiss3d 0.47.0, wgpu backend (default), loaded via `SceneNode3d::add_gltf` on `samples/Seed-san.vrm`.
- Spike: `fixtures/private/spikes/kiss3d-avatar` (own workspace, shared target dir `target/`).
- Lockfile pins `wesl`, `wesl-macros`, `wgsl-parse`, `wgsl-types` to 0.4.1 because 0.4.2+ need rustc 1.97.1 and this machine has 1.97.0.
- Camera (0, 1.35, 1.8) looking at (0, 1.2, 0), same framing as bevy-bare. Point light at (0, 2, 2). Background (0.2, 0.22, 0.26).
- Morph: VRM 1.0 `aa` read from the glb JSON chunk (node 2, morph index 25), driven as `0.5 - 0.5 cos(2 pi t)` (1 Hz, 0..1) on every object with more than 25 targets.
- Frame cap: sleep to 33.3 ms per frame (no built-in cap in kiss3d).
- Run: 15 s, exits itself. Screenshots at 5 s and 5.5 s.
- Bevy reference: `fixtures/private/spikes/bevy-bare`, `--seconds 15`, no other flags.

## First run (functional check)

- `aa binds node 2 morph index 25`
- `shot_5s.png` at w=0.001: mouth closed. `shot_5_5s.png` at w=0.987: mouth open. Morph works.
- Skin: the mesh draws with correct geometry and materials, but the pose is the bind pose (T-pose), so bone deformation is NOT shown. Skinning is unproven.
- Run result: 380 frames in 15.02 s, 25.3 fps, 3 morph objects touched per frame.

## Predictions (written before the 3-run measurement)

- kiss3d one-core CPU: 15 to 35 percent (threshold for pass: under 60 percent of Bevy).
- kiss3d peak working set: 150 to 250 MB (threshold: under 330 MB).
- kiss3d peak private: 200 to 350 MB.
- Run-to-run noise: under 10 percent of CPU.
- Bevy reference from lean-threads.md: 45 percent one core, 273 MB WS. Expect the alternating runs to match within noise.

## Measurements

Order K, B, K, B, K, B. Sampler: `fixtures/private/spikes/measure.ps1 -Exe <exe> -Seconds 15`. CPU one-core = process CPU time / wall time. Total-machine figure = one-core / 12 cores.

| Run | Exe | CPU % one core | CPU % total | Peak WS MB | Peak private MB |
|---|---|---|---|---|---|
| 1 | kiss3d-avatar | 13.7 | 1.1 | 214.8 | 1164.8 |
| 2 | bevy-bare | 53.7 | 4.5 | 254.6 | 400.5 |
| 3 | kiss3d-avatar | 14.0 | 1.2 | 214.9 | 1165.0 |
| 4 | bevy-bare | 53.4 | 4.5 | 265.3 | 410.8 |
| 5 | kiss3d-avatar | 13.1 | 1.1 | 214.6 | 1164.7 |
| 6 | bevy-bare | 49.5 | 4.1 | 255.3 | 401.1 |

Means: kiss3d 13.6 percent one core, 214.8 MB WS, 1165 MB private. Bevy 52.2 percent one core, 273.6 MB WS, 404 MB private.
Noise (max minus min): kiss3d 0.9 points CPU, 0.3 MB WS. Bevy 4.2 points CPU, 10.7 MB WS.
kiss3d CPU / Bevy CPU = 0.26 (under the 0.60 threshold). WS 215 MB (under 330 MB threshold).

Predictions vs measured:
- CPU 15 to 35 percent: measured 13.6. Below range, held as "low side" only (prediction missed by 1.4 points).
- WS 150 to 250 MB: measured 215. Held.
- Private 200 to 350 MB: measured 1165. MISSED by a wide margin. kiss3d private is about 3x Bevy private. Not explained yet.
- Noise under 10 percent: held.
- Bevy 45 percent, 273 MB: measured 52 percent, 274 MB. CPU is higher than the older note; WS held.

Frame rate: the spike logged 380 frames in 15.02 s, 25.3 fps, even with a 33.3 ms sleep cap. The CPU is low (about 14 percent), so the frame loop is probably waiting on present or vsync. Not confirmed.

## Gap list

Evidence is from the kiss3d 0.47.0 source in the cargo registry and from the spike run. "Not checked" means no test was run.

1. VRM expression binds: kiss3d has no VRMC_vrm parser (`grep VRMC` in src finds nothing). The spike reads `aa` by hand from the glb JSON and maps it to node 2 index 25. Morph verified visually (shot_5s w=0.001 closed, shot_5_5s w=0.987 open). Other presets (blink, happy, etc.) are not mapped.
2. Skinning: shown by the head-yaw test (see "Skin test result"). Limit: the head is a one-joint skin, so only the head deforms. Multi-bone body deformation not tested.
3. MToon-like shading: no MToon or VRMC_materials_mtoon code in kiss3d. Materials go through its own default.wgsl PBR path. Not matched to MToon look (no toon ramp, no rim). Visually the screenshot is plain PBR shading.
4. Outlines: no outline pass found in src. Not supported.
5. Texture alpha: glTF alpha modes exist in default.wgsl (alpha_mode field, object_material.rs). Not checked on Seed-san's transparent materials.
6. Spring bones (VRMC_springBone, 9 springs, 28 joints): not implemented (no springbone code). Hair and tail stay static.
7. Look-at (VRMC_node_constraint on 23 nodes): not implemented. Not checked against the file.
8. Eye blink (morph indices 1 and 2 on node 2): not driven by the spike. Not checked.
9. Texture memory: 15 PNG textures, about 6.95 MB source. GPU memory not measured separately. Not checked.
10. Face-tracker feed per frame: the spike uses a sine. A real tracker would call set_morph each frame. Cost of set_morph: 3 objects per frame, rebuilt weights vector each frame. Not measured with a tracker.
11. Windows backend: wgpu default, Windows D3D12 by default. Backend not explicitly verified in this run.
12. Frame pacing: kiss3d has no built-in frame cap. Fixed: the sleep-after-render loop gave 25 to 28 fps; deadline pacing with a spin tail gives 30.0 fps (see "FPS cause").
13. Build: the wesl crates had to be pinned to 0.4.1 to build on rustc 1.97.0. A toolchain bump would lift this.

## Verdict

Against the thresholds:
- CPU under 60 percent of Bevy: PASS (26 percent of Bevy).
- Working set under 330 MB: PASS (215 MB).
- Morph correct: PASS for the `aa` morph (visual check).
- Skin correct: NOT SHOWN. Only the bind pose was rendered.
- Private memory: not a threshold, but 1165 MB is about 3x Bevy and needs an explanation before a switch.

Updated after the PM follow-up:
- Skin: PASS for the head joint (two-extreme screenshots, see "Skin test result").
- Morph: PASS.
- 30 fps cap: PASS after deadline pacing (30.0 empty, 29.9 with avatar).
- CPU under 60 percent of Bevy: PASS (14.9 percent one core, 0.29 of Bevy).
- Working set under 330 MB: PASS (228 MB).
- Private memory: still about 1180 MB, not a threshold. Cause located (see "Memory cause"). No cheap fix found.

Task 1.1 ticked. Task 2.1 stays unticked (PM verdict pending).

## Skin test result

- Shots at the two extremes: `shot_yaw_plus30.png` at t=0.50 s, yaw +30.0 deg, morph w=1.000. `shot_yaw_minus30.png` at t=1.51 s, yaw -30.0 deg, morph w=0.999.
- Visual check: the face turns the opposite way between the two. The face and hair shift, and the eye spacing narrows from about 35 px to 30 px. The neck and body do not move. The mouth is open in both (morph works during the yaw).
- Prediction vs measured: predicted yaw +30 at 0.5 s and -30 at 1.5 s. Measured +30.0 at 0.50 s and -30.0 at 1.51 s. Morph unaffected. Matches.
- First run fired a shot late (t=0.81 s, yaw +16.8 deg). Fixed by firing only within 0.5 deg of the extreme. Re-run gives +30.0 and -30.0.
- Limit: the head is a one-joint skin (skin 2 has joint [45]). This shows the joint-driven pose of the head, not a full multi-bone body. The check is visual, not numeric.

## Memory cause (PM follow-up)

In-process readings (K32GetProcessMemoryInfo) at each stage, same build. Private = commit.

| Stage | WS MB | Private MB |
|---|---|---|
| start | 8.3 | 2.3 |
| after Window::new | 160.8 | 852.0 |
| after add_gltf (avatar) | 173.0 (peak 256.2) | 866.7 |
| frame 60 (avatar) | 214.8 | 1164.7 |
| empty scene (SKIP_GLTF=1), after window | 161.1 | 851.8 |
| empty scene, frame 60 | 180.0 | 869.6 |

Findings:
- About 850 MB private is committed by window and wgpu init, before any model. An empty scene is 870 MB. This is kiss3d/wgpu/driver cost, not the model.
- `add_gltf` adds only about 15 MB private. The glTF load, including texture decode into CPU memory, does not show as large commit here.
- The first avatar frames add about 295 MB private (frame 60 minus after add_gltf). Likely GPU upload and material pipeline creation. Not isolated further.
- Backend check: `WGPU_BACKEND=dx12` and `vulkan` both give 851 MB after window (identical). Not verified that the Vulkan switch took effect.
- Not tested: a texture-less copy of the model (needs a rewritten glb). A smaller model was not tried.
- Cheap fix: none evident. No change applied, so the 3-run re-measure was not repeated for memory. Private is not a threshold, and WS is under 330 MB.

## FPS cause (PM follow-up)

- Old loop: `render_3d` runs before the frame timer, then sleep(33.3 ms). Each cycle is render time plus 33.3 ms, about 34.5 ms, so 27 to 29 fps.
- Timer resolution is not the cause: `timeBeginPeriod(1)` on the empty scene gives 29.0 fps, the same as the default (29.0 and 28.9). Rust's sleep already uses high-resolution waits here.
- Fix: deadline pacing. Each frame owns one 33.333 ms slot, sleep to 2 ms before the slot, then spin. Overruns resync.
- Result, empty scene: 30.0 fps (both timer settings). With avatar: 29.9 fps.

Re-measure with deadline pacing (avatar, default timer, measure.ps1 3 runs, labels A A A, 15 s):

| Run | CPU % one core | CPU % total | Peak WS MB | Peak private MB |
|---|---|---|---|---|
| 1 | 14.5 | 1.2 | 227.7 | 1178.4 |
| 2 | 14.9 | 1.2 | 228.0 | 1178.4 |
| 3 | 15.2 | 1.3 | 227.8 | 1178.1 |

Mean: 14.9 percent one core (was 13.6 before the pacing change), 228 MB WS (was 215), 1178 MB private (was 1165). The spin tail costs about 1.3 CPU points. Bevy reference (earlier, unchanged): 52.2 percent one core, 274 MB WS.

- kiss3d / Bevy CPU: 14.9 / 52.2 = 0.29 (under 0.60). PASS.
- Measurement tool note: `measure.ps1 -Runs` takes run labels (`A`/`B`), not a count. `-Runs 3` ran once.

## Skin test (PM follow-up)

Source finding (kiss3d 0.47.0):
- `build_skin_parts` (src/loader/gltf.rs:261) stores each glTF joint as a weak handle to the scene node built for that joint. Inverse bind matrices come from the glTF accessor.
- `Skin3d` (src/scene/object3d.rs:42) recomputes the palette every frame as `joint_world * inverse_bind`, from the propagated world transforms of those joint nodes. The skinned mesh's own node transform is ignored, as glTF requires.
- So kiss3d poses bones from the node tree. Rotating a joint node's local rotation (`SceneNode3d::set_rotation`, scene_node3d.rs:2976) should deform the skin.
- VRM 1.0 head bone: humanoid `head` = node 45 (parent `neck` = node 44, 12 children). Skin 2 (used by mesh node 2) has exactly one joint, node 45. So rotating node 45 should turn the head mesh.
- kiss3d does not keep glTF node names. The spike finds node 45 by its bind rotation (-0.08127573, 0, 0, 0.9966917) and 12 children, and keeps a handle.

Prediction (written before the change and the run):
- Yaw +30 deg at 0.5 s and -30 deg at 1.5 s (0.5 Hz, `30 sin(pi t)`), so the head turns about 60 deg between the two shots.
- `shot` at 0.5 s and 1.5 s should show the face turned to opposite sides, with the head mesh moving as one rigid piece. The neck and body stay put (only node 45 and its skin are posed).
- Morph `aa` keeps working during the yaw (the two effects are independent).
- A failed prediction would be: the two images look identical (no skinning), or the head moves without the face pivoting (wrong joint).
