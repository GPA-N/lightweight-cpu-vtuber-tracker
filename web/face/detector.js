// Port of crates/infer/src/detector.rs: BlazeFace-style SSD (128x128 letterbox, 896 anchors, weighted NMS).
export const INPUT_SIZE = 128;
export const MIN_SCORE = 0.5;
export const NMS_IOU = 0.3;
const f = Math.fround;

export function letterboxFor(w, h) {
  const s = f(INPUT_SIZE / Math.max(w, h));
  const spanX = f(f(w * s) / INPUT_SIZE);
  const spanY = f(f(h * s) / INPUT_SIZE);
  return { padX: f(f(1 - spanX) / 2), padY: f(f(1 - spanY) / 2), spanX, spanY };
}

/** NHWC 128x128x3 tensor data, RGB, (v/255)*2-1, black border; and the letterbox. */
export function preprocess(rgb, w, h) {
  const lb = letterboxFor(w, h);
  const s = f(INPUT_SIZE / Math.max(w, h));
  const [ox, oy, cw, ch] = [f(lb.padX * INPUT_SIZE), f(lb.padY * INPUT_SIZE), f(lb.spanX * INPUT_SIZE), f(lb.spanY * INPUT_SIZE)];
  const out = new Float32Array(INPUT_SIZE * INPUT_SIZE * 3); // border stays 0 and normalises to -1 below
  for (let dy = 0; dy < INPUT_SIZE; dy++) {
    if (dy < oy || dy >= f(oy + ch)) continue;
    const sy = Math.min(Math.max(f(f(dy - oy) / s), 0), h - 1);
    const y0 = Math.floor(sy), y1 = Math.min(y0 + 1, h - 1), fy = f(sy - y0);
    for (let dx = 0; dx < INPUT_SIZE; dx++) {
      if (dx < ox || dx >= f(ox + cw)) continue;
      const sx = Math.min(Math.max(f(f(dx - ox) / s), 0), w - 1);
      const x0 = Math.floor(sx), x1 = Math.min(x0 + 1, w - 1), fx = f(sx - x0);
      for (let c = 0; c < 3; c++) {
        const p = (x, y) => rgb[(y * w + x) * 3 + c];
        const top = f(f(p(x0, y0) * f(1 - fx)) + f(p(x1, y0) * fx));
        const bot = f(f(p(x0, y1) * f(1 - fx)) + f(p(x1, y1) * fx));
        out[(dy * INPUT_SIZE + dx) * 3 + c] = f(f(top * f(1 - fy)) + f(bot * fy));
      }
    }
  }
  for (let i = 0; i < out.length; i++) out[i] = f(f(f(out[i] / 255) * 2) - 1);
  return { data: out, lb };
}

/** The 896 anchor centres: stride 8 (2 per cell) then stride 16 (6 per cell), y-major. */
export function generateAnchors() {
  const a = [];
  for (const [stride, per] of [[8, 2], [16, 6]]) {
    const fm = Math.ceil(INPUT_SIZE / stride);
    for (let y = 0; y < fm; y++) for (let x = 0; x < fm; x++) for (let k = 0; k < per; k++) a.push([f((x + 0.5) / fm), f((y + 0.5) / fm)]);
  }
  return a;
}

const sigmoid = (x) => f(1 / f(1 + Math.exp(-Math.min(Math.max(x, -100), 100))));

const iou = (a, b) => {
  const ix = Math.min(a[0] + a[2], b[0] + b[2]) - Math.max(a[0], b[0]);
  const iy = Math.min(a[1] + a[3], b[1] + b[3]) - Math.max(a[1], b[1]);
  if (ix <= 0 || iy <= 0) return 0;
  const inter = ix * iy;
  return inter / (a[2] * a[3] + b[2] * b[3] - inter);
};

// Best first; every candidate with IoU > threshold joins the best's cluster; the result is the score-weighted average.
function weightedNms(pending, thr) {
  pending = pending.slice().sort((a, b) => b.score - a.score);
  const out = [];
  while (pending.length) {
    const best = pending.shift();
    const members = [best];
    pending = pending.filter((c) => (iou(best.bbox, c.bbox) > thr ? (members.push(c), false) : true));
    const total = members.reduce((s, c) => s + c.score, 0);
    const bbox = [0, 0, 0, 0];
    const kp = Array.from({ length: 6 }, () => [0, 0]);
    for (const c of members) {
      const w = total > 0 ? c.score / total : 1 / members.length;
      for (let i = 0; i < 4; i++) bbox[i] += w * c.bbox[i];
      c.kp.forEach((p, k) => { kp[k][0] += w * p[0]; kp[k][1] += w * p[1]; });
    }
    out.push({ score: best.score, bbox, kp });
  }
  return out;
}

/** Faces `{ box: [xmin, ymin, w, h], keypoints: [[x, y] x6], score }` normalised to the frame, best score first. */
export function decode(regs, scores, anchors, lb, minScore = MIN_SCORE) {
  const cands = [];
  scores.forEach((logit, i) => {
    const score = sigmoid(logit);
    if (score < minScore) return;
    const r = regs.subarray(i * 16, i * 16 + 16);
    const [ax, ay] = anchors[i];
    const cx = f(f(r[0] / INPUT_SIZE) + ax), cy = f(f(r[1] / INPUT_SIZE) + ay);
    const w = f(r[2] / INPUT_SIZE), h = f(r[3] / INPUT_SIZE);
    const kp = [0, 1, 2, 3, 4, 5].map((k) => [f(f(r[4 + 2 * k] / INPUT_SIZE) + ax), f(f(r[5 + 2 * k] / INPUT_SIZE) + ay)]);
    cands.push({ score, bbox: [f(cx - f(w / 2)), f(cy - f(h / 2)), w, h], kp });
  });
  const map = (x, y) => [f(f(x - lb.padX) / lb.spanX), f(f(y - lb.padY) / lb.spanY)];
  return weightedNms(cands, NMS_IOU).map((c) => {
    const [x0, y0] = map(c.bbox[0], c.bbox[1]);
    const [x1, y1] = map(c.bbox[0] + c.bbox[2], c.bbox[1] + c.bbox[3]);
    return { box: [x0, y0, f(x1 - x0), f(y1 - y0)], keypoints: c.kp.map(([x, y]) => map(x, y)), score: c.score };
  });
}
