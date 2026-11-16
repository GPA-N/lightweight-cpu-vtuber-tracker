// Port of crates/infer/src/upper_body.rs (Mp2023Hands): palm detection decode, hand ROIs, landmark decode, side assignment.
// Rects are { cx, cy, width, height, angle } in frame pixels; images are { rgb, w, h }.
export const PALM_SIZE = 192;
export const HAND_LM_SIZE = 224;
export const DET_SCORE_MIN = 0.5;
export const NMS_IOU = 0.3;
const f = Math.fround;
const sigmoid = (x) => f(1 / f(1 + Math.exp(-Math.min(Math.max(x, -100), 100))));

// facelogic crop_to_frame
export function cropToFrame(u, v, r, n) {
  const lx = f(f(f(u / n) - 0.5) * r.width), ly = f(f(f(v / n) - 0.5) * r.height);
  const s = Math.sin(r.angle), c = Math.cos(r.angle);
  return [f(r.cx + f(f(c * lx) - f(s * ly))), f(r.cy + f(f(s * lx) + f(c * ly)))];
}

/** Warps `rect` to an out x out NHWC RGB tensor in 0..1, black outside the image. */
export function warpBlack(img, rect, out) {
  const { rgb, w, h } = img;
  const t = new Float32Array(out * out * 3);
  const px = (x, y, c) => (x < 0 || y < 0 || x >= w || y >= h ? 0 : rgb[(y * w + x) * 3 + c]);
  let o = 0;
  for (let v = 0; v < out; v++) {
    for (let u = 0; u < out; u++) {
      const [x, y] = cropToFrame(u, v, rect, out);
      const x0 = Math.floor(x), y0 = Math.floor(y), fx = f(x - x0), fy = f(y - y0);
      for (let c = 0; c < 3; c++) {
        const top = f(f(px(x0, y0, c) * f(1 - fx)) + f(px(x0 + 1, y0, c) * fx));
        const bot = f(f(px(x0, y0 + 1, c) * f(1 - fx)) + f(px(x0 + 1, y0 + 1, c) * fx));
        t[o++] = f(f(f(top * f(1 - fy)) + f(bot * fy)) / 255);
      }
    }
  }
  return t;
}

export const letterboxRect = (w, h) => ({ cx: f(w / 2), cy: f(h / 2), width: Math.max(w, h), height: Math.max(w, h), angle: 0 });

/** SSD anchor centres; equal-stride layers merge into one grid with two anchors per layer per cell. */
export function ssdAnchors(input, strides) {
  const out = [];
  for (let i = 0; i < strides.length;) {
    let layers = 0;
    while (i + layers < strides.length && strides[i + layers] === strides[i]) layers++;
    const fm = Math.ceil(input / strides[i]);
    for (let y = 0; y < fm; y++) for (let x = 0; x < fm; x++) for (let k = 0; k < layers * 2; k++) out.push([f((x + 0.5) / fm), f((y + 0.5) / fm)]);
    i += layers;
  }
  return out;
}

/** `boxes` [n * (4 + 2kp)], `scores` [n] logits. Returns `{ score, cx, cy, w, h, keypoints: [[x, y]...] }` above minScore. */
export function decodeDetections(boxes, scores, anchors, input, kp, minScore) {
  const stride = 4 + 2 * kp;
  if (boxes.length !== anchors.length * stride || scores.length !== anchors.length) throw new Error('detector output sizes do not match the anchors');
  const out = [];
  anchors.forEach(([ax, ay], i) => {
    const score = sigmoid(scores[i]);
    if (score < minScore) return;
    const b = boxes.subarray(i * stride, (i + 1) * stride);
    out.push({
      score, cx: f(f(b[0] / input) + ax), cy: f(f(b[1] / input) + ay), w: f(b[2] / input), h: f(b[3] / input),
      keypoints: Array.from({ length: kp }, (_, k) => [f(f(b[4 + 2 * k] / input) + ax), f(f(b[5 + 2 * k] / input) + ay)]),
    });
  });
  return out;
}

function iou(a, b) {
  const [ax0, ax1, ay0, ay1] = [a.cx - a.w / 2, a.cx + a.w / 2, a.cy - a.h / 2, a.cy + a.h / 2];
  const [bx0, bx1, by0, by1] = [b.cx - b.w / 2, b.cx + b.w / 2, b.cy - b.h / 2, b.cy + b.h / 2];
  const inter = Math.max(Math.min(ax1, bx1) - Math.max(ax0, bx0), 0) * Math.max(Math.min(ay1, by1) - Math.max(ay0, by0), 0);
  const union = a.w * a.h + b.w * b.h - inter;
  return union <= 0 ? 0 : inter / union;
}

/** Greedy non-maximum suppression, best score first. */
export function nms(dets, iouMax) {
  const kept = [];
  for (const d of dets.slice().sort((a, b) => b.score - a.score)) if (kept.every((k) => iou(k, d) <= iouMax)) kept.push(d);
  return kept;
}

const upAngle = (a, b) => Math.atan2(f(b[0] - a[0]), -f(b[1] - a[1]));
const fromLetterbox = (p, w, h) => {
  const s = Math.max(w, h);
  return [f(f(p[0] * s) - f(f(s - w) / 2)), f(f(p[1] * s) - f(f(s - h) / 2))];
};

/** Hand ROI from a palm detection (MediaPipe palm_detection_detection_to_roi), or null. */
export function palmRoi(det, w, h) {
  if (det.keypoints.length < 3) return null;
  const p0 = fromLetterbox(det.keypoints[0], w, h), p2 = fromLetterbox(det.keypoints[2], w, h);
  const s = Math.max(w, h);
  const [bw, bh] = [f(det.w * s), f(det.h * s)];
  const angle = f(upAngle(p0, p2));
  const c = fromLetterbox([det.cx, det.cy], w, h);
  const shift = f(-0.5 * bh);
  const side = f(Math.max(bw, bh) * f(2.6));
  return side > 1 ? { cx: f(c[0] - f(shift * Math.sin(angle))), cy: f(c[1] + f(shift * Math.cos(angle))), width: side, height: side, angle } : null;
}

/** Hand crop from the pose arm points when the palm detector finds nothing, or null. */
export function handRect(elbow, wrist, index, pinky) {
  const forearm = f(Math.sqrt(f(f(f(wrist.x - elbow.x) ** 2) + f(f(wrist.y - elbow.y) ** 2))));
  const [kx, ky] = [f(f(f(index.x + pinky.x) / 2) - wrist.x), f(f(f(index.y + pinky.y) / 2) - wrist.y)];
  const knuckle = f(Math.sqrt(f(f(kx * kx) + f(ky * ky))));
  let dir, len;
  if (knuckle >= f(0.12 * forearm) && knuckle > 2) [dir, len] = [[f(kx / knuckle), f(ky / knuckle)], f(2 * knuckle)];
  else if (forearm > 4) [dir, len] = [[f(f(wrist.x - elbow.x) / forearm), f(f(wrist.y - elbow.y) / forearm)], f(0.7 * forearm)];
  else return null;
  const side = Math.max(f(1.5 * len), 32);
  return { cx: f(wrist.x + f(dir[0] * f(0.45 * len))), cy: f(wrist.y + f(dir[1] * f(0.45 * len))), width: side, height: side, angle: f(Math.atan2(dir[0], -dir[1])) };
}

/** Hand points `[[x,y,z] x21]` from model input pixels to frame pixels; z scales with the crop. */
export const handToFrame = (points, rect, input) =>
  points.map((p) => [...cropToFrame(p[0], p[1], rect, input), f(f(p[2] / input) * rect.width)]);

const dist = (a, b) => Math.sqrt(f(f(f(a[0] - b[0]) ** 2) + f(f(a[1] - b[1]) ** 2)));

/** Side per hand from the pose wrists (slots 15 left, 16 right), minimising the total distance. */
export function assignSides(wrists, pose) {
  const l = [pose[15].x, pose[15].y], r = [pose[16].x, pose[16].y];
  if (wrists.length === 0) return [];
  const [a, b] = wrists;
  if (wrists.length === 1) return [dist(a, l) <= dist(a, r) ? 'left' : 'right'];
  return f(dist(a, l) + dist(b, r)) <= f(dist(a, r) + dist(b, l)) ? ['left', 'right'] : ['right', 'left'];
}
