// Pure-logic units against the Rust fixtures and the Rust unit tests, no browser, no models:
//   node --no-warnings --test web/face/test/units.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { SEQ_LEN, STILLS, readJson, seqName, maxDiff, angleDeg } from '../../body/test/lib.mjs';
import { fitHeadRotation } from '../fit.js';
import { FaceLost, QuatFilter, WeightsFilter } from '../smooth.js';
import { applyMap, applyMirror, browsFromWeights, buildMap, gazeFromExpressions, BLENDSHAPE_NAMES } from '../targets.js';
import { DetectionScheduler, rectFromDetection } from '../crop.js';
import { AutoBrightness, gammaTable, meanBrightness } from '../autobrightness.js';

const f = Math.fround;
const map = buildMap(JSON.parse(fs.readFileSync(new URL('../../../data/blendshape_map.json', import.meta.url), 'utf8')));
const seq = Array.from({ length: SEQ_LEN }, (_, k) => readJson(`seq/face/${seqName(k)}.json`));
const stills = STILLS.map((id) => readJson(`face/${id}.json`));
const f32 = (a) => a.map((p) => p.map(f));

test('head fit on the Rust landmarks gives the Rust rotation (< 0.01 deg) or the same error', () => {
  for (const fx of [...stills, ...seq]) {
    if (!fx.pointsPx) continue;
    const fit = fitHeadRotation(f32(fx.pointsPx));
    if (fx.headFit.error) assert.equal(fit.error, fx.headFit.error);
    else assert.ok(angleDeg(fit.rotation, fx.headFit.rotation) < 0.01 && Math.abs(fit.residual - fx.headFit.residual) < 1e-4);
  }
});

test('mirror, weight and quaternion filters on the Rust unsmoothed values reproduce the Rust output over the sequence', () => {
  const wf = new WeightsFilter(1, 0.007), qf = new QuatFilter();
  seq.forEach((fx) => {
    const fit = fx.headFit.rotation ?? [0, 0, 0, 1];
    const [w, q] = applyMirror(true, Float32Array.from(fx.blend), fit.map(f));
    assert.ok(maxDiff(Array.from(w), fx.mirrored.weights) < 1e-6);
    const ow = wf.filter(w, fx.dt), oq = qf.filter(q, fx.dt);
    assert.ok(maxDiff(Array.from(ow), fx.output.weights) < 1e-5);
    assert.ok(angleDeg(oq, fx.output.headRotation) < 0.01);
  });
});

test('expressions, brows and gaze from the Rust output weights match to 1e-5', () => {
  for (const fx of [...stills, ...seq]) {
    const w = Float32Array.from(fx.output.weights);
    const ex = applyMap(map, w);
    assert.deepEqual(Object.keys(ex).sort(), Object.keys(fx.expressions));
    for (const k in ex) assert.ok(Math.abs(ex[k] - fx.expressions[k]) < 1e-5, k);
    const br = browsFromWeights((n) => w[BLENDSHAPE_NAMES.indexOf(n)]);
    assert.ok(maxDiff(Object.values(br), Object.values(fx.brows)) < 1e-5);
    const gz = gazeFromExpressions((n) => ex[n] ?? 0);
    assert.ok(maxDiff([gz.x, gz.y], [fx.gaze.x, fx.gaze.y]) < 1e-5);
  }
});

test('face lost: hold 0.5 s then ease to neutral over 1 s matches the Rust replay', () => {
  const last = seq[SEQ_LEN - 1].output;
  const lost = new FaceLost();
  lost.update({ weights: Float32Array.from(last.weights), headRotation: last.headRotation }, 0);
  for (let k = 1; k <= 90; k++) {
    const fx = readJson(`seq/facelost/${seqName(k)}.json`);
    const o = lost.update(null, fx.elapsed);
    assert.equal(o.faceFound, false);
    assert.ok(maxDiff(Array.from(o.weights), fx.weights) < 1e-5, `step ${k}`);
    assert.ok(angleDeg(o.headRotation, fx.headRotation) < 0.01, `step ${k}`);
  }
});

test('scheduler: detector runs on frame 0, every 5th frame, and while there is no face or low presence', () => {
  const s = new DetectionScheduler(0.8);
  const run = (face, presence) => { const d = s.shouldDetect(); s.report(face, presence); return d; };
  assert.deepEqual([0, 1, 2, 3, 4, 5, 6].map(() => run(true, 0.99)), [true, false, false, false, false, true, false]);
  assert.equal(run(true, 0.5), false); // low presence is reported after this frame...
  assert.equal(run(true, 0.99), true); // ...so the next one detects
  assert.equal(run(false, null), false);
  assert.equal(run(false, null), true); // no face: every frame
  assert.deepEqual(seq.map((fx) => fx.detector.ran), seq.map((_, k) => k % 5 === 0));
});

test('rect from a detection reproduces the Rust worked example', () => {
  const r = rectFromDetection([0.4, 0.3, 0.2, 0.25], [0.45, 0.4], [0.55, 0.42], 640, 480);
  assert.ok(Math.abs(r.angle - 0.14889) < 1e-5 && Math.abs(r.cx - 320) < 1e-3 && Math.abs(r.width - 192) < 1e-3 && Math.abs(r.height - 180) < 1e-3);
});

// ---- auto brightness: the Rust unit tests
const flat = (v, w, h) => new Uint8Array(w * h * 3).fill(v);
function settle(auto, scene, w, h, seconds) {
  let last = scene;
  for (let i = 0; i < seconds * 10; i++) { last = scene.slice(); auto.process(last, w, h, i * 0.1); }
  return last;
}
test('auto brightness: dark lifted, bright pulled down, good scene and covered lens untouched, table sane', () => {
  const [w, h] = [64, 48];
  let a = new AutoBrightness();
  assert.ok(Math.abs(meanBrightness(settle(a, flat(45, w, h), w, h, 6), w, h) - 0.45) < 0.05 && a.gamma < 1);
  a = new AutoBrightness();
  settle(a, flat(10, w, h), w, h, 6);
  assert.ok(Math.abs(a.gamma - 0.4) < 0.02);
  a = new AutoBrightness();
  const out = settle(a, flat(240, w, h), w, h, 6);
  assert.ok(meanBrightness(out, w, h) < meanBrightness(flat(240, w, h), w, h) - 0.03 && a.gamma > 1.5);
  a = new AutoBrightness();
  const ok = flat(Math.floor(0.45 * 255), w, h);
  assert.deepEqual(settle(a, ok, w, h, 3), ok);
  a = new AutoBrightness();
  assert.deepEqual(settle(a, flat(3, w, h), w, h, 3), flat(3, w, h));
  assert.equal(a.gamma, 1);
  for (const g of [0.4, 0.7, 1.4, 1.8]) {
    const t = gammaTable(g);
    assert.ok(t[0] === 0 && t[255] === 255 && t.every((v, i) => i === 0 || t[i - 1] <= v));
  }
});
