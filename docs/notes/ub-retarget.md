# Upper body and hands retargeting to VRM humanoid bones

Research note, 2026-10-17. Scope: map MediaPipe Pose world landmarks (33) and hand landmarks (21 per hand) to VRM humanoid bone rotations, for any VRM model, in the Bevy 0.19 avatar app. Items marked UNVERIFIED were not checked against a primary source in this pass.

## Sources and dates

- kalidokit (JS reference solver). Code read from `src/PoseSolver/calcArms.ts`, `calcHips.ts`, `calcLegs.ts`, `src/HandSolver/index.ts` on the `main` branch of https://github.com/yeemachine/kalidokit, fetched via raw GitHub. npm `kalidokit` latest is 1.1.5; npm registry `modified` field is 2022-05-07. This is older than 2025, so treat it as possibly stale. No maintained successor was found in a 2026 search (search results were 2021-2023 pages). UNVERIFIED that no successor exists.
- bevy_vrm1 0.10.0 (crates.io, max version, depends on `bevy ^0.19`; crate index updated 2026-10-03). Retarget code read from `src/vrma/animation/bone_rotation.rs` in the local registry copy `bevy_vrm1-0.10.0`.
- three-vrm 3.5.5 (npm registry, package modified 2026-07-09). Used only as the VRM rig reference; the rig code was not read in this pass.
- MediaPipe Pose Landmarker (Google AI Edge docs, https://developers.google.com/edge/mediapipe/solutions/vision/pose_landmarker/web_js, undated in the search result). Says world landmarks are metric with the hip midpoint as origin, and landmarks 23/24 are the hips. No 2026 revision was confirmed. UNVERIFIED for 2026 changes.
- Project files read: `crates/avatar/src/window.rs`, `crates/avatar/src/expressions.rs`, `crates/avatar/src/map_apply.rs`, `crates/infer/src/tracker.rs`.

## Project baseline (what exists now)

- `crates/avatar/src/window.rs` line 89: `tracking_to_targets(out, map)` returns a head `Quat` and a blendshape map. Line 118: `SMOOTH_TAU_SECONDS = 0.03`. Lines 164-173: `smooth_targets` computes `follow_factor(dt, SMOOTH_TAU_SECONDS)` and slerps the head. Head only.
- `crates/avatar/src/expressions.rs`: `ExpressionTarget(HashMap<String,f32>)`, clamps 0..1, drives `SetExpressions` in PostUpdate before `VrmSystemSets::Expressions`. Face only.
- `crates/avatar/src/map_apply.rs`: `apply(table, weights)` sums weight times entry weight, then clamps. Face only.
- `crates/infer/src/tracker.rs`: `Tracker<D,L,B>::step(frame)` runs face detect, landmarks, blend, fit, filter. No pose or hand model yet.

Conclusion: the project has no body or hand pipeline. Everything below is a design for a new pipeline.

## bevy_vrm1 normalized bones (model independence)

`Transformation::new(src_rest, src_rest_g, dist_rest, dist_rest_g)` and `transform(src_pose)` in `bone_rotation.rs` (lines 74 and 88) implement:

```
normalized_local = src_rest_g * src_rest^-1 * src_pose * src_rest_g^-1
out              = dist_rest * dist_rest_g^-1 * normalized_local * dist_rest_g
```

Here `src_*` is the source animation (VRMA) rest and pose, `dist_*` is the target VRM rest, and `_g` is the global rest rotation. Rest is captured per bone from each model, so the same animation drives any VRM. This is the VRMA path. Our tracker can reuse the same formula: treat the MediaPipe-derived pose as `src_pose` and the target VRM rest as `dist_rest`. The source rest is a fixed reference pose we define (T or A, see below).

bevy_vrm1 also has a body tracking module, `src/vrm/body_tracking.rs`, with `bone_rotation` (line 164) and an additive gaze-style path (lines 205-227). It runs in PostUpdate in the GazeControl set. UNVERIFIED whether it fits our body data; not read in full.

## Rest pose: T versus A

- VRM 1.0 humanoid rest pose is the bind pose the model file defines. Many VRMs use T-pose, others A-pose. The retarget formula above removes the difference because it uses each model's captured rest.
- Use A-pose as the MediaPipe reference (arms about 30-45 degrees down) only if you define `src_rest` that way. Keep `src_rest` as a constant you choose, and document it.
- UNVERIFIED: the exact VRM 1.0 spec wording on rest pose; check the vrm.dev humanoid spec before relying on it.

## Per-chain math

Coordinate frames: MediaPipe world landmarks are metric, hip-midpoint origin, y down in the MediaPipe convention. Convert to Bevy (y up, right-handed). Mirror: webcam preview is mirrored, so the subject's right hand is image left. Decide one convention and keep it in one place (`set_mirror` exists in tracker.rs). Swap the left and right VRM bones when mirroring is on.

Quaternion order: rotations compose right to left in the parent frame. `q_local = q_parent^-1 * q_world`. Apply `Quat::from_rotation_arc(rest_dir, target_dir)` to get the swing, then optional twist about the bone axis.

### Limb-direction retargeting (chosen approach)

For a bone with a parent joint P and child joint C (both world landmarks), take the bone direction `d_src = normalize(C - P)`. Compute `q_swing = Quat::from_rotation_arc(d_rest, d_src)` in the source frame, then feed through the bevy_vrm1 formula. This is the pattern kalidokit uses via `Vector.findRotation` (its arm code).

### Analytic two-bone IK (alternative)

For upperArm and lowerArm, given shoulder S, elbow E, wrist W, with lengths L1 and L2: `d = |W - S|`, `cos(a) = (L1^2 + d^2 - L2^2) / (2 L1 d)`. This gives the elbow bend, which direction-only retargeting can miss when the elbow is hidden. Trade-off: IK needs bone lengths; direction retarget does not. Recommended: direction retarget, with IK only for the elbow pole when visibility is low.

### Spine, chest, upperChest (lean and twist)

- Lean: direction from hip midpoint to shoulder midpoint, in the hips frame. Split between spine, chest, upperChest by fixed weights (UNVERIFIED weights; start 0.3, 0.3, 0.4).
- Twist: yaw of the shoulder line relative to the hip line. kalidokit computes this with `rollPitchYaw` on lm3d 23/24 and 11/12, then clamps the wrap at +/-0.5 normalized (its `hips.rotation.y` fix).
- Clamp each segment to a human range, e.g. lean +/-30 degrees, twist +/-35 degrees (UNVERIFIED numbers; tune per model).

### Shoulders, upperArm, lowerArm, hand

- Shoulder: direction from spine top to shoulder landmark, small weight (0.3).
- upperArm: shoulder to elbow direction, direction retarget.
- lowerArm: elbow to wrist direction, direction retarget, clamp elbow flex to 0..-145 degrees (kalidokit clamps LowerArm z to [-2.14, 0] normalized; UNVERIFIED degrees conversion).
- hand: wrist direction from palm landmarks (index 0 wrist, 5 index MCP, 17 pinky MCP in the hand model). kalidokit uses `rollPitchYaw(palm[0], palm[1], palm[2])` and then `y -= 0.4` offset (its code). Clamp wrist.y to a range.

### Fingers (21 hand landmarks)

Each finger has three segments: proximal, intermediate, distal. For each joint, flex = angle between consecutive bone vectors (`angleBetween3DCoords` in kalidokit). Apply as z rotation on proximal, intermediate, distal bones with clamps. kalidokit dampens thumb rotation (proximal x 2.2, y 2.2, z 0.5). Hand landmarks are not in world metres; use them as relative directions only.

## Joint-limit clamping

Clamp after computing each Euler or swing angle, before smoothing. kalidokit uses clamp ranges in `rigArm`, `rigFingers`, `calcHips`. Keep limits in a table keyed by bone name, so every VRM uses the same table.

## Missing detection: hold, then ease to rest

- Per-joint visibility from Pose landmarks. If visibility < 0.5 (UNVERIFIED threshold), hold the last good target for 0.5 s.
- After the hold, ease toward rest with `follow_factor(dt, tau)` with tau around 0.4 s (UNVERIFIED).
- kalidokit instead multiplies the arm by 0 when offscreen (`lm3d[15].y > 0.1` or visibility < 0.23) and resets to a resting default. Hard reset causes jumps; prefer the ease.

## Smoothing

- Existing project: exponential smoothing, `SMOOTH_TAU_SECONDS = 0.03` for the head. Reuse it for hands (fast) and use a larger tau for the torso (UNVERIFIED values).
- One Euro filter: adaptive cutoff, lower lag on fast motion. Stronger candidate for hand landmarks, which jitter most. UNVERIFIED: not checked against the original paper in this pass (Casiez et al., CHI 2012).
- Recommendation: One Euro on landmarks, exponential on final bone quaternions.

## Mirror and left/right

Mirror in one place. If preview is mirrored, swap MediaPipe left and right landmark indices before computing chains, and set bone sides to match. kalidokit uses `invert = side === RIGHT ? 1 : -1` on each rig function.

## Monocular depth reliability

MediaPipe world z is estimated from a single camera. Treat z as noisy. Use x and y for chain directions where possible. Lower confidence on z-driven angles (elbow flex, arm forward). UNVERIFIED accuracy numbers; no measured error in this project.

## Hips and legs: yaw/roll rotation and sideways sway

The lower torso can rotate, not only sway.

Hips rotation (tracked):
- Roll (z): from the line between landmarks 23 and 24, in the hips frame. Clamp to +/-15 degrees (UNVERIFIED).
- Yaw (y): from the same line projected on the ground plane. Clamp to +/-20 degrees (UNVERIFIED). kalidokit fixes the -PI/PI wrap by checking `rotation.y > 0.5` and subtracting 2.
- Pitch (x): set to 0 (kalidokit does this too, "temp fix for inaccurate X axis").

Sideways sway (automatic, about 0.25 Hz):
- Frequency f = 0.25 Hz, so period T = 4 s.
- Hips translate x: `x(t) = A_x * sin(2*pi*f*t)`, A_x about 0.02 m (UNVERIFIED amplitude).
- Hips roll: `roll(t) = A_r * sin(2*pi*f*t)`, A_r about 2 degrees (UNVERIFIED).
- Phase: start at t = 0 when tracking begins, or lock to the body-lean phase if available.

Composition:

```
q_sway   = Quat::from_rotation_z(roll(t))
q_hips   = q_track_yaw * q_track_roll * q_sway      # local hips frame
hips.translation = rest_translation + Vec3::X * x(t) # local
```

Right-multiplying makes the sway local to the hips. Keep translation additive to the rest position.

Blend with upper torso lean:
- Upper torso lean comes from the spine chain, which is driven by tracking.
- Let w = visibility-based weight (0 to 1). Sway amplitude scales by (1 - w) so sway does not fight tracked motion. Upper torso is independent of hips sway.

Legs counter-rotation without foot IK:
- Upper leg roll = `-k * roll(t)`, with k about 0.5 (UNVERIFIED). This counters the hip roll so the feet stay roughly planted in the visual sense.
- No foot IK: the foot is not placed on the floor; the counter rotation is a visual approximation only.
- kalidokit computes legs from `Vector.getSphericalCoords(lm[23], lm[25])` and clamps lower leg x to <= 0 (`LowerLeg.x = -Math.abs(theta)`). Its foot offscreen check uses visibility 0.63 (UNVERIFIED for current kalidokit version).

## Bevy sketch (not compiled)

```rust
// crates/avatar/src/window.rs (new system alongside smooth_targets)
// Reuse: follow_factor(dt, tau), slerp, SMOOTH_TAU_SECONDS pattern.
fn smooth_body(time: Res<Time>, raw: Res<BodyTargetRaw>, mut out: ResMut<BodyTargetSmoothed>) {
    let dt = time.delta_secs();
    for (bone, target) in raw.0.iter() {
        let f = follow_factor(dt, tau_for(bone));
        let prev = out.0.entry(bone.clone()).or_insert(*target);
        *prev = prev.slerp(*target, f);
    }
}

// Sway, per frame, in PostUpdate before VrmSystemSets::SpringBone:
fn hips_sway(time: Res<Time>, vis: Res<HipsVisibility>, mut q: Query<&mut Transform, With<HipsBone>>) {
    let t = time.elapsed_secs();
    let phase = std::f32::consts::TAU * 0.25 * t;
    let w = 1.0 - vis.0.clamp(0.0, 1.0);
    let roll = Quat::from_rotation_z(0.035 * phase.sin() * w);
    let tx = 0.02 * phase.sin() * w;
    for mut tf in &mut q { tf.rotation = tf.rotation * roll; tf.translation.x += tx; }
}
```

Caveats: the sketch is a shape, not tested code. Translation accumulation is wrong as written (adds each frame); the real version must store rest translation and set it. UNVERIFIED that Bevy 0.19 exposes these names as written.

Where new code goes: `crates/infer/src/tracker.rs` (new pose and hand stages), `crates/avatar/src/map_apply.rs` (keep face table separate; a bone table is needed), `crates/avatar/src/window.rs` (smoothing and sway systems), `crates/avatar/src/expressions.rs` (unchanged).

## Open questions and unverified items

1. Confirm MediaPipe pose world landmark definition in the current model version (2026).
2. Confirm VRM 1.0 humanoid rest-pose spec wording.
3. Choose a One Euro parameter set and check the paper.
4. Replace kalidokit (last npm update 2022) with a maintained approach, or keep it as a read-only reference.
5. Measure monocular z error on the project's camera before trusting depth-driven angles.
