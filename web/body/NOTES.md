# Notes for resuming (ticket 001, branch ticket/001-js-port)

State: stage 1 (body) done and committed. Stages 2 face, 3 hands, 4 page are specified in the main checkout
`specs/tickets/00{2,3,4}-*/` (untracked). Results of stage 1: `specs/tickets/001-body-port/results.md` (main checkout).

## Run
- Parity (Chrome vs Rust, about 90 s): from the repo root `node --no-warnings web/body/test/parity.mjs`.
  Needs `%TEMP%\webtest` (playwright-core, onnxruntime-web 1.31.1), models in `C:\Users\Lenovo\relstage\vtuber-tracker\models`,
  fixtures in `C:\Users\Lenovo\web-fixtures` (env `WEBTEST`, `MODELS`, `WEB_FIXTURES`, `CHROME` override).
- `RUSTBOX=1` hands Rust's own detected box to the pipeline (separates RF-DETR noise from the rest); `DUMP=<file>` writes our outputs.
- Units: `node --no-warnings --test web/body/test/units.test.mjs`.
- Live mode and chunks: `PAGE=live.mjs CHUNKS=<dir with index.json + rfdetr_nano.onnx.partN> node web/body/test/parity.mjs`.

## Decided
- `solve()` returns `locals` (one per rig node, contract) and `changed` (`[index, quat][]`, Rust `Solved.locals`); `advanceBody` takes `changed`.
- No `PersonBoxTracker`. `process`'s `face` is ignored.
- Parity harness pattern for stages 2/3: a page module (`test/page.mjs`) runs the JS on fixture frames and leaves JSON in `window.__results`;
  `parity.mjs` serves `/ort/ /models/ /fix/ /web/`, launches Chrome, compares in node (`test/lib.mjs`: `maxDiff`, `angleDeg`).
  JSON numbers from fixtures are doubles: `Math.fround` them before feeding f32 code.
- Detector noise: RF-DETR in ORT-web differs from native by about 2e-3 px in the box, which the body model amplifies to up to 0.49 px
  on the sequence steps that reuse one box (steps 1..29). With Rust's box the pipeline matches to 3e-3 px / 1e-6 m / 2e-3 deg.

## Remains
- Hands (stage 3) feed `tracker.attachHands`; the 21-point hand path of `solve` is already ported and checked with the fixture hands.
- Page (stage 4): apply `advanceBody` output (`[nodeIndex, quat]`) to the VRM bones; production model URLs on huggingface.co/momolesang/InstantHMR (nano, MHR) and chunked rfdetr parts (run `tools/split-model.mjs`).
