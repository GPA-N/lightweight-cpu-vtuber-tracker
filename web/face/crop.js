// Port of crates/facelogic: rotated crop rect, warp to the 256x256 landmark input, largest face, detection scheduler.
const f = Math.fround;
const PI = f(Math.PI);

const normalizeRadians = (a) => {
  const t = f(a + PI), m = f(2 * PI);
  return f(f(((t % m) + m) % m) - PI); // rem_euclid
};
const rotation = (p0, p1) => normalizeRadians(f(0 - Math.atan2(-f(p1[1] - p0[1]), f(p1[0] - p0[0]))));

export function rectFromDetection(box, k0, k1, w, h) {
  const [xmin, ymin, bw, bh] = box;
  return {
    cx: f(f(xmin + f(bw / 2)) * w), cy: f(f(ymin + f(bh / 2)) * h),
    width: f(f(1.5 * bw) * w), height: f(f(1.5 * bh) * h),
    angle: rotation([f(k0[0] * w), f(k0[1] * h)], [f(k1[0] * w), f(k1[1] * h)]),
  };
}

/** `norm` is [[x, y] ...] normalised landmarks. */
export function rectFromLandmarks(norm, ia, ib, w, h) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const [x, y] of norm) { x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y); }
  const side = Math.max(f(f(x1 - x0) * w), f(f(y1 - y0) * h));
  const [a, b] = [norm[ia], norm[ib]];
  return {
    cx: f(f(f(x0 + x1) / 2) * w), cy: f(f(f(y0 + y1) / 2) * h), width: f(1.5 * side), height: f(1.5 * side),
    angle: rotation([f(a[0] * w), f(a[1] * h)], [f(b[0] * w), f(b[1] * h)]),
  };
}

export function cropToFrame(u, v, rect, n) {
  const lx = f(f(f(u / n) - 0.5) * rect.width), ly = f(f(f(v / n) - 0.5) * rect.height);
  const s = Math.sin(rect.angle), c = Math.cos(rect.angle);
  return [f(rect.cx + f(f(c * lx) - f(s * ly))), f(rect.cy + f(f(s * lx) + f(c * ly)))];
}

/** NHWC outSize x outSize x 3 tensor data in 0..1: bilinear with replicate border. */
export function warpToTensor(rgb, w, h, rect, n = 256) {
  const out = new Float32Array(n * n * 3);
  const clamp = (v, m) => Math.min(Math.max(v, 0), m - 1);
  let o = 0;
  for (let v = 0; v < n; v++) {
    for (let u = 0; u < n; u++) {
      const [x, y] = cropToFrame(u, v, rect, n);
      const x0 = Math.floor(x), y0 = Math.floor(y), fx = f(x - x0), fy = f(y - y0);
      const [xa, xb, ya, yb] = [clamp(x0, w), clamp(x0 + 1, w), clamp(y0, h), clamp(y0 + 1, h)];
      for (let c = 0; c < 3; c++) {
        const p = (xi, yi) => rgb[(yi * w + xi) * 3 + c];
        const top = f(f(p(xa, ya) * f(1 - fx)) + f(p(xb, ya) * fx));
        const bot = f(f(p(xa, yb) * f(1 - fx)) + f(p(xb, yb) * fx));
        out[o++] = f(f(f(top * f(1 - fy)) + f(bot * fy)) / 255);
      }
    }
  }
  return out;
}

/** Index of the box with the largest area (ties go to the first), or null. `boxes` are [x, y, w, h]. */
export function largest(boxes) {
  let best = null;
  boxes.forEach((b, i) => {
    const a = b[2] > 0 && b[3] > 0 ? f(b[2] * b[3]) : 0;
    if (best === null || a > best[1]) best = [i, a];
  });
  return best && best[0];
}

/** Decides on which frames the face detector runs (every `interval`, and while no face or low presence). */
export class DetectionScheduler {
  constructor(presenceThreshold = 0.5, interval = 5) {
    this.interval = interval;
    this.presenceThreshold = presenceThreshold;
    this.frameIndex = 0;
    this.hasFace = false;
    this.lowPresence = false;
    this.lastDetect = null;
  }
  shouldDetect() {
    const index = this.frameIndex++;
    const due = this.lastDetect === null || index - this.lastDetect >= this.interval;
    const run = due || !this.hasFace || this.lowPresence;
    if (run) this.lastDetect = index;
    return run;
  }
  report(faceHeld, presence = null) {
    this.hasFace = faceHeld;
    this.lowPresence = presence !== null && presence < this.presenceThreshold;
  }
}
