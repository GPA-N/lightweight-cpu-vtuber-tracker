// Runs in headless Chrome (served by parity.mjs). Runs the JS body pipeline on the fixture frames and leaves the
// outputs in window.__results; parity.mjs compares them with the Rust reference.
import * as ort from '/ort/ort.min.mjs';
import { BodyTracker } from '/web/body/pipeline.js';

ort.env.wasm.wasmPaths = '/ort/';
ort.env.wasm.numThreads = 1;

const j = async (p) => (await fetch(p)).json();
const bin = async (p) => new Uint8Array(await (await fetch(p)).arrayBuffer());
const arr = (x) => Array.from(x);

const STILLS = ['photo_claws', 'photo_fists', 'photo_thumbs', 'priv_a', 'priv_b', 'priv_c', 'blank'];
const SEQ_LEN = 60;
const timing = {};

const tracker = new BodyTracker({
  ort,
  models: {
    rfdetr: { url: '/models/upper_body/rfdetr_nano.onnx' },
    hmr: { url: '/models/upper_body/instanthmr-nano.onnx' },
    mhr: { url: '/models/upper_body/mhr-body.onnx' },
  },
  mirror: true,
  visMin: 0.8,
  handMin: 0.8,
  awaitDetector: true, // the reference delivers the detection on the step that asked for it
});
let t = performance.now();
await tracker.init();
timing.initMs = performance.now() - t;

// ?rustbox=1: the detector hands over Rust's own detected box, to separate detector noise from the rest of the pipeline.
const RUST_BOX = new URLSearchParams(location.search).has('rustbox');

async function step(id, frameId, dt, handsPath, fx) {
  const meta = await j(`/fix/frames/${frameId}.json`);
  const frame = { rgb: await bin(`/fix/frames/${frameId}.rgb`), w: meta.w, h: meta.h };
  tracker.attachHands((await j(handsPath)).smoothed);
  if (RUST_BOX) {
    const d = fx.detector.detected;
    const box = d ? Object.fromEntries(Object.entries(d).map(([k, v]) => [k, Math.fround(v)])) : null;
    tracker.detector.detect = async () => ({ box, ms: 0 });
  }
  const t0 = performance.now();
  const r = await tracker.process(frame, dt);
  const total = performance.now() - t0;
  return {
    id, box: r.box, boxFresh: r.boxFresh, crop: r.crop, cond: arr(r.cond), pose: r.pose, body: r.body,
    raw: { xy: arr(r.raw.xy), xyz: arr(r.raw.xyz) }, joints: { xy: arr(r.joints.xy), xyz: arr(r.joints.xyz) },
    ms: { ...r.ms, total },
  };
}

const stills = [];
for (const id of STILLS) {
  tracker.reset();
  const fx = await j(`/fix/body/${id}.json`);
  stills.push(await step(id, id, fx.dt, `/fix/hands/${id}.json`, fx));
}
const seq = [];
tracker.reset();
for (let k = 0; k < SEQ_LEN; k++) {
  const n = String(k).padStart(3, '0');
  const fx = await j(`/fix/seq/body/${n}.json`);
  seq.push(await step(n, `seq_${n}`, fx.dt, `/fix/seq/hands/${n}.json`, fx));
}
window.__results = { stills, seq, timing };
window.__done = true;
