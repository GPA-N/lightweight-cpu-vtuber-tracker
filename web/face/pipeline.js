// Port of crates/infer/src/tracker.rs `Tracker::step` plus avatar `tracking_to_targets`:
// BlazeFace detector, Face Mesh landmarks, 146-point blendshape model, head fit, mirror, One Euro filters, face-lost hold,
// then the VRM expression / brow / gaze values.
import { loadBytes } from '../body/models.js';
import * as det from './detector.js';
import { DetectionScheduler, largest, rectFromDetection, rectFromLandmarks, warpToTensor, cropToFrame } from './crop.js';
import { fitHeadRotation } from './fit.js';
import { FaceLost, QuatFilter, WeightsFilter } from './smooth.js';
import { BLENDSHAPE_NAMES, applyMap, applyMirror, browsFromWeights, buildMap, fromMediapipe, gazeFromExpressions } from './targets.js';
import { SUBSET_146 } from './data.js';

const f = Math.fround;
const WEIGHTS_MIN_CUTOFF = 1.0;
const WEIGHTS_BETA = 0.007;
const sigmoid = (x) => f(1 / f(1 + Math.exp(-x)));

export class FaceTracker {
  /**
   * `models`: `{ detector, landmarks, blendshapes }`, each `{ url } | { bytes }`. `mapTable`: parsed data/blendshape_map.json.
   * `faceConfidence` is the app default 0.8 (landmark presence below it counts as no face).
   */
  constructor({ ort, models, mapTable, mirror, faceConfidence = 0.8 }) {
    this.ort = ort;
    this.models = models;
    this.mirror = mirror;
    this.map = buildMap(mapTable);
    this.confidence = faceConfidence;
    this.anchors = det.generateAnchors();
    this.reset();
  }

  reset() {
    this.sched = new DetectionScheduler(this.confidence);
    this.rect = null;
    this.prevPx = null;
    this.weights = new WeightsFilter(WEIGHTS_MIN_CUTOFF, WEIGHTS_BETA);
    this.quat = new QuatFilter();
    this.lost = new FaceLost();
    this.started = false;
    this.clock = 0; // seconds: the sum of the (clamped) dts, what the Rust reads from Instant::now()
  }

  async init() {
    const opts = { executionProviders: ['wasm'] };
    const m = this.models;
    const [d, l, b] = await Promise.all([loadBytes(m.detector), loadBytes(m.landmarks), loadBytes(m.blendshapes)]);
    this.sessions = {
      detector: await this.ort.InferenceSession.create(d, opts),
      landmarks: await this.ort.InferenceSession.create(l, opts),
      blend: await this.ort.InferenceSession.create(b, opts),
    };
  }

  /**
   * `frame = { rgb, w, h }` (not mirrored; apply auto-brightness first if wanted), `dt` seconds since the last call.
   * `now` (seconds) overrides the internal clock the face-lost hold reads. Returns the contract's fields plus
   * `fitFailed` and `debug` (every intermediate, for the parity test).
   */
  async process(frame, dt, now) {
    const { rgb, w, h } = frame;
    const { ort, sessions } = this;
    const ms = { detector: 0, landmarks: 0, blendshapes: 0 };
    dt = this.started ? Math.min(Math.max(f(dt), f(1 / 120)), f(1 / 5)) : f(1 / 30);
    this.started = true;
    this.clock = f(this.clock + dt);
    now ??= this.clock;
    const dbg = { ran: false, detections: [], rect: null, landmarks: null, pointsPx: null, subset: null, blendRaw: null, headFit: null, mirrored: null };

    if (this.sched.shouldDetect()) {
      dbg.ran = true;
      let t = performance.now();
      const { data, lb } = det.preprocess(rgb, w, h);
      const out = await sessions.detector.run({ input: new ort.Tensor('float32', data, [1, det.INPUT_SIZE, det.INPUT_SIZE, 3]) });
      dbg.detections = det.decode(out.regressors.data, out.classificators.data, this.anchors, lb);
      ms.detector = performance.now() - t;
      const i = largest(dbg.detections.map((d) => d.box));
      this.rect = i === null ? null : rectFromDetection(dbg.detections[i].box, dbg.detections[i].keypoints[0], dbg.detections[i].keypoints[1], w, h);
    } else if (this.prevPx && this.prevPx.length > 263) {
      this.rect = rectFromLandmarks(this.prevPx.map((p) => [f(p[0] / w), f(p[1] / h)]), 33, 263, w, h);
    }
    const lostResult = () => this.#result(this.lost.update(null, now), null, ms, dbg);
    if (!this.rect) {
      this.prevPx = null;
      this.sched.report(false, null);
      return lostResult();
    }
    dbg.rect = this.rect;

    let t = performance.now();
    const lm = await sessions.landmarks.run({ input_12: new ort.Tensor('float32', warpToTensor(rgb, w, h, this.rect, 256), [1, 256, 256, 3]) });
    ms.landmarks = performance.now() - t;
    const presence = sigmoid(lm.Identity_1.data[0]);
    const found = presence >= this.sched.presenceThreshold;
    dbg.landmarks = { presence, logit: lm.Identity_1.data[0], found, crop: lm.Identity.data };
    this.sched.report(found, presence);
    if (!found) {
      this.prevPx = null;
      return lostResult();
    }

    const pts = lm.Identity.data;
    const px = Array.from({ length: 478 }, (_, k) => {
      const [x, y] = cropToFrame(pts[k * 3], pts[k * 3 + 1], this.rect, 256);
      return [x, y, f(f(pts[k * 3 + 2] / 256) * this.rect.width)];
    });
    dbg.pointsPx = px;
    const subset = Float32Array.from(SUBSET_146.flatMap((i) => [px[i][0], px[i][1]]));
    dbg.subset = subset;
    t = performance.now();
    const b = await sessions.blend.run({ 'serving_default_input_points:0': new ort.Tensor('float32', subset, [1, 146, 2]) });
    ms.blendshapes = performance.now() - t;
    dbg.blendRaw = b['StatefulPartitionedCall:0'].data;
    const raw = fromMediapipe(dbg.blendRaw);

    const fit = fitHeadRotation(px);
    dbg.headFit = fit;
    const rotation = fit.error ? [0, 0, 0, 1] : fit.rotation;
    this.prevPx = px;

    const [mw, mr] = applyMirror(this.mirror, raw, rotation);
    dbg.mirrored = { weights: mw, rotation: mr };
    const out = { weights: this.weights.filter(mw, dt), headRotation: this.quat.filter(mr, dt) };
    return this.#result(this.lost.update(out, now), px, ms, dbg);
  }

  #result(o, px, ms, debug) {
    const get = (n) => { const i = BLENDSHAPE_NAMES.indexOf(n); return i < 0 ? 0 : o.weights[i]; };
    const expressions = applyMap(this.map, o.weights);
    const g = (n) => expressions[n] ?? 0;
    let faceBox = null;
    if (px) {
      const xs = px.map((p) => p[0]), ys = px.map((p) => p[1]);
      const [x0, y0] = [Math.min(...xs), Math.min(...ys)];
      faceBox = { x: x0, y: y0, w: Math.max(...xs) - x0, h: Math.max(...ys) - y0 };
    }
    return {
      found: o.faceFound, weights: o.weights, headRotation: o.headRotation, expressions,
      brows: browsFromWeights(get), gaze: gazeFromExpressions(g), faceBox,
      landmarks: px ? Float32Array.from(px.flat()) : null, ms, fitFailed: !!debug.headFit?.error, debug,
    };
  }
}
