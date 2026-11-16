// Port of Mp2023Hands::run (upper_body.rs) and HandSmoother (hmr_body.rs): MediaPipe 2023 palm detector + hand landmarks.
import { OneEuro } from '../body/oneeuro.js';
import { loadBytes } from '../body/models.js';
import * as H from './hands.js';

const f = Math.fround;
const SMOOTH_2D = [1.0, 0.01]; // (min cutoff Hz, beta), the same for x, y and z (z is on the x scale)
const FALLBACK_DT = f(1 / 30);
const SIDES = ['left', 'right'];

export class HandStage {
  /** `models`: `{ palm, hand }`, each `{ url } | { chunks } | { bytes }`. */
  constructor({ ort, models }) {
    this.ort = ort;
    this.models = models;
    this.anchors = H.ssdAnchors(H.PALM_SIZE, [8, 16, 16, 16]);
    this.lastMs = { palm: 0, hand: 0 };
  }

  async init() {
    const opts = { executionProviders: ['wasm'] };
    const [p, h] = await Promise.all([loadBytes(this.models.palm), loadBytes(this.models.hand)]);
    this.palm = await this.ort.InferenceSession.create(p, opts);
    this.hand = await this.ort.InferenceSession.create(h, opts);
  }

  async #palmDetections(img) {
    const n = H.PALM_SIZE;
    const out = await this.palm.run({ input_1: new this.ort.Tensor('float32', H.warpBlack(img, H.letterboxRect(img.w, img.h), n), [1, n, n, 3]) });
    return H.nms(H.decodeDetections(out.Identity.data, out.Identity_1.data, this.anchors, n, 7, H.DET_SCORE_MIN), H.NMS_IOU);
  }

  async #landmarks(img, rect) {
    const n = H.HAND_LM_SIZE;
    const out = await this.hand.run({ input_1: new this.ort.Tensor('float32', H.warpBlack(img, rect, n), [1, n, n, 3]) }, ['Identity', 'Identity_1', 'Identity_2']);
    const lm = out.Identity.data;
    return {
      points: Array.from({ length: 21 }, (_, i) => [lm[i * 3], lm[i * 3 + 1], lm[i * 3 + 2]]),
      score: out.Identity_1.data[0], handedness: out.Identity_2.data[0],
    };
  }

  /**
   * `frame = { rgb, w, h }` (not mirrored), `pose` = PoseLandmark[33] (the previous good frame's when there is one, like the app)
   * or null (palm detections only; sides then go by wrist x: the smaller x is the person's right in an unmirrored frame).
   * Returns `[{ side, points: [[x,y,z] x21] (frame pixels), score, handedness }]`, raw (see HandSmoother). `debug` keeps the intermediates.
   */
  async run(frame, pose) {
    const img = { rgb: frame.rgb, w: frame.w, h: frame.h };
    const debug = { palm: [], rois: [], raw: [] };
    let t = performance.now();
    const dets = await this.#palmDetections(img);
    this.lastMs = { palm: performance.now() - t, hand: 0 };
    debug.palm = dets;
    const found = [];
    t = performance.now();
    for (const det of dets.slice(0, 2)) {
      const rect = H.palmRoi(det, img.w, img.h);
      debug.rois.push(rect);
      if (rect) found.push({ raw: await this.#landmarks(img, rect), rect, source: 'palm' });
    }
    if (found.length === 0 && pose) {
      // palm detector found nothing: try a pose-derived crop for each arm whose wrist is visible
      for (const ids of [[13, 15, 19, 17], [14, 16, 20, 18]]) {
        const [e, wr, ix, pk] = ids.map((i) => pose[i]);
        if (wr.visibility < 0.5) continue;
        const rect = H.handRect(e, wr, ix, pk);
        if (rect) found.push({ raw: await this.#landmarks(img, rect), rect, source: 'pose' });
      }
    }
    this.lastMs.hand = performance.now() - t;
    const wrists = found.map(({ raw, rect }) => H.handToFrame(raw.points, rect, H.HAND_LM_SIZE)[0].slice(0, 2));
    const sides = pose ? H.assignSides(wrists, pose) : wrists.length === 2 ? (wrists[0][0] <= wrists[1][0] ? ['right', 'left'] : ['left', 'right']) : wrists.map((w) => (w[0] < img.w / 2 ? 'right' : 'left'));
    debug.raw = found;
    this.debug = debug;
    return found.map(({ raw, rect }, i) => ({ side: sides[i], points: H.handToFrame(raw.points, rect, H.HAND_LM_SIZE), score: raw.score, handedness: raw.handedness }));
  }
}

/** One One Euro filter per coordinate of each of the 21 points, per side; a side with no hand keeps its filters. */
export class HandSmoother {
  constructor() {
    this.sides = SIDES.map(() => ({ filters: Array.from({ length: 21 }, () => [0, 1, 2].map(() => new OneEuro(...SMOOTH_2D))), last: null }));
  }

  /** Filters the first hand of each side in place and returns `hands`. `now` in seconds. */
  apply(hands, now) {
    SIDES.forEach((side, si) => {
      const hand = hands.find((h) => h.side === side);
      if (!hand) return;
      const s = this.sides[si];
      const gap = s.last === null ? 0 : f(now - s.last);
      const dt = gap > 0 ? gap : FALLBACK_DT;
      hand.points.forEach((p, i) => { for (let k = 0; k < 3; k++) p[k] = s.filters[i][k].filter(p[k], dt); });
      s.last = now;
    });
    return hands;
  }
}
