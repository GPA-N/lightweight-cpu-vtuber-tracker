// Port of crates/app/src/hmr_body.rs `HmrBody::run`: RF-DETR box (worker), InstantHMR nano, MHR body model,
// joint smoothing, pose slots and the avatar-space pose. Fingers come from the hand stage (attachHands).
import { OneEuro } from './oneeuro.js';
import { DETECTOR_STRIDE, expand } from './person.js';
import * as hmr from './instanthmr.js';
import { loadBytes } from './models.js';
import { PersonDetectorWorker } from './detector.js';
import { SLOTS, dropArmsWithoutElbow, poseLandmarks, toAvatarSpace } from './bodypose.js';

const SMOOTH_2D = [1.0, 0.01]; // (min cutoff Hz, beta) for joints_2d, frame pixels
const SMOOTH_3D = [1.0, 4.0]; // for joints_3d_local, metres
const FALLBACK_DT = Math.fround(1 / 30);

/** The demo's temporal smoothing of the 13 joints the avatar reads: one One Euro filter per coordinate. */
export class JointSmoother {
  constructor() {
    this.xy = SLOTS.map(() => [0, 1].map(() => new OneEuro(...SMOOTH_2D)));
    this.xyz = SLOTS.map(() => [0, 1, 2].map(() => new OneEuro(...SMOOTH_3D)));
    this.started = false;
  }

  /** Filters `j = { xy: Float32Array(140), xyz: Float32Array(210) }` in place; `dt` seconds since the last call. */
  apply(j, dt) {
    // The maker's fallback: 1/30 s when no time has passed (also the first call).
    dt = this.started && dt > 0 ? dt : FALLBACK_DT;
    this.started = true;
    SLOTS.forEach(([, m], i) => {
      for (let k = 0; k < 2; k++) j.xy[m * 2 + k] = this.xy[i][k].filter(j.xy[m * 2 + k], dt);
      for (let k = 0; k < 3; k++) j.xyz[m * 3 + k] = this.xyz[i][k].filter(j.xyz[m * 3 + k], dt);
    });
  }
}

export class BodyTracker {
  /**
   * `ort`: the onnxruntime-web module (wasm EP, one thread, configured by the caller). `models`: `{ rfdetr, hmr, mhr }`,
   * each `{ url } | { chunks } | { bytes }`. `ortUrl` / `wasmPaths` tell the RF-DETR worker where to import ort from
   * (default: `ort.env.wasm.wasmPaths + 'ort.min.mjs'`). `awaitDetector` makes a stride frame wait for its detection
   * (tests, and stills); live use leaves it false so the page never waits on the detector.
   */
  constructor({ ort, models, mirror, visMin = 0.5, handMin = 0.5, ortUrl, wasmPaths, awaitDetector = false, detectorInput }) {
    this.ort = ort;
    this.models = models;
    this.mirror = mirror;
    this.visMin = visMin;
    this.handMin = handMin;
    this.awaitDetector = awaitDetector;
    const paths = wasmPaths ?? ort.env.wasm.wasmPaths;
    this.detector = new PersonDetectorWorker({ source: models.rfdetr, ortUrl: ortUrl ?? paths + 'ort.min.mjs', wasmPaths: paths, inputName: detectorInput });
    this.hands = [];
    this.reset();
  }

  reset() {
    this.smooth = new JointSmoother();
    this.frames = 0;
    this.latest = null; // { box, id } last box the detector delivered
    this.boxId = 0;
    this.detectorMs = 0;
  }

  async init() {
    const opts = { executionProviders: ['wasm'] };
    const [hmrBytes, mhrBytes] = await Promise.all([loadBytes(this.models.hmr), loadBytes(this.models.mhr)]);
    this.hmrSession = await this.ort.InferenceSession.create(hmrBytes, opts);
    this.mhrSession = await this.ort.InferenceSession.create(mhrBytes, opts);
    await this.detector.init();
  }

  /** Smoothed per-side hand landmarks `[{ side: 'left'|'right', points: [[x,y,z] x21], score }]` for the next `process`. */
  attachHands(hands) {
    this.hands = hands ?? [];
  }

  #deliver(r) {
    if (!r) return;
    this.detectorMs = r.ms;
    if (r.box) this.latest = { box: r.box, id: (this.latest?.id ?? 0) + 1 };
  }

  /** `frame = { rgb, w, h }` (not mirrored), `dt` seconds since the last call. `face` is unused: the live app always runs the detector. */
  async process(frame, dt, face = null) {
    const { rgb, w, h } = frame;
    if (rgb.length !== w * h * 3 || w === 0 || h === 0) throw new Error('image buffer does not match its size');
    const whole = { x: 0, y: 0, w, h };
    if (this.frames % DETECTOR_STRIDE === 0) {
      const job = this.detector.detect(frame);
      if (this.awaitDetector) this.#deliver(await job);
      else job.then((r) => this.#deliver(r), () => {});
    }
    this.frames++;
    let person = whole;
    let boxFresh = false;
    if (this.latest) {
      boxFresh = this.latest.id !== this.boxId;
      this.boxId = this.latest.id;
      person = boxFresh ? this.latest.box : expand(this.latest.box, w, h);
    }

    const ms = { detector: this.detectorMs, hmr: 0, mhr: 0 };
    const crop = hmr.squareCrop(person);
    const cond = hmr.cliffCond(person, w, h);
    const image = hmr.preprocess(rgb, w, h, crop);
    const ort = this.ort;
    let t = performance.now();
    const net = await this.hmrSession.run({
      image: new ort.Tensor('float32', image, [1, 3, hmr.INPUT, hmr.INPUT]),
      cliff_cond: new ort.Tensor('float32', cond, [1, 3]),
    });
    ms.hmr = performance.now() - t;
    t = performance.now();
    const body = await this.mhrSession.run(
      { mhr_params: net.mhr_params, shape_params: net.shape_params },
      ['joints_3d'], // the model also returns the 18k-vertex mesh, which nothing reads
    );
    ms.mhr = performance.now() - t;
    // The 2D output is found by shape, like the Rust (70 x 2 values).
    const two = Object.values(net).find((o) => o.dims[o.dims.length - 1] === 2 && o.size === hmr.JOINTS * 2);
    const raw = hmr.decode(two.data, body.joints_3d.data, crop);
    const joints = { xy: Float32Array.from(raw.xy), xyz: Float32Array.from(raw.xyz) };
    this.smooth.apply(joints, dt);

    const pose = poseLandmarks(joints.xy, joints.xyz);
    const bodyPose = dropArmsWithoutElbow(toAvatarSpace(pose, this.hands, this.mirror, this.visMin, this.handMin));
    return { body: bodyPose, pose, joints, box: person, boxFresh, ms, crop, cond, raw };
  }
}
