// Runs in headless Chrome (served by parity.mjs): the JS hand stage on the fixture frames with the pose the Rust was given.
import * as ort from '/ort/ort.min.mjs';
import { HandStage, HandSmoother } from '/web/hands/pipeline.js';

ort.env.wasm.wasmPaths = '/ort/';
ort.env.wasm.numThreads = 1;
const j = async (p) => (await fetch(p)).json();
const bin = async (p) => new Uint8Array(await (await fetch(p)).arrayBuffer());

const stage = new HandStage({
  ort,
  models: { palm: { url: '/models/upper_body/palm_detection_mediapipe_2023feb.onnx' }, hand: { url: '/models/upper_body/handpose_estimation_mediapipe_2023feb.onnx' } },
});
let t = performance.now();
await stage.init();
const timing = { initMs: performance.now() - t };
const DT_S = 33_333_333e-9; // the fixed clock of the dump: step k happens at k * 33_333_333 ns

async function step(frameId, fx, smoother, k, rec) {
  const meta = await j(`/fix/frames/${frameId}.json`);
  const frame = { rgb: await bin(`/fix/frames/${frameId}.rgb`), w: meta.w, h: meta.h };
  const raw = await stage.run(frame, fx.poseIn);
  const copy = raw.map((h) => ({ ...h, points: h.points.map((p) => p.slice()) }));
  const smoothed = smoother.apply(copy, k * DT_S);
  rec.push({
    id: frameId, ms: { ...stage.lastMs }, palm: stage.debug.palm, rois: stage.debug.rois,
    found: stage.debug.raw.map((r) => ({ source: r.source, rect: r.rect, crop: r.raw.points })), raw, smoothed,
  });
}

const STILLS = ['photo_claws', 'photo_fists', 'photo_thumbs', 'priv_a', 'priv_b', 'priv_c', 'blank'];
const stills = [], seq = [];
for (const id of STILLS) await step(id, await j(`/fix/hands/${id}.json`), new HandSmoother(), 0, stills);
const sm = new HandSmoother();
for (let k = 0; k < 60; k++) {
  const n = String(k).padStart(3, '0');
  await step(`seq_${n}`, await j(`/fix/seq/hands/${n}.json`), sm, k, seq);
}
// no hand: a blank frame without a pose gives an empty list
const blank = await j('/fix/frames/blank.json');
const empty = await stage.run({ rgb: await bin('/fix/frames/blank.rgb'), w: blank.w, h: blank.h }, null);
window.__results = { stills, seq, timing, emptyWithoutPose: empty.length };
window.__done = true;
