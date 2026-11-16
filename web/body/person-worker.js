// Web Worker (type: module) that runs RF-DETR so the page never waits on the ~1 s single-thread run.
// Messages in:  { type: 'init', ortUrl, wasmPaths, source }  and  { type: 'detect', id, rgb, w, h }
// Messages out: { type: 'ready' } | { type: 'result', id, box, ms } | { type: 'error', id, message }
import { loadBytes } from './models.js';
import { INPUT, decode, preprocess } from './person.js';

let ort = null;
let session = null;

self.onmessage = async ({ data: m }) => {
  try {
    if (m.type === 'init') {
      ort = await import(m.ortUrl);
      ort.env.wasm.wasmPaths = m.wasmPaths;
      ort.env.wasm.numThreads = 1;
      session = await ort.InferenceSession.create(await loadBytes(m.source), { executionProviders: ['wasm'] });
      self.postMessage({ type: 'ready' });
    } else if (m.type === 'detect') {
      const t0 = performance.now();
      const input = new ort.Tensor('float32', preprocess(m.rgb, m.w, m.h), [1, 3, INPUT, INPUT]);
      const out = await session.run({ input });
      // Outputs are told apart by shape like the Rust: boxes end in 4, labels are the other 3-D tensor.
      const outs = session.outputNames.map((n) => out[n]);
      const dets = outs.find((o) => o.dims[o.dims.length - 1] === 4);
      const labels = outs.find((o) => o !== dets && o.dims.length === 3);
      const box = decode(dets.data, labels.data, labels.dims[labels.dims.length - 1], m.w, m.h);
      self.postMessage({ type: 'result', id: m.id, box, ms: performance.now() - t0 });
    }
  } catch (e) {
    self.postMessage({ type: 'error', id: m.id, message: String(e?.message ?? e) });
  }
};
