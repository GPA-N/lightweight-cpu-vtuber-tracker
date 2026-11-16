// Port of crates/smoothing: WeightsFilter, QuatFilter, FaceLost (One Euro from web/body/oneeuro.js).
import { OneEuro } from '../body/oneeuro.js';

const f = Math.fround;
const clamp01 = (x) => Math.min(Math.max(x, 0), 1);

export class WeightsFilter {
  constructor(minCutoff, beta) {
    this.filters = Array.from({ length: 52 }, () => new OneEuro(minCutoff, beta));
  }
  filter(w, dt) {
    return Float32Array.from(w, (x, i) => clamp01(this.filters[i].filter(clamp01(f(x)), dt)));
  }
}

/** One Euro per quaternion component, sign-aligned to the previous output, then normalised. */
export class QuatFilter {
  constructor(minCutoff = 1, beta = 0) {
    this.filters = [0, 1, 2, 3].map(() => new OneEuro(minCutoff, beta));
    this.prev = null;
  }
  filter(q, dt) {
    let c = q.map(f);
    if (this.prev && c.reduce((s, v, i) => f(s + f(v * this.prev[i])), 0) < 0) c = c.map((v) => -v);
    const o = c.map((v, i) => this.filters[i].filter(v, dt));
    const n = f(Math.sqrt(o.reduce((s, v) => f(s + f(v * v)), 0)));
    this.prev = o.map((v) => f(v / n));
    return this.prev;
  }
}

const easeAfterHold = (elapsed) => Math.min(Math.max(f(elapsed - 0.5), 0), 1); // holds 0.5 s, ramps over 1 s

// nalgebra UnitQuaternion::slerp towards identity (shortest way)
function slerpToIdentity(q, t) {
  const id = [0, 0, 0, 1];
  const o = q[3] < 0 ? id.map((v) => -v) : id;
  const c = Math.min(q[0] * o[0] + q[1] * o[1] + q[2] * o[2] + q[3] * o[3], 1);
  const a = Math.acos(c), s = Math.sin(a);
  if (c >= 1 - 1e-7 || s === 0) return q;
  return q.map((v, i) => f((Math.sin((1 - t) * a) * v + Math.sin(t * a) * o[i]) / s));
}

/** Holds the last tracked output after the face is lost, then eases it to neutral. Times are seconds. */
export class FaceLost {
  constructor() { this.last = null; }
  update(tracked, now) {
    if (tracked) {
      this.last = { ...tracked, ready: now };
      return { ...tracked, faceFound: true };
    }
    if (!this.last) return { weights: new Float32Array(52), headRotation: [0, 0, 0, 1], faceFound: false };
    const ease = easeAfterHold(Math.max(f(now - this.last.ready), 0));
    return {
      weights: this.last.weights.map((w) => f(w * f(1 - ease))),
      headRotation: slerpToIdentity(this.last.headRotation, ease),
      faceFound: false,
    };
  }
}
