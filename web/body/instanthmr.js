// Port of crates/infer/src/instant_hmr.rs: InstantHMR v1.1 nano input, camera condition and output decode.
export const JOINTS = 70;
export const INPUT = 288;
export const CROP_EXPAND = 1.2;
export const FOCAL_FALLBACK_DIAG = 1.05;
const MEAN = [0.485, 0.456, 0.406];
const STD = [0.229, 0.224, 0.225];
const f = Math.fround;

// MHR70 joint indices the app reads.
export const joint = {
  NOSE: 0, L_EAR: 3, R_EAR: 4, L_SHOULDER: 5, R_SHOULDER: 6, L_ELBOW: 7, R_ELBOW: 8,
  R_INDEX_KNUCKLE: 28, R_PINKY_KNUCKLE: 40, R_WRIST: 41, L_INDEX_KNUCKLE: 49, L_PINKY_KNUCKLE: 61, L_WRIST: 62,
};

// A square centred on the person box, CROP_EXPAND times its long side (may reach outside the frame).
export function squareCrop(p) {
  const size = Math.max(f(Math.max(p.w, p.h) * f(CROP_EXPAND)), 1);
  return { x0: f(f(p.x + f(p.w / 2)) - f(size / 2)), y0: f(f(p.y + f(p.h / 2)) - f(size / 2)), size };
}

// The person box as the model's camera condition: angles of the box centre off the axis and its long side, over the focal length.
export function cliffCond(p, frameW, frameH) {
  const focal = f(f(FOCAL_FALLBACK_DIAG) * f(Math.hypot(frameW, frameH)));
  const cx = f(p.x + f(p.w / 2));
  const cy = f(p.y + f(p.h / 2));
  return Float32Array.of(
    Math.atan(f(f(cx - f(frameW / 2)) / focal)),
    Math.atan(f(f(cy - f(frameH / 2)) / focal)),
    f(Math.max(p.w, p.h) / focal),
  );
}

// Bilinear resize of the crop to INPUT x INPUT, normalised, planar (CHW). Pixels outside the frame are black.
export function preprocess(rgb, w, h, crop) {
  const plane = INPUT * INPUT;
  const out = new Float32Array(3 * plane);
  const scale = f(crop.size / INPUT);
  const axis = (origin) =>
    Array.from({ length: INPUT }, (_, o) => {
      const s = f(f(origin + f(f(o + 0.5) * scale)) - 0.5);
      return [Math.floor(s), f(s - Math.floor(s))];
    });
  const rows = axis(crop.y0);
  const cols = axis(crop.x0);
  const at = (x, y, c) => (x < 0 || y < 0 || x >= w || y >= h ? 0 : f(rgb[(y * w + x) * 3 + c] / 255));
  rows.forEach(([y0, fy], oy) => {
    cols.forEach(([x0, fx], ox) => {
      for (let c = 0; c < 3; c++) {
        const top = f(f(at(x0, y0, c) * f(1 - fx)) + f(at(x0 + 1, y0, c) * fx));
        const bottom = f(f(at(x0, y0 + 1, c) * f(1 - fx)) + f(at(x0 + 1, y0 + 1, c) * fx));
        const v = f(f(top * f(1 - fy)) + f(bottom * fy));
        out[c * plane + oy * INPUT + ox] = f(f(v - MEAN[c]) / STD[c]);
      }
    });
  });
  return out;
}

// joints2d is [70, 2] in [-1, 1] crop space, joints3d is [70, 3] metres. Returns { xy, xyz } with xy in frame pixels.
export function decode(joints2d, joints3d, crop) {
  const scale = f(crop.size / INPUT);
  const xy = new Float32Array(JOINTS * 2);
  for (let i = 0; i < JOINTS; i++) {
    for (let k = 0; k < 2; k++) {
      const cropPx = f(f(f(joints2d[i * 2 + k] + 1) * 0.5) * INPUT);
      xy[i * 2 + k] = f(f(cropPx * scale) + (k === 0 ? crop.x0 : crop.y0));
    }
  }
  return { xy, xyz: Float32Array.from(joints3d.subarray(0, JOINTS * 3)) };
}
