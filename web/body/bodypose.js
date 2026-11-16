// Port of avatar::body::to_avatar_space and hmr_body.rs drop_arms_without_elbow / pose_landmarks.
import { joint } from './instanthmr.js';

const f = Math.fround;

// Pose slot (33-point layout, BlazePose order) and InstantHMR joint for each landmark the avatar solver reads.
export const SLOTS = [
  [7, joint.L_EAR], [8, joint.R_EAR], [11, joint.L_SHOULDER], [12, joint.R_SHOULDER], [13, joint.L_ELBOW], [14, joint.R_ELBOW],
  [15, joint.L_WRIST], [16, joint.R_WRIST], [17, joint.L_PINKY_KNUCKLE], [18, joint.R_PINKY_KNUCKLE],
  [19, joint.L_INDEX_KNUCKLE], [20, joint.R_INDEX_KNUCKLE], [0, joint.NOSE],
];

// 33 PoseLandmark: pixels plus metric world position for the joints the solver reads, zeros elsewhere.
export function poseLandmarks(xy, xyz) {
  const pose = Array.from({ length: 33 }, () => ({ x: 0, y: 0, z: 0, visibility: 0, world: [0, 0, 0] }));
  for (const [slot, m] of SLOTS) {
    pose[slot] = { x: xy[m * 2], y: xy[m * 2 + 1], z: 0, visibility: 1, world: [xyz[m * 3], xyz[m * 3 + 1], xyz[m * 3 + 2]] };
  }
  return pose;
}

/** Image coordinates (x right, y down, z away) to avatar space (+x avatar left, +y up, +z to the viewer).
 *  `pose` is PoseLandmark[33], `hands` is [{ side, points, score }] (smoothed, pixels). With `mirror` the person's
 *  right arm drives the avatar's left bones. Hips (23..) are never read. */
export function toAvatarSpace(pose, hands, mirror, visMin, handMin) {
  const sx = mirror ? -1 : 1;
  const v = (x, y, z) => [f(sx * x), f(-y), f(-z)];
  const point = (i) => {
    const p = pose[i];
    const [x, y, z] = p.world;
    return p.visibility >= visMin && Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(z) ? v(x, y, z) : null;
  };
  const arm = (subject) => {
    const [sh, el, wr, ix, pk] = (subject === 'left' ? [11, 13, 15, 19, 17] : [12, 14, 16, 20, 18]).map(point);
    const h = hands.find((hh) => hh.side === subject && hh.score >= handMin);
    return { shoulder: sh, elbow: el, wrist: wr, index: ix, pinky: pk, hand: h ? h.points.map((p) => v(p[0], p[1], p[2])) : null };
  };
  const subjectLeft = arm('left');
  const subjectRight = arm('right');
  const a = point(7), b = point(8);
  const head = a && b ? [f(f(a[0] + b[0]) / 2), f(f(a[1] + b[1]) / 2), f(f(a[2] + b[2]) / 2)] : null;
  return mirror ? { left: subjectRight, right: subjectLeft, head } : { left: subjectLeft, right: subjectRight, head };
}

/** An arm whose elbow is missing keeps only its shoulder (the avatar holds it at rest). Mutates and returns `body`. */
export function dropArmsWithoutElbow(body) {
  for (const arm of [body.left, body.right]) {
    if (arm.elbow === null) {
      arm.wrist = null;
      arm.index = null;
      arm.pinky = null;
      arm.hand = null;
    }
  }
  return body;
}
