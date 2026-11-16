// Comparison helpers shared by parity.mjs (Chrome run) and units.test.mjs (plain node).
import fs from 'node:fs';
import path from 'node:path';

export const FIX = process.env.WEB_FIXTURES ?? 'C:/Users/Lenovo/web-fixtures';
export const STILLS = ['photo_claws', 'photo_fists', 'photo_thumbs', 'priv_a', 'priv_b', 'priv_c', 'blank'];
export const SEQ_LEN = 60;
export const readJson = (rel) => JSON.parse(fs.readFileSync(path.join(FIX, rel), 'utf8'));
export const seqName = (k) => String(k).padStart(3, '0');

/** Max abs difference of two same-shape nested numeric arrays. A null on one side only counts as Infinity. */
export function maxDiff(a, b) {
  if (a === null || b === null || a === undefined || b === undefined) return a == b ? 0 : Infinity;
  if (typeof a === 'number' || typeof b === 'number') return Math.abs(a - b);
  if (a.length !== b.length) return Infinity;
  let m = 0;
  for (let i = 0; i < a.length; i++) m = Math.max(m, maxDiff(a[i], b[i]));
  return m;
}

export const boxArr = (b) => (b ? [b.x, b.y, b.w, b.h] : null);

// Angle in degrees between two rotations (q and -q are the same rotation).
export function angleDeg(a, b) {
  const [x, y, z, w] = [-b[0], -b[1], -b[2], b[3]];
  const r = [
    a[3] * x + a[0] * w + a[1] * z - a[2] * y,
    a[3] * y - a[0] * z + a[1] * w + a[2] * x,
    a[3] * z + a[0] * y - a[1] * x + a[2] * w,
    a[3] * w - a[0] * x - a[1] * y - a[2] * z,
  ];
  return ((2 * Math.atan2(Math.hypot(r[0], r[1], r[2]), Math.abs(r[3]))) * 180) / Math.PI;
}

/** Fixture arm pose `{shoulder, ...}` and BodyPose compare, per component; Infinity when a joint is null on one side only. */
export function bodyPoseDiff(a, b) {
  let m = 0;
  for (const side of ['left', 'right']) {
    for (const k of ['shoulder', 'elbow', 'wrist', 'index', 'pinky', 'hand']) m = Math.max(m, maxDiff(a[side][k], b[side][k]));
  }
  return Math.max(m, maxDiff(a.head, b.head));
}

/** Rust `Solved.locals` (`[{node, rotation}]`) as one rotation per rig node (rest where not listed). */
export function fixtureLocals(rig, listed) {
  const out = rig.nodes.map((n) => n.restLocal);
  for (const { node, rotation } of listed) out[node] = rotation;
  return out;
}

/** Max angle (deg) over rig nodes between two full local-rotation arrays. */
export function maxAngle(a, b) {
  return a.reduce((m, q, i) => Math.max(m, angleDeg(q, b[i])), 0);
}
