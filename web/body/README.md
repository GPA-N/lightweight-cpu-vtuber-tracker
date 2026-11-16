# web/body: the app's body path in the browser (stage 1 of ticket 001)

JS port of the Rust body path, same models and same logic: RF-DETR person box (Web Worker), InstantHMR v1.1 nano, MHR
body model, One Euro joint smoothing, pose slots, `to_avatar_space` (mirror), the arm/spine/hand solver and the per-bone
follow filter. onnxruntime-web 1.31.1, wasm EP, one thread. No dependencies besides `ort`, no three.js.

## Files
- `pipeline.js`: `BodyTracker` (`init`, `process(frame, dt, face)`, `attachHands`, `reset`), `JointSmoother`.
- `solve.js`: `buildRig`, `solve` (returns `locals` per node, `changed` = Rust's `Solved.locals`, `report`), `BodyFollow`, `advanceBody`, `isHandBone`.
- `bodypose.js`: `toAvatarSpace`, `dropArmsWithoutElbow`, `poseLandmarks`, `SLOTS`.
- `person.js`, `instanthmr.js`, `oneeuro.js`: pure preprocess / decode / crop / cliff / filter code.
- `person-worker.js` + `detector.js`: RF-DETR off the main thread. `models.js`: `{url} | {chunks} | {bytes}` loader.
- `tools/split-model.mjs`: splits a model into parts of at most 50 MB (`<name>.partN`); parts and models are never committed.
- `test/`: `parity.mjs` (Chrome vs Rust fixtures), `units.test.mjs` (`node --test`), `lib.mjs`, `page.mjs`, `live.mjs`.

## Ported / not ported
Ported: `person_detector.rs` (preprocess, decode, expand, stride), `instant_hmr.rs` (crop, cliff, preprocess, decode, MHR run),
`hmr_body.rs` (`JointSmoother`, `SLOTS`, `pose_landmarks`, box reuse, `drop_arms_without_elbow`), `body.rs` (`to_avatar_space`,
`Rig`, `Pose`, `solve` incl. spine lean, arm reach, hand aim and roll, finger curl, `BodyFollow`, `advance_body`, `QuatSmoother`).
Not ported: `PersonBoxTracker` / `box_keypoints` (the live app always runs the detector, so the face-box fallback is dead
there; `process`'s `face` argument is accepted and ignored), `BodyLost` (the app does not call it), `HandSmoother` and the
hand models (stage 3), the Bevy system `apply_body_live` (stage 4 applies `advanceBody` output to the VRM).

## Conventions
- Quaternions `[x, y, z, w]`, vectors `[x, y, z]`, Hamilton product as glam (`qmul(a, b)` = `a * b`, applies `b` first), `Quat * Vec3` = glam `mul_vec3`.
- `Quat::inverse` is the conjugate (unit quaternions). `normalize_or_zero`: zero when 1/length is not finite and positive.
- `from_rotation_arc`: identity when dot > 1 - 2*EPS, `any_orthonormal_vector` axis at pi when dot < -(1 - 2*EPS), else normalised (cross, 1 + dot).
- Spine lean: `Ry(yaw*acc) * Rx(lean*acc) * Rz(roll*acc) * restGlobalRot`, weights 0.3/0.3/0.4 renormalised over the bones present.
- Mirror: `(sx*x, -y, -z)` with `sx = -1` when mirror is on; with mirror the person's right arm drives the avatar's left bones.
- f32: `Math.fround` after every operation in preprocess, decode, One Euro and the quaternion filter (division and multiplication of f32
  values in doubles then rounded equals f32 arithmetic). The solver runs in doubles on f32 inputs (differences about 1e-6).
- Hands: `attachHands` takes the smoothed per-side hands (`score >= handMin` is used); `solve` uses their 21 points for hand aim, roll and finger curl exactly as Rust.
- The first sample of a stateful run uses dt = 1/30 in the joint smoother (the maker's fallback), like the Rust.
- Extra, additive: `BodyTracker` options `ortUrl`, `wasmPaths` (where the worker imports ort), `awaitDetector` (tests); `process` also returns `crop`, `cond`, `raw`.

## Measured (headless system Chrome, 1 thread, Ryzen 5 7430U)
See `NOTES.md` for the parity table and timing of the last run.
