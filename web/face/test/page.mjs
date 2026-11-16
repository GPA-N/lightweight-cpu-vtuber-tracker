// Runs in headless Chrome (served by parity.mjs): the JS face pipeline on the fixture frames; outputs go to window.__results.
import * as ort from '/ort/ort.min.mjs';
import { FaceTracker } from '/web/face/pipeline.js';

ort.env.wasm.wasmPaths = '/ort/';
ort.env.wasm.numThreads = 1;
const j = async (p) => (await fetch(p)).json();
const bin = async (p) => new Uint8Array(await (await fetch(p)).arrayBuffer());
const arr = (x) => (x ? Array.from(x) : x);

const tracker = new FaceTracker({
  ort,
  models: { detector: { url: '/models/face_detector.onnx' }, landmarks: { url: '/models/face_landmarks.onnx' }, blendshapes: { url: '/models/face_blendshapes.onnx' } },
  mapTable: await j('/data/blendshape_map.json'),
  mirror: true,
});
let t = performance.now();
await tracker.init();
const timing = { initMs: performance.now() - t };

const frames = {};
async function frame(id) {
  const meta = await j(`/fix/frames/${id}.json`);
  return (frames[id] ??= { rgb: await bin(`/fix/frames/${id}.rgb`), w: meta.w, h: meta.h });
}
async function step(id, dt, rec) {
  const fr = await frame(id);
  const t0 = performance.now();
  const r = await tracker.process(fr, dt);
  const total = performance.now() - t0;
  const d = r.debug;
  rec.push({
    id, found: r.found, weights: arr(r.weights), headRotation: r.headRotation, expressions: r.expressions, brows: r.brows, gaze: r.gaze,
    ms: { ...r.ms, total }, ran: d.ran, detections: d.detections, rect: d.rect,
    landmarks: d.landmarks && { presence: d.landmarks.presence, found: d.landmarks.found, crop: arr(d.landmarks.crop) },
    pointsPx: d.pointsPx, subset: arr(d.subset), blendRaw: arr(d.blendRaw), headFit: d.headFit, mirrored: d.mirrored && { weights: arr(d.mirrored.weights), rotation: d.mirrored.rotation },
  });
}

const STILLS = ['photo_claws', 'photo_fists', 'photo_thumbs', 'priv_a', 'priv_b', 'priv_c', 'blank'];
const stills = [], seq = [], lost = [];
for (const id of STILLS) { tracker.reset(); await step(id, (await j(`/fix/face/${id}.json`)).dt, stills); }
tracker.reset();
for (let k = 0; k < 60; k++) {
  const n = String(k).padStart(3, '0');
  await step(`seq_${n}`, (await j(`/fix/seq/face/${n}.json`)).dt, seq);
}
// the face goes away after the sequence: 90 blank frames on the same clock (the Rust replays FaceLost on it)
for (let k = 1; k <= 90; k++) await step('blank', 0.03333333, lost);
window.__results = { stills, seq, lost, timing };
window.__done = true;
