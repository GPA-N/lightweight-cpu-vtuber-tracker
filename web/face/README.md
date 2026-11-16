# web/face: the app's face path in the browser (stage 2 of ticket 001)

JS port of the Rust face tracking and of the avatar-side targets: BlazeFace detector, Face Mesh landmarks (478),
146-point blendshape model, head fit, mirror, One Euro filters, face-lost hold, then the VRM expression, brow and gaze
values (`tracking_to_targets`). onnxruntime-web 1.31.1, wasm EP, one thread. `oneeuro.js` and `models.js` are imported from `../body/` read-only.

## Files
- `pipeline.js`: `FaceTracker` (`init`, `process(frame, dt, now?)`, `reset`). `process` returns the contract's
  `{ found, weights: Float32Array(52), headRotation: [x,y,z,w], expressions, brows, gaze, faceBox, landmarks: Float32Array(478*3) | null, ms }`
  plus `fitFailed` and `debug` (all intermediates, used by the parity test).
- `detector.js` (letterbox, anchors, decode, weighted NMS), `crop.js` (rotated rect, warp, `largest`, `DetectionScheduler`),
  `fit.js` (`fitHeadRotation`, own 3x3 SVD), `smooth.js` (`WeightsFilter`, `QuatFilter`, `FaceLost`),
  `targets.js` (names, `applyMirror`, `buildMap`/`applyMap`, `browsFromWeights`, `gazeFromExpressions`), `data.js` (generated: 33 canonical basis points, the 146 subset indices),
  `autobrightness.js` (`AutoBrightness`).
- `test/parity.mjs` (Chrome vs Rust), `test/page.mjs`, `test/units.test.mjs`.

## Names (Rust to JS)
`TrackingOutput.weights` -> `weights`, `head_rotation` -> `headRotation` ([x, y, z, w], what `quat_from_tracking` gives), `face_found` -> `found`.
`BrowInput` fields -> `brows.{downLeft, downRight, upLeft, upRight, wide, funnel, pucker, lipUp, lipDown}`; `GazeInput` -> `gaze.{x, y}`;
`map_apply::apply` -> `expressions` (only expressions with a table entry). `faceBox` is the bounding box of the 478 pixel landmarks (the Rust app's face box for the body stage is not ported; body ignores it).

## Decisions
- Models are not committed: `licenses/` holds only the SAM licence and `SHA256SUMS` names no licence, so the three face models load by URL (`{url} | {bytes}`).
- Face confidence default 0.8 (the app's tuning default); it is both the found threshold and the scheduler's low-presence threshold, as `set_min_presence` does.
- Time: the face-lost hold reads a clock. The Rust reads `Instant::now()`; here the clock is the sum of the clamped dts, or the `now` (seconds) you pass. dt is clamped to 1/120..1/5, the first call uses 1/30, like the Rust.
- Auto-brightness: on by default in the app (`capture`), it rewrites every camera frame before the face and body models. Ported as `AutoBrightness` (`process(rgb, w, h, nowSeconds)`, in place); the stage 4 page applies it to the camera frame before both trackers. The fixtures were dumped without it, so it is checked by the Rust unit tests only, not against Rust frames.
- Stale rect: like the Rust, when the detector does not run and there are no previous landmarks the last rect is kept.
- Head fit: f64 like the Rust, with a one-sided Jacobi SVD (singular values sorted descending, `R = V diag(1,1,d) U^T`); quaternion from the matrix as nalgebra does.
- Not ported: the body stage's face-box use, Bevy plumbing (gains, expression application on the mesh, gaze-bone aiming, head parent motion).

## Run
`node --no-warnings web/face/test/parity.mjs` (about 40 s; needs the same setup as `web/body/NOTES.md`); units `node --no-warnings --test web/face/test/units.test.mjs`.

## Measured
See `NOTES.md`.
