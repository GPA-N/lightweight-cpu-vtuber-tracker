// Pure-logic units against the Rust fixtures, no browser, no models: node --test web/body/test/units.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { STILLS, SEQ_LEN, readJson, seqName, maxDiff, bodyPoseDiff, fixtureLocals, maxAngle, angleDeg } from './lib.mjs';
import { OneEuro } from '../oneeuro.js';
import * as person from '../person.js';
import * as hmr from '../instanthmr.js';
import { JointSmoother } from '../pipeline.js';
import { SLOTS, poseLandmarks, toAvatarSpace, dropArmsWithoutElbow } from '../bodypose.js';
import { buildRig, solve, BodyFollow, advanceBody, isHandBone } from '../solve.js';

const f = Math.fround;
const rig = buildRig(readJson('rig.json').nodes);
const fr = (o) => Object.fromEntries(Object.entries(o).map(([k, v]) => [k, f(v)])); // JSON doubles back to the f32 they were
const flat = (a) => Float32Array.from(a.flat());
const steps = () => [
  ...STILLS.map((id) => ({ fx: readJson(`body/${id}.json`), hands: readJson(`hands/${id}.json`).smoothed, fresh: true })),
  ...Array.from({ length: SEQ_LEN }, (_, k) => ({ fx: readJson(`seq/body/${seqName(k)}.json`), hands: readJson(`seq/hands/${seqName(k)}.json`).smoothed, fresh: k === 0 })),
];

test('one euro: first sample passes, a step moves monotonically and settles', () => {
  const o = new OneEuro(1, 0);
  assert.equal(o.filter(5, 1 / 30), 5);
  let prev = 5;
  for (let i = 0; i < 90; i++) {
    const y = o.filter(10, 1 / 30);
    assert.ok(y >= prev && y <= 10);
    prev = y;
  }
  assert.ok(10 - prev < 0.05);
});

test('person: expand grows a tenth per side and stays in the frame', () => {
  assert.deepEqual(person.expand({ x: 100, y: 0, w: 100, h: 100 }, 1000, 1000), { x: 90, y: 0, w: 120, h: 110 });
});

test('person: the most confident query wins and boxes come back in frame pixels', () => {
  const labels = new Float32Array(2 * 91).fill(-10);
  labels[1] = 0; // sigmoid(0) = 0.5, at the threshold
  labels[91 + 1] = 4;
  const b = person.decode(Float32Array.of(0.5, 0.5, 0.2, 0.2, 0.5, 0.5, 0.5, 1.0), labels, 91, 1000, 500);
  assert.deepEqual(b, { x: 250, y: 0, w: 500, h: 500 });
  assert.equal(person.decode(Float32Array.of(0.5, 0.5, 0.5, 0.5), new Float32Array(91).fill(-3), 91, 100, 100), null);
});

test('preprocess of a constant frame is the normalised constant (detector and body model)', () => {
  const t = person.preprocess(new Uint8Array(8 * 6 * 3).fill(255), 8, 6);
  assert.ok(Math.abs(t[0] - (1 - 0.485) / 0.229) < 1e-5 && Math.abs(t[2 * 384 * 384] - (1 - 0.406) / 0.225) < 1e-5);
  const h = hmr.preprocess(new Uint8Array(8 * 8 * 3).fill(255), 8, 8, { x0: -100, y0: 0, size: 8 });
  assert.ok(Math.abs(h[0] - (0 - 0.485) / 0.229) < 1e-4, 'black outside the frame');
});

test('crop and cliff_cond equal the Rust values on the Rust boxes (< 1e-4)', () => {
  for (const { fx } of steps()) {
    const u = fr(fx.detector.used);
    const c = hmr.squareCrop(u);
    assert.ok(maxDiff([c.x0, c.y0, c.size], [fx.crop.x0, fx.crop.y0, fx.crop.size]) < 1e-4);
    assert.ok(maxDiff(Array.from(hmr.cliffCond(u, fx.w, fx.h)), fx.cliffCond) < 1e-4);
  }
});

test('the box rule: fresh detection as is, later frames expand it (sequence fixtures)', () => {
  let latest = null;
  SEQ_LEN && Array.from({ length: SEQ_LEN }).forEach((_, k) => {
    const fx = readJson(`seq/body/${seqName(k)}.json`);
    if (fx.detector.detected) latest = fr(fx.detector.detected);
    const want = fx.detector.fresh ? latest : person.expand(latest, fx.w, fx.h);
    assert.ok(maxDiff([want.x, want.y, want.w, want.h], [fx.detector.used.x, fx.detector.used.y, fx.detector.used.w, fx.detector.used.h]) < 1e-3, `step ${k}`);
  });
});

test('joint smoothing of the Rust raw joints reproduces the Rust smoothed joints (sequence and stills)', () => {
  let s = new JointSmoother();
  let worst = 0;
  for (const { fx, fresh } of steps()) {
    if (fresh) s = new JointSmoother();
    const j = { xy: flat(fx.joints.raw.xy), xyz: flat(fx.joints.raw.xyz) };
    s.apply(j, fx.dt);
    worst = Math.max(worst, maxDiff(Array.from(j.xy), fx.joints.smoothed.xy.flat()), 1e3 * maxDiff(Array.from(j.xyz), fx.joints.smoothed.xyz.flat()));
  }
  assert.ok(worst < 1e-3, `worst ${worst}`);
});

test('pose slots and BodyPose from the Rust smoothed joints (both mirrors) match to 1e-3', () => {
  for (const { fx, hands } of steps()) {
    const pose = poseLandmarks(flat(fx.joints.smoothed.xy), flat(fx.joints.smoothed.xyz));
    assert.equal(SLOTS.length, 13);
    fx.pose.forEach((p, i) => assert.ok(maxDiff([p.x, p.y, p.z, p.visibility, ...p.world], [pose[i].x, pose[i].y, pose[i].z, pose[i].visibility, ...pose[i].world]) < 1e-3));
    for (const [m, key] of [[true, 'mirrorTrue'], [false, 'mirrorFalse']]) {
      assert.ok(bodyPoseDiff(dropArmsWithoutElbow(toAvatarSpace(pose, hands, m, 0.8, 0.8)), fx.bodyPose[key]) < 1e-3, `${key}`);
    }
  }
});

test('solve on the Rust BodyPose is within 0.5 degrees of the Rust rotations on every bone', () => {
  let worst = 0;
  for (const { fx } of steps()) {
    for (const key of ['mirrorTrue', 'mirrorFalse']) {
      const want = fixtureLocals(rig, fx.solved[key].locals.map((l) => ({ node: l.node, rotation: l.rotation })));
      worst = Math.max(worst, maxAngle(solve(rig, fx.bodyPose[key]).locals, want));
    }
  }
  assert.ok(worst < 0.5, `worst ${worst} deg`);
});

test('advance_body on the Rust targets follows the Rust per-bone filter (sequence and stills)', () => {
  let follow = new BodyFollow();
  const rest = (k) => [rig.nodes[k].restLocal, isHandBone(rig.nodes[k].name)];
  let worst = 0;
  for (const { fx, fresh } of steps()) {
    if (fresh) follow = new BodyFollow();
    const target = fx.solved.mirrorTrue.locals.map((l) => [l.node, l.rotation]);
    const got = advanceBody(follow, target, rest, fx.dt);
    const want = new Map(fx.followed.map((o) => [o.node, o.rotation]));
    assert.equal(got.length, want.size);
    for (const [i, q] of got) worst = Math.max(worst, angleDeg(q, want.get(i)));
  }
  assert.ok(worst < 0.5, `worst ${worst} deg`);
});
