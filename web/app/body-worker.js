// Web Worker (module): the body stage (RF-DETR runs in its own nested worker, InstantHMR + MHR here).
// In:  { type: 'init', ortBase, models: { rfdetr, hmr, mhr } (urls) }  { type: 'frame', id, rgb, w, h, dt }
// Out: { type: 'ready' } | { type: 'result', id, pose, xy, box, boxFresh, ms } | { type: 'error', message }
import { BodyTracker } from '../body/pipeline.js';

let tracker = null;

self.onmessage = async ({ data: m }) => {
  try {
    if (m.type === 'init') {
      const ort = await import(m.ortBase + 'ort.min.mjs');
      ort.env.wasm.wasmPaths = m.ortBase;
      ort.env.wasm.numThreads = 1;
      tracker = new BodyTracker({
        ort, mirror: true, wasmPaths: m.ortBase, ortUrl: m.ortBase + 'ort.min.mjs', detectorInput: m.detectorInput,
        models: { rfdetr: { url: m.models.rfdetr }, hmr: { url: m.models.hmr }, mhr: { url: m.models.mhr } },
      });
      await tracker.init();
      self.postMessage({ type: 'ready' });
    } else if (m.type === 'frame') {
      const r = await tracker.process({ rgb: new Uint8Array(m.rgb), w: m.w, h: m.h }, m.dt);
      self.postMessage({ type: 'result', id: m.id, pose: r.pose, xy: r.joints.xy, box: r.box, boxFresh: r.boxFresh, ms: r.ms });
    }
  } catch (e) {
    self.postMessage({ type: 'error', id: m.id, message: String(e?.message ?? e) });
  }
};
