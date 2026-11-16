// Pure-logic units against the Rust fixtures and Rust unit tests: node --no-warnings --test web/hands/test/units.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { SEQ_LEN, readJson, seqName, maxDiff } from '../../body/test/lib.mjs';
import { HandSmoother } from '../pipeline.js';
import * as H from '../hands.js';

const f = Math.fround;
const seq = Array.from({ length: SEQ_LEN }, (_, k) => readJson(`seq/hands/${seqName(k)}.json`));
const f32 = (o) => Object.fromEntries(Object.entries(o).map(([k, v]) => [k, f(v)]));

test('anchor counts and merge rule match the Rust tests', () => {
  assert.equal(H.ssdAnchors(224, [8, 16, 32, 32, 32]).length, 2254);
  assert.equal(H.ssdAnchors(192, [8, 16, 16, 16]).length, 2016);
  const a = H.ssdAnchors(224, [8, 16, 32, 32, 32]);
  assert.deepEqual(a[0], [f(0.5 / 28), f(0.5 / 28)]);
  assert.deepEqual(a[1], a[0]);
});

test('decode adds the anchor and filters by score; nms keeps the best of overlapping boxes', () => {
  const boxes = new Float32Array(12);
  boxes.set([22.4, 0, 112, 56, 0, 22.4]);
  const d = H.decodeDetections(boxes, Float32Array.of(10, -10), [[0.25, 0.25], [0.75, 0.75]], 224, 1, 0.5);
  assert.equal(d.length, 1);
  assert.ok(Math.abs(d[0].cx - 0.35) < 1e-5 && Math.abs(d[0].w - 0.5) < 1e-5 && Math.abs(d[0].keypoints[0][1] - 0.35) < 1e-5);
  const mk = (cx, s) => ({ score: s, cx, cy: 0.5, w: 0.2, h: 0.2, keypoints: [] });
  assert.deepEqual(H.nms([mk(0.5, 0.6), mk(0.51, 0.9), mk(0.9, 0.7)], 0.3).map((x) => x.score), [0.9, 0.7]);
});

test('palm ROI and pose-derived crop from the Rust fixtures reproduce the Rust rects', () => {
  for (const fx of seq) {
    // the pose-derived crops: poseIn is the Rust's input
    const p = fx.poseIn.map((l) => f32({ x: l.x, y: l.y }) && { ...l, x: f(l.x), y: f(l.y) });
    const rects = [[13, 15, 19, 17], [14, 16, 20, 18]].filter((ids) => p[ids[1]].visibility >= 0.5).map((ids) => H.handRect(...ids.map((i) => p[i]))).filter(Boolean);
    const raws = fx.raw.filter((r) => r.source === 'pose');
    if (raws.length === 0) { assert.ok(fx.raw.length > 0); } else assert.equal(rects.length, raws.length);
    raws.forEach((r, i) => assert.ok(maxDiff([rects[i].cx, rects[i].cy, rects[i].width, rects[i].angle], [r.rect.cx, r.rect.cy, r.rect.width, r.rect.angle]) < 1e-3));
    fx.palm.forEach((d, i) => {
      const roi = H.palmRoi({ ...f32({ score: d.score, cx: d.cx, cy: d.cy, w: d.w, h: d.h }), keypoints: d.keypoints.map((k) => k.map(f)) }, 640, 480);
      assert.ok(maxDiff([roi.cx, roi.cy, roi.width, roi.angle], [fx.rois[i].cx, fx.rois[i].cy, fx.rois[i].width, fx.rois[i].angle]) < 1e-3);
    });
  }
});

test('sides: nearest pose wrists, total distance minimal', () => {
  const pose = Array.from({ length: 33 }, () => ({ x: 0, y: 0 }));
  pose[15] = { x: 100, y: 100 }; pose[16] = { x: 300, y: 100 };
  assert.deepEqual(H.assignSides([[110, 100]], pose), ['left']);
  assert.deepEqual(H.assignSides([[290, 100]], pose), ['right']);
  assert.deepEqual(H.assignSides([[290, 100], [110, 100]], pose), ['right', 'left']);
  assert.deepEqual(H.assignSides([], pose), []);
});

test('smoothing the Rust raw hands reproduces the Rust smoothed hands over the sequence; a side with no hand keeps its filters', () => {
  const sm = new HandSmoother();
  let worst = 0;
  seq.forEach((fx, k) => {
    const hands = fx.smoothed.map((s) => ({ side: s.side, points: fx.raw[fx.smoothed.indexOf(s)].points.map((p) => p.map(f)) }));
    sm.apply(hands, k * 33_333_333e-9);
    hands.forEach((h, i) => { worst = Math.max(worst, maxDiff(h.points, fx.smoothed[i].points)); });
  });
  assert.ok(worst < 1e-3, `worst ${worst}`);
  // left absent for 10 steps, right continues: right output must equal a right-only smoother
  const both = new HandSmoother(), solo = new HandSmoother();
  for (let i = 0; i < 40; i++) {
    const pt = (x) => Array.from({ length: 21 }, () => [x, 50, 1]);
    const hs = [{ side: 'right', points: pt(200 + i) }];
    if (i < 10) hs.push({ side: 'left', points: pt(10 + i) });
    both.apply(hs, i * 0.055);
    const s = [{ side: 'right', points: pt(200 + i) }];
    solo.apply(s, i * 0.055);
    assert.deepEqual(hs.find((h) => h.side === 'right').points, s[0].points);
  }
});
