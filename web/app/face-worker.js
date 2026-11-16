// Web Worker (module): the face stage.
// In:  { type: 'init', ortBase, models: { detector, landmarks, blendshapes } (urls), mapTable, mirror, faceConfidence }
//      { type: 'config', mirror?, faceConfidence? }  { type: 'frame', id, rgb, w, h, dt }
// Out: { type: 'ready' } | { type: 'result', id, ...FaceTracker result without debug } | { type: 'error', message }
import { FaceTracker } from '../face/pipeline.js';

let tracker = null;

self.onmessage = async ({ data: m }) => {
  try {
    if (m.type === 'init') {
      const ort = await import(m.ortBase + 'ort.min.mjs');
      ort.env.wasm.wasmPaths = m.ortBase;
      ort.env.wasm.numThreads = 1;
      const u = m.models;
      tracker = new FaceTracker({
        ort, mapTable: m.mapTable, mirror: m.mirror, faceConfidence: m.faceConfidence,
        models: { detector: { url: u.detector }, landmarks: { url: u.landmarks }, blendshapes: { url: u.blendshapes } },
      });
      await tracker.init();
      self.postMessage({ type: 'ready' });
    } else if (m.type === 'config') {
      if (m.mirror !== undefined) tracker.mirror = m.mirror;
      if (m.faceConfidence !== undefined) tracker.sched.presenceThreshold = tracker.confidence = m.faceConfidence;
    } else if (m.type === 'frame') {
      const { debug, landmarks, ...r } = await tracker.process({ rgb: new Uint8Array(m.rgb), w: m.w, h: m.h }, m.dt);
      self.postMessage({ type: 'result', id: m.id, ...r });
    }
  } catch (e) {
    self.postMessage({ type: 'error', id: m.id, message: String(e?.message ?? e) });
  }
};
