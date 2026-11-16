// Names, mirror (pose/mirror.rs, pad51.rs) and the avatar-side targets (map_apply.rs, brows.rs, gaze.rs).
const f = Math.fround;
const clamp01 = (x) => Math.min(Math.max(x, 0), 1);

export const BLENDSHAPE_NAMES = ['browDownLeft', 'browDownRight', 'browInnerUp', 'browOuterUpLeft', 'browOuterUpRight', 'cheekPuff', 'cheekSquintLeft', 'cheekSquintRight', 'eyeBlinkLeft', 'eyeBlinkRight', 'eyeLookDownLeft', 'eyeLookDownRight', 'eyeLookInLeft', 'eyeLookInRight', 'eyeLookOutLeft', 'eyeLookOutRight', 'eyeLookUpLeft', 'eyeLookUpRight', 'eyeSquintLeft', 'eyeSquintRight', 'eyeWideLeft', 'eyeWideRight', 'jawForward', 'jawLeft', 'jawOpen', 'jawRight', 'mouthClose', 'mouthDimpleLeft', 'mouthDimpleRight', 'mouthFrownLeft', 'mouthFrownRight', 'mouthFunnel', 'mouthLeft', 'mouthLowerDownLeft', 'mouthLowerDownRight', 'mouthPressLeft', 'mouthPressRight', 'mouthPucker', 'mouthRight', 'mouthRollLower', 'mouthRollUpper', 'mouthShrugLower', 'mouthShrugUpper', 'mouthSmileLeft', 'mouthSmileRight', 'mouthStretchLeft', 'mouthStretchRight', 'mouthUpperUpLeft', 'mouthUpperUpRight', 'noseSneerLeft', 'noseSneerRight', 'tongueOut'];
const index = (n) => BLENDSHAPE_NAMES.indexOf(n);
const partner = BLENDSHAPE_NAMES.map((n, i) => {
  const p = n.endsWith('Left') ? n.slice(0, -4) + 'Right' : n.endsWith('Right') ? n.slice(0, -5) + 'Left' : null;
  return p && index(p) >= 0 ? index(p) : i;
});

/** Model output (MediaPipe order, `_neutral` first) to 52 clamped ARKit weights (`tongueOut` 0); NaN becomes 0. */
export function fromMediapipe(raw) {
  const out = new Float32Array(52);
  for (let i = 1; i < 52; i++) out[i - 1] = Number.isNaN(raw[i]) ? 0 : clamp01(raw[i]);
  return out;
}

/** With mirror: left/right weight pairs swap and the head rotation becomes M R M (negate y and z of the vector part). */
export function applyMirror(mirror, weights, q) {
  if (!mirror) return [weights, q];
  return [Float32Array.from(weights, (_, i) => weights[partner[i]]), [q[0], -q[1], -q[2], q[3]]];
}

/** `mapTable` is the parsed data/blendshape_map.json. Returns a Map-free object of expression -> weight (sum, then clamp). */
export function buildMap(mapTable) {
  return Object.keys(mapTable).sort().map((name) => {
    if (index(name) < 0) throw new Error(`mapping table entry ${name} is not an ARKit blendshape name`);
    const v = mapTable[name];
    return [index(name), (Array.isArray(v) ? v : [v]).map((e) => [e.expression, Math.max(Math.min(f(e.weight), f(e.max ?? 1)), 0)])];
  });
}

export function applyMap(map, weights) {
  const sums = {};
  for (const [i, entries] of map) for (const [expr, w] of entries) sums[expr] = f((sums[expr] ?? 0) + f(weights[i] * w));
  for (const k in sums) sums[k] = clamp01(sums[k]);
  return sums;
}

export function browsFromWeights(get) {
  const inner = get('browInnerUp');
  const avg = (a, b) => clamp01(f(f(get(a) + get(b)) / 2));
  return {
    downLeft: clamp01(get('browDownLeft')), downRight: clamp01(get('browDownRight')),
    upLeft: clamp01(Math.max(inner, get('browOuterUpLeft'))), upRight: clamp01(Math.max(inner, get('browOuterUpRight'))),
    wide: avg('mouthStretchLeft', 'mouthStretchRight'), funnel: clamp01(get('mouthFunnel')), pucker: clamp01(get('mouthPucker')),
    lipUp: avg('mouthUpperUpLeft', 'mouthUpperUpRight'), lipDown: avg('mouthLowerDownLeft', 'mouthLowerDownRight'),
  };
}

export function gazeFromExpressions(get) {
  const c = (x) => Math.min(Math.max(x, -1), 1);
  return { x: c(f(get('lookLeft') - get('lookRight'))), y: c(f(get('lookUp') - get('lookDown'))) };
}
