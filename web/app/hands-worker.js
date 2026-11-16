// Web Worker (module): the hand stage (palm detector + hand landmarks). Smoothing happens on the page (HandSmoother).
// In:  { type: 'init', ortBase, models: { palm, hand } (urls) }  { type: 'frame', id, rgb, w, h, pose }
// Out: { type: 'ready' } | { type: 'result', id, hands, ms } | { type: 'error', message }
import { HandStage } from '../hands/pipeline.js';

let stage = null;

self.onmessage = async ({ data: m }) => {
  try {
    if (m.type === 'init') {
      const ort = await import(m.ortBase + 'ort.min.mjs');
      ort.env.wasm.wasmPaths = m.ortBase;
      ort.env.wasm.numThreads = 1;
      stage = new HandStage({ ort, models: { palm: { url: m.models.palm }, hand: { url: m.models.hand } } });
      await stage.init();
      self.postMessage({ type: 'ready' });
    } else if (m.type === 'frame') {
      const hands = await stage.run({ rgb: new Uint8Array(m.rgb), w: m.w, h: m.h }, m.pose);
      self.postMessage({ type: 'result', id: m.id, hands, ms: { ...stage.lastMs } });
    }
  } catch (e) {
    self.postMessage({ type: 'error', id: m.id, message: String(e?.message ?? e) });
  }
};
