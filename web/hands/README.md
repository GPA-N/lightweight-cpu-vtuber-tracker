# web/hands: the app's hand stage in the browser (stage 3 of ticket 001)

JS port of `Mp2023Hands` (crates/infer/src/upper_body.rs) and `HandSmoother` (crates/app/src/hmr_body.rs): MediaPipe 2023 palm
detector (192 px) and hand landmark model (224 px, 21 points), side assignment, per-side One Euro smoothing.
onnxruntime-web 1.31.1, wasm EP, one thread. `oneeuro.js` and `models.js` are imported from `../body/` read-only.

## Files
- `pipeline.js`: `HandStage` (`init`, `run(frame, pose)`, `lastMs = { palm, hand }`, `debug`) and `HandSmoother` (`apply(hands, nowSeconds)`, in place).
- `hands.js`: pure code: `warpBlack`, `ssdAnchors`, `decodeDetections`, `nms`, `palmRoi`, `handRect`, `handToFrame`, `assignSides`.
- `test/parity.mjs`, `test/page.mjs`, `test/units.test.mjs`.

## Behaviour (as the Rust)
- Palm detector on the whole frame letterboxed to a square; best 2 after NMS (IoU 0.3, score 0.5); each gives a rotated ROI; landmarks per ROI.
- No palm found: pose-derived crop for each arm whose pose wrist visibility is >= 0.5 (elbow, wrist, index, pinky slots 13/15/19/17 and 14/16/20/18).
- Side: nearest pose wrist (slots 15 left, 16 right), total distance minimal. Output points are frame pixels, z on the x scale (z / 224 * crop width), score and handedness as the model gives them.
- The caller passes the pose the way the app does: the previous good frame's pose, else (first frame) the current frame's, from `BodyTracker.process().pose`.
- Smoother: first hand of each side, one filter per coordinate (1.0 Hz, beta 0.01), dt = time since that side last had a hand, 1/30 s on the first sample or a zero gap; a side with no hand keeps its filters.
- Extra beyond the Rust: `pose = null` is allowed (palm detections only; sides then by wrist x, smaller x is the person's right in an unmirrored frame). The Rust always has a pose, so this path has no reference.
- The app lets a hand score below 0.8 through the stage; `toAvatarSpace` drops it (handMin).
- Not ported: the InstantHMR-joint fallback `hmr_hands.rs` (not used by the default live configuration: `live.rs` runs `Mp2023Hands`); the solve-side use of the landmarks (finger curl, hand aim, roll) is already in `web/body/solve.js` and is tested here with these hands.

## Run
`node --no-warnings web/hands/test/parity.mjs` (about 30 s, setup as `web/body/NOTES.md`); units `node --no-warnings --test web/hands/test/units.test.mjs`.
Models are not committed (`licenses/` has only the SAM licence): load by URL.

## Measured
See `NOTES.md`.
