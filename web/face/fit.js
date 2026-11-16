// Port of crates/pose/src/fit.rs fit_head_rotation (weighted similarity fit of 33 landmarks to the canonical face, f64 like the Rust).
import { BASIS_33 } from './data.js';

const RESIDUAL_LIMIT = 0.12;
const mat = () => [[0, 0, 0], [0, 0, 0], [0, 0, 0]];

// One-sided Jacobi SVD of a 3x3 (rows): returns { u, s, v } with columns sorted by descending singular value, A = U diag(s) V^T.
function svd3(h) {
  const a = h.map((r) => r.slice());
  const v = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
  for (let sweep = 0; sweep < 60; sweep++) {
    let off = 0;
    for (const [p, q] of [[0, 1], [0, 2], [1, 2]]) {
      let alpha = 0, beta = 0, gamma = 0;
      for (let i = 0; i < 3; i++) { alpha += a[i][p] ** 2; beta += a[i][q] ** 2; gamma += a[i][p] * a[i][q]; }
      if (Math.abs(gamma) <= 1e-300 || Math.abs(gamma) <= 1e-16 * Math.sqrt(alpha * beta)) continue;
      off = Math.max(off, Math.abs(gamma) / Math.sqrt(alpha * beta));
      const zeta = (beta - alpha) / (2 * gamma);
      const t = Math.sign(zeta || 1) / (Math.abs(zeta) + Math.sqrt(1 + zeta * zeta));
      const c = 1 / Math.sqrt(1 + t * t), s = c * t;
      for (const m of [a, v]) for (let i = 0; i < 3; i++) { const x = m[i][p], y = m[i][q]; m[i][p] = c * x - s * y; m[i][q] = s * x + c * y; }
    }
    if (off < 1e-15) break;
  }
  const sv = [0, 1, 2].map((j) => Math.hypot(a[0][j], a[1][j], a[2][j]));
  const order = [0, 1, 2].sort((x, y) => sv[y] - sv[x]);
  const U = mat(), V = mat();
  order.forEach((j, k) => { for (let i = 0; i < 3; i++) { U[i][k] = sv[j] > 0 ? a[i][j] / sv[j] : 0; V[i][k] = v[i][j]; } });
  return { u: U, s: order.map((j) => sv[j]), v: V };
}

const mm = (A, B) => A.map((r) => [0, 1, 2].map((j) => r[0] * B[0][j] + r[1] * B[1][j] + r[2] * B[2][j]));
const tr = (A) => [0, 1, 2].map((i) => [A[0][i], A[1][i], A[2][i]]);
const det = (m) => m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);

// nalgebra UnitQuaternion::from_rotation_matrix -> [x, y, z, w]
function quatFromMatrix(m) {
  const t = m[0][0] + m[1][1] + m[2][2];
  if (t > 0) {
    const d = Math.sqrt(t + 1) * 2;
    return [(m[2][1] - m[1][2]) / d, (m[0][2] - m[2][0]) / d, (m[1][0] - m[0][1]) / d, 0.25 * d];
  }
  if (m[0][0] > m[1][1] && m[0][0] > m[2][2]) {
    const d = Math.sqrt(1 + m[0][0] - m[1][1] - m[2][2]) * 2;
    return [0.25 * d, (m[0][1] + m[1][0]) / d, (m[0][2] + m[2][0]) / d, (m[2][1] - m[1][2]) / d];
  }
  if (m[1][1] > m[2][2]) {
    const d = Math.sqrt(1 + m[1][1] - m[0][0] - m[2][2]) * 2;
    return [(m[0][1] + m[1][0]) / d, 0.25 * d, (m[1][2] + m[2][1]) / d, (m[0][2] - m[2][0]) / d];
  }
  const d = Math.sqrt(1 + m[2][2] - m[0][0] - m[1][1]) * 2;
  return [(m[0][2] + m[2][0]) / d, (m[1][2] + m[2][1]) / d, 0.25 * d, (m[1][0] - m[0][1]) / d];
}

/** `px` is 478 [x, y, z] frame-pixel points. Returns `{ rotation: [x,y,z,w], scale, residual }` or `{ error }`. */
export function fitHeadRotation(px) {
  if (px.length < 478) return { error: 'TooFewPoints' };
  const to = (p) => [p[0], -p[1], -p[2]]; // F = diag(1, -1, -1)
  if (to(px[4])[2] <= (to(px[33])[2] + to(px[263])[2]) / 2) return { error: 'ZSignAssumptionFailed' };
  const pts = BASIS_33.map(([i, w, c]) => [w, c, to(px[i])]);
  const wSum = pts.reduce((s, p) => s + p[0], 0);
  const cen = (k) => [0, 1, 2].map((d) => pts.reduce((s, p) => s + p[k][d] * p[0], 0) / wSum);
  const cSrc = cen(1), cDst = cen(2);
  const cp = pts.map(([w, s, d]) => [w, [0, 1, 2].map((k) => s[k] - cSrc[k]), [0, 1, 2].map((k) => d[k] - cDst[k])]);
  const H = mat();
  for (const [w, s, d] of cp) for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) H[i][j] += s[i] * d[j] * w;
  const { u, s: sv, v } = svd3(H);
  const dd = Math.sign(det(mm(v, tr(u)))) || 1;
  const R = mm(mm(v, [[1, 0, 0], [0, 1, 0], [0, 0, dd]]), tr(u));
  const srcEnergy = cp.reduce((s, [w, src]) => s + w * (src[0] ** 2 + src[1] ** 2 + src[2] ** 2), 0);
  const scale = (sv[0] + sv[1] + dd * sv[2]) / srcEnergy;
  let err2 = 0;
  for (const [w, src, dst] of cp) {
    const r = [0, 1, 2].map((i) => R[i][0] * src[0] + R[i][1] * src[1] + R[i][2] * src[2]);
    err2 += w * [0, 1, 2].reduce((s, i) => s + (dst[i] - r[i] * scale) ** 2, 0);
  }
  const a = to(px[33]), b = to(px[263]);
  const residual = Math.sqrt(err2 / wSum) / Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
  if (Number.isNaN(residual) || residual > RESIDUAL_LIMIT) return { error: 'FitResidualHigh' };
  const q = quatFromMatrix(R.map((r) => r.map(Math.fround)));
  return { rotation: q, scale, residual };
}
