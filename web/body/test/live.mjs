// Side check (PAGE=live.mjs CHUNKS=<dir with rfdetr_nano.onnx.partN> node web/body/test/parity.mjs): live mode, detector off the
// main thread, RF-DETR loaded from chunks. Reports the longest main-thread stall while the detector runs and whether
// the delivered box equals the Rust one.
import * as ort from '/ort/ort.min.mjs';
import { BodyTracker } from '/web/body/pipeline.js';

ort.env.wasm.wasmPaths = '/ort/';
ort.env.wasm.numThreads = 1;
const j = async (p) => (await fetch(p)).json();
const chunks = (await (await fetch('/chunks/index.json')).json()).map((n) => '/chunks/' + n);

const tracker = new BodyTracker({
  ort,
  models: { rfdetr: { chunks }, hmr: { url: '/models/upper_body/instanthmr-nano.onnx' }, mhr: { url: '/models/upper_body/mhr-body.onnx' } },
  mirror: true, visMin: 0.8, handMin: 0.8, // awaitDetector stays false: this is the live configuration
});
await tracker.init();
const meta = await j('/fix/frames/photo_claws.json');
const frame = { rgb: new Uint8Array(await (await fetch('/fix/frames/photo_claws.rgb')).arrayBuffer()), w: meta.w, h: meta.h };

const first = await tracker.process(frame, 1 / 30); // asks the detector, runs on the whole frame meanwhile
const t0 = performance.now();
let last = t0, maxGap = 0;
while (!tracker.latest) {
  await new Promise((r) => setTimeout(r, 10));
  const now = performance.now();
  maxGap = Math.max(maxGap, now - last);
  last = now;
}
const waited = performance.now() - t0;
const second = await tracker.process(frame, 1 / 30);
const want = (await j('/fix/body/photo_claws.json')).detector.detected;
window.__results = {
  firstBoxWasWholeFrame: first.box.w === meta.w && first.box.h === meta.h && !first.boxFresh,
  detectorWaitedMs: Math.round(waited), maxMainThreadGapMs: Math.round(maxGap),
  secondBoxFresh: second.boxFresh, boxDiffVsRust: Math.max(...['x', 'y', 'w', 'h'].map((k) => Math.abs(second.box[k] - want[k]))),
  chunkCount: chunks.length,
};
window.__done = true;
