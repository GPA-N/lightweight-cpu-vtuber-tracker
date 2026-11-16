// Port of crates/avatar/src/body.rs: rig, forward kinematics, the arm/spine/hand solver, BodyFollow and advance_body.
// Quaternions are [x, y, z, w], vectors [x, y, z]; glam's formulas are copied (see README for the conventions).
// Maths runs in doubles on f32 inputs: the Rust uses f32 and the two agree to about 1e-6 (see parity numbers).
const f = Math.fround;
const EPS_F32 = 2 ** -23;
const SPINE_WEIGHTS = [0.3, 0.3, 0.4].map(f);

// ---- vec3 and quat helpers (glam semantics)
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const mul = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const len = (a) => Math.sqrt(dot(a, a));
const isZero = (a) => a[0] === 0 && a[1] === 0 && a[2] === 0;
const ZERO = [0, 0, 0];
// glam normalize_or_zero: zero when the reciprocal length is not finite and positive.
const normOrZero = (a) => {
  const r = 1 / len(a);
  return Number.isFinite(r) && r > 0 ? mul(a, r) : ZERO;
};
const normalize = (a) => mul(a, 1 / len(a));

const qmul = (a, b) => [
  a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
  a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
  a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
  a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
];
const qinv = (q) => [-q[0], -q[1], -q[2], q[3]]; // glam inverse() of a unit quaternion
const qnorm = (q) => {
  const r = 1 / Math.hypot(q[0], q[1], q[2], q[3]);
  return [q[0] * r, q[1] * r, q[2] * r, q[3] * r];
};
const qdot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
// glam Quat * Vec3
const qrot = (q, v) => {
  const w = q[3];
  const b = [q[0], q[1], q[2]];
  return add(add(mul(v, w * w - dot(b, b)), mul(b, dot(v, b) * 2)), mul(cross(b, v), w * 2));
};
const fromAxisAngle = (axis, angle) => {
  const s = Math.sin(angle * 0.5);
  return [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(angle * 0.5)];
};
const fromX = (a) => [Math.sin(a * 0.5), 0, 0, Math.cos(a * 0.5)];
const fromY = (a) => [0, Math.sin(a * 0.5), 0, Math.cos(a * 0.5)];
const fromZ = (a) => [0, 0, Math.sin(a * 0.5), Math.cos(a * 0.5)];
// glam Vec3::any_orthonormal_vector (f32 signum is +-1, 1 for +0).
const anyOrthonormal = (v) => {
  const sign = Object.is(v[2], -0) || v[2] < 0 ? -1 : 1;
  const a = -1 / (sign + v[2]);
  return [v[0] * v[1] * a, sign + v[1] * v[1] * a, -v[1]];
};
// glam Quat::from_rotation_arc: both vectors unit.
const fromRotationArc = (from, to) => {
  const d = dot(from, to);
  const oneMinusEps = 1 - 2 * EPS_F32;
  if (d > oneMinusEps) return [0, 0, 0, 1];
  if (d < -oneMinusEps) return fromAxisAngle(anyOrthonormal(from), Math.PI);
  const c = cross(from, to);
  return qnorm([c[0], c[1], c[2], 1 + d]);
};

// ---- rig
export function buildRig(nodes) {
  const rig = {
    nodes: nodes.map((n) => ({
      name: n.name == null ? null : String(n.name).toLowerCase(),
      parent: n.parent,
      restLocal: n.restLocal,
      restGlobalRot: n.restGlobalRot,
      restGlobalPos: n.restGlobalPos,
    })),
  };
  rig.find = (name) => {
    const i = rig.nodes.findIndex((n) => n.name === name.toLowerCase());
    return i < 0 ? null : i;
  };
  return rig;
}

const parentWorldRest = (rig, i) => qmul(rig.nodes[i].restGlobalRot, qinv(rig.nodes[i].restLocal));

// Current local rotations plus forward kinematics over a rig.
class Pose {
  constructor(rig) {
    this.rig = rig;
    this.locals = rig.nodes.map((n) => n.restLocal.slice());
  }
  parentWorld(i) {
    const p = this.rig.nodes[i].parent;
    return p == null ? parentWorldRest(this.rig, i) : this.world(p);
  }
  world(i) {
    return qmul(this.parentWorld(i), this.locals[i]);
  }
  setWorld(i, world) {
    this.locals[i] = qmul(qinv(this.parentWorld(i)), world);
  }
  dirLocal(bone, child) {
    const b = this.rig.nodes[bone];
    const c = this.rig.nodes[child];
    return qrot(qinv(b.restGlobalRot), normOrZero(sub(c.restGlobalPos, b.restGlobalPos)));
  }
  worldDir(bone, child) {
    return qrot(this.world(bone), this.dirLocal(bone, child));
  }
  // Swings `bone` so that its direction towards `child` points along `target`.
  aim(bone, child, target) {
    const current = this.worldDir(bone, child);
    const swing = fromRotationArc(current, normalize(target));
    this.setWorld(bone, qmul(swing, this.world(bone)));
  }
}

// ---- hand maths
export function palmNormal(left, wrist, middle, index, little) {
  const u = sub(middle, wrist);
  const v = sub(index, little);
  return normOrZero(left ? cross(u, v) : cross(v, u));
}

// Finger flexion angles in radians, rows thumb, index, middle, ring, little; columns the three joints from the base.
export function curlAngles(p) {
  const out = [];
  for (let fi = 0; fi < 5; fi++) {
    const base = fi === 0 ? 1 : 5 + (fi - 1) * 4;
    const chain = [0, base, base + 1, base + 2, base + 3];
    const row = [];
    for (let j = 0; j < 3; j++) {
      const a = normOrZero(sub(p[chain[j + 1]], p[chain[j]]));
      const b = normOrZero(sub(p[chain[j + 2]], p[chain[j + 1]]));
      row.push(isZero(a) || isZero(b) ? 0 : Math.acos(Math.min(Math.max(dot(a, b), -1), 1)));
    }
    out.push(row);
  }
  return out;
}

const FINGERS = [
  ['Thumb', ['Metacarpal', 'Proximal', 'Distal']],
  ['Index', ['Proximal', 'Intermediate', 'Distal']],
  ['Middle', ['Proximal', 'Intermediate', 'Distal']],
  ['Ring', ['Proximal', 'Intermediate', 'Distal']],
  ['Little', ['Proximal', 'Intermediate', 'Distal']],
];

function torsoRoll(pose) {
  const l = pose.left.shoulder, r = pose.right.shoulder;
  if (!l || !r) return null;
  const d = sub(l, r);
  if (Math.abs(d[0]) < 1e-3) return null;
  return Math.atan2(d[1], d[0]);
}
function torsoYaw(pose) {
  const l = pose.left.shoulder, r = pose.right.shoulder;
  if (!l || !r) return null;
  const d = sub(l, r);
  if (Math.abs(d[0]) < 1e-3) return null;
  return Math.atan2(-d[2], Math.abs(d[0]));
}
function torsoLean(pose) {
  const l = pose.left.shoulder, r = pose.right.shoulder, head = pose.head;
  if (!l || !r || !head) return null;
  const v = sub(head, mul(add(l, r), 0.5));
  if (v[1] <= 0) return null;
  return Math.atan2(v[2], v[1]);
}

// Signed angle from `a` to `b` about unit `axis`, both projected onto the plane normal to `axis`.
function signedAngleAbout(axis, a, b) {
  a = sub(a, mul(axis, dot(a, axis)));
  b = sub(b, mul(axis, dot(b, axis)));
  if (dot(a, a) < 1e-8 || dot(b, b) < 1e-8) return 0;
  return Math.atan2(dot(cross(a, b), axis), dot(a, b));
}

// Upper-arm and forearm directions from a two-bone solve that puts the avatar wrist where the tracked wrist is.
export function reachDirs(rig, upper, lower, hand, s, e, w) {
  const ps = rig.nodes[upper].restGlobalPos, pe = rig.nodes[lower].restGlobalPos, pw = rig.nodes[hand].restGlobalPos;
  const l1 = len(sub(pe, ps)), l2 = len(sub(pw, pe));
  const t1 = len(sub(e, s)), t2 = len(sub(w, e));
  if (l1 < 1e-4 || l2 < 1e-4 || t1 < 1e-4 || t2 < 1e-4) return null;
  const target = mul(sub(w, s), (l1 + l2) / (t1 + t2));
  const dist = len(target);
  if (dist < 1e-4) return null;
  const u = mul(target, 1 / dist);
  const d = Math.min(dist, (l1 + l2) * 0.999);
  const off = sub(e, s);
  const side = sub(off, mul(u, dot(off, u)));
  if (!(len(side) > 1e-4)) return null;
  const pole = normalize(side);
  const a = (l1 * l1 - l2 * l2 + d * d) / (2 * d);
  const h = Math.sqrt(Math.max(l1 * l1 - a * a, 0));
  const elbow = add(mul(u, a), mul(pole, h));
  return [normOrZero(elbow), normOrZero(sub(mul(u, d), elbow))];
}

function solveArm(p, side, arm, report) {
  const left = side === 'left';
  const rig = p.rig;
  const n = (part) => rig.find(side + part);
  const upper = n('UpperArm'), lower = n('LowerArm'), hand = n('Hand');
  const { shoulder: s, elbow: e, wrist: w } = arm;

  const reach = upper != null && lower != null && hand != null && s && e && w ? reachDirs(rig, upper, lower, hand, s, e, w) : null;
  if (upper != null && lower != null && s && e) {
    const t = reach ? reach[0] : normOrZero(sub(e, s));
    if (!isZero(t)) {
      p.aim(upper, lower, t);
      report.push({ bone: side + 'UpperArm', target: t, achieved: p.worldDir(upper, lower) });
    }
  }
  if (lower != null && hand != null && e && w) {
    const t = reach ? reach[1] : normOrZero(sub(w, e));
    if (!isZero(t)) {
      p.aim(lower, hand, t);
      report.push({ bone: side + 'LowerArm', target: t, achieved: p.worldDir(lower, hand) });
    }
  }

  // A hand is only driven when its forearm was.
  const forearmDriven = report.some((r) => r.bone === side + 'LowerArm');
  if (hand == null || !forearmDriven) return;
  const h = hand;
  const fingerRoot = (name) => rig.find(`${side}${name}Proximal`);
  const midB = fingerRoot('Middle'), idxB = fingerRoot('Index'), litB = fingerRoot('Little');
  const restPos = (i) => rig.nodes[i].restGlobalPos;
  const restNormal = midB != null && idxB != null && litB != null ? palmNormal(left, restPos(h), restPos(midB), restPos(idxB), restPos(litB)) : null;

  // Hand direction and palm roll come from the 21-point hand model when present; the pose index and pinky are the fallback.
  let handFrame = null;
  if (arm.hand) handFrame = [arm.hand[0], arm.hand[5], arm.hand[17], arm.hand[9]];
  else if (arm.wrist && arm.index && arm.pinky) handFrame = [arm.wrist, arm.index, arm.pinky, mul(add(arm.index, arm.pinky), 0.5)];
  if (handFrame) {
    const [wr, ix, pk, mid] = handFrame;
    const t = normOrZero(sub(mid, wr));
    const child = midB ?? idxB ?? litB;
    if (child != null && !isZero(t)) {
      p.aim(h, child, t);
      if (restNormal && !isZero(restNormal)) {
        const targetN = palmNormal(left, wr, mid, ix, pk);
        if (!isZero(targetN)) {
          const curN = qrot(p.world(h), qrot(qinv(rig.nodes[h].restGlobalRot), restNormal));
          const angle = signedAngleAbout(t, curN, targetN);
          p.setWorld(h, qmul(fromAxisAngle(t, angle), p.world(h)));
        }
      }
      report.push({ bone: side + 'Hand', target: t, achieved: p.worldDir(h, child) });
    }
  }

  if (arm.hand && restNormal && !isZero(restNormal)) {
    const pts = arm.hand;
    const nr = restNormal;
    const angles = curlAngles(pts);
    FINGERS.forEach(([finger, joints], fi) => {
      const bones = joints.map((j) => rig.find(`${side}${finger}${j}`));
      // The thumb swings across the palm: its first two bones aim along the tracked segments, only the last joint curls.
      const firstCurled = fi === 0 ? 2 : 0;
      if (fi === 0) {
        if (bones[0] != null && bones[1] != null) {
          const t = normOrZero(sub(pts[2], pts[1]));
          if (!isZero(t)) p.aim(bones[0], bones[1], t);
        }
        if (bones[1] != null && bones[2] != null) {
          const t = normOrZero(sub(pts[3], pts[2]));
          if (!isZero(t)) p.aim(bones[1], bones[2], t);
        }
      }
      for (let j = firstCurled; j < bones.length; j++) {
        const b = bones[j];
        if (b == null) continue;
        let dir;
        if (j + 1 < bones.length && bones[j + 1] != null) dir = normOrZero(sub(restPos(bones[j + 1]), restPos(b)));
        else if (j > 0) dir = bones[j - 1] != null ? normOrZero(sub(restPos(b), restPos(bones[j - 1]))) : ZERO;
        else dir = normOrZero(sub(restPos(b), restPos(h)));
        const axisWorld = normOrZero(cross(dir, nr));
        if (isZero(axisWorld)) continue;
        const axisLocal = qrot(qinv(rig.nodes[b].restGlobalRot), axisWorld);
        p.locals[b] = qmul(p.locals[b], fromAxisAngle(axisLocal, angles[fi][j]));
      }
    });
  }
}

/** Solves the whole upper body. `locals` has one rotation per rig node (rest where untouched); `changed` lists
 *  only the `[nodeIndex, quat]` that differ from rest, which is what Rust's `Solved.locals` is and what `advanceBody` takes. */
export function solve(rig, pose) {
  const p = new Pose(rig);
  const report = [];

  const roll = torsoRoll(pose), yaw = torsoYaw(pose), lean = torsoLean(pose);
  if (roll !== null || yaw !== null || lean !== null) {
    const [r, y, l] = [roll ?? 0, yaw ?? 0, lean ?? 0];
    const names = ['spine', 'chest', 'upperChest'];
    const bones = names.map((nm) => rig.find(nm)).filter((i) => i != null);
    const weights = names.map((nm, k) => (rig.find(nm) != null ? SPINE_WEIGHTS[k] : null)).filter((w) => w != null);
    const total = weights.reduce((a, b) => f(a + b), 0);
    let acc = 0;
    bones.forEach((b, k) => {
      acc = f(acc + f(weights[k] / total));
      const world = qmul(qmul(qmul(fromY(y * acc), fromX(l * acc)), fromZ(r * acc)), rig.nodes[b].restGlobalRot);
      p.setWorld(b, world);
    });
  }

  solveArm(p, 'left', pose.left, report);
  solveArm(p, 'right', pose.right, report);

  const changed = [];
  p.locals.forEach((q, i) => {
    if (Math.abs(qdot(q, rig.nodes[i].restLocal)) < f(1 - 1e-7)) changed.push([i, q]);
  });
  return { locals: p.locals, changed, report };
}

// ---- following: One Euro on the quaternion components, window.rs QuatSmoother
const BODY_FILTER_MIN_CUTOFF_HZ = 1;
const BODY_FILTER_BETA = 4;
const BODY_FILTER_DERIV_CUTOFF_HZ = 1;
const lowpassAlpha = (cutoff, dt) => {
  const tau = f(1 / f(f(2 * f(Math.PI)) * f(cutoff)));
  return f(1 / f(1 + f(tau / dt)));
};

class QuatSmoother {
  constructor() {
    this.out = null;
    this.prev = [0, 0, 0, 1];
    this.dx = [0, 0, 0, 0];
  }
  filter(q, dt) {
    q = q.map(f);
    dt = f(dt);
    if (this.out === null) {
      this.out = q;
      this.prev = q;
      return q;
    }
    if (dt <= 0) return this.out;
    const out = this.out;
    if (f(qdot32(q, out)) < 0) q = q.map((v) => -v);
    const a = lowpassAlpha(BODY_FILTER_DERIV_CUTOFF_HZ, dt);
    for (let k = 0; k < 4; k++) {
      const dx = f(f(q[k] - this.prev[k]) / dt);
      this.dx[k] = f(this.dx[k] + f(f(dx - this.dx[k]) * a));
    }
    this.prev = q;
    const dxLen = f(Math.sqrt(f(f(f(f(this.dx[0] * this.dx[0]) + f(this.dx[1] * this.dx[1])) + f(this.dx[2] * this.dx[2])) + f(this.dx[3] * this.dx[3]))));
    const cutoff = f(BODY_FILTER_MIN_CUTOFF_HZ + f(f(BODY_FILTER_BETA) * dxLen));
    const al = lowpassAlpha(cutoff, dt);
    const o = [0, 1, 2, 3].map((k) => f(out[k] + f(f(q[k] - out[k]) * al)));
    const n = f(Math.sqrt(f(f(f(f(o[0] * o[0]) + f(o[1] * o[1])) + f(o[2] * o[2])) + f(o[3] * o[3]))));
    const r = f(1 / n);
    this.out = o.map((v) => f(v * r));
    return this.out;
  }
}
const qdot32 = (a, b) => f(f(f(f(a[0] * b[0]) + f(a[1] * b[1])) + f(a[2] * b[2])) + f(a[3] * b[3]));

export class BodyFollow {
  constructor() {
    this.bones = []; // { index, rest, isHand, current, smoother }
  }
}

/** Advances the live body by `dt`. `target` is `[nodeIndex, quat][]` (the solver's `changed`); other tracked bones keep their last pose.
 *  `rest(index)` returns `[restLocalQuat, isHand]`. Returns `[nodeIndex, quat][]` for every tracked bone. */
export function advanceBody(follow, target, rest, dt) {
  for (const [index] of target) {
    if (!follow.bones.some((b) => b.index === index)) {
      const [restRot, isHand] = rest(index);
      follow.bones.push({ index, rest: restRot, isHand, current: restRot, smoother: new QuatSmoother() });
    }
  }
  for (const bone of follow.bones) {
    const t = target.find(([i]) => i === bone.index);
    if (t) bone.current = bone.smoother.filter(t[1], dt);
  }
  return follow.bones.map((b) => [b.index, b.current]);
}

export const isHandBone = (name) => name != null && ['hand', 'thumb', 'index', 'middle', 'ring', 'little'].some((p) => name.includes(p));
