// Port of crates/infer/src/person_detector.rs: RF-DETR nano person box.
export const INPUT = 384;
export const PERSON_CLASS = 1;
export const MIN_CONFIDENCE = 0.5;
export const REUSE_EXPAND = 0.1;
export const DETECTOR_STRIDE = 30;
const MEAN = [0.485, 0.456, 0.406];
const STD = [0.229, 0.224, 0.225];
const f = Math.fround;

// Squashes the whole frame to INPUT x INPUT (bilinear), normalised, planar (CHW). `rgb` is w*h*3 bytes.
export function preprocess(rgb, w, h) {
  const plane = INPUT * INPUT;
  const out = new Float32Array(3 * plane);
  const axis = (len) => {
    const scale = f(len / INPUT);
    return Array.from({ length: INPUT }, (_, o) => {
      const s = Math.min(Math.max(f(f(f(o + 0.5) * scale) - 0.5), 0), len - 1);
      const i0 = Math.floor(s);
      return [i0, Math.min(i0 + 1, len - 1), f(s - i0)];
    });
  };
  const rows = axis(h);
  const cols = axis(w);
  rows.forEach(([y0, y1, fy], oy) => {
    cols.forEach(([x0, x1, fx], ox) => {
      for (let c = 0; c < 3; c++) {
        const at = (x, y) => f(rgb[(y * w + x) * 3 + c] / 255);
        const top = f(f(at(x0, y0) * f(1 - fx)) + f(at(x1, y0) * fx));
        const bottom = f(f(at(x0, y1) * f(1 - fx)) + f(at(x1, y1) * fx));
        const v = f(f(top * f(1 - fy)) + f(bottom * fy));
        out[c * plane + oy * INPUT + ox] = f(f(v - MEAN[c]) / STD[c]);
      }
    });
  });
  return out;
}

const sigmoid = (x) => f(1 / f(1 + Math.exp(-x)));

// The most confident person at or above MIN_CONFIDENCE, in frame pixels, or null.
// `dets` is [Q, 4] (cx, cy, w, h normalised), `labels` is [Q, classes] logits.
export function decode(dets, labels, classes, frameW, frameH) {
  if (classes <= PERSON_CLASS) throw new Error('labels output has no person class');
  let best = null;
  for (let q = 0; q < labels.length / classes; q++) {
    const score = sigmoid(labels[q * classes + PERSON_CLASS]);
    if (score >= MIN_CONFIDENCE && (best === null || score > best[0])) best = [score, q];
  }
  if (best === null) return null;
  const d = dets.subarray(best[1] * 4, best[1] * 4 + 4);
  const x0 = Math.max(f(f(d[0] - f(d[2] / 2)) * frameW), 0);
  const y0 = Math.max(f(f(d[1] - f(d[3] / 2)) * frameH), 0);
  const x1 = Math.min(f(f(d[0] + f(d[2] / 2)) * frameW), frameW);
  const y1 = Math.min(f(f(d[1] + f(d[3] / 2)) * frameH), frameH);
  return x1 > x0 && y1 > y0 ? { x: x0, y: y0, w: f(x1 - x0), h: f(y1 - y0) } : null;
}

// `b` grown by REUSE_EXPAND of its size on each side and clamped to the frame.
export function expand(b, frameW, frameH) {
  const x0 = Math.max(f(b.x - f(b.w * REUSE_EXPAND)), 0);
  const y0 = Math.max(f(b.y - f(b.h * REUSE_EXPAND)), 0);
  const x1 = Math.min(f(b.x + f(b.w * f(1 + REUSE_EXPAND))), frameW);
  const y1 = Math.min(f(b.y + f(b.h * f(1 + REUSE_EXPAND))), frameH);
  return { x: x0, y: y0, w: f(x1 - x0), h: f(y1 - y0) };
}
