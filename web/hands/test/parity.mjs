// Stage 3 parity: the JS hand stage in headless system Chrome against the Rust reference, fed the pose the Rust was given
// (fixture `poseIn`). Then the hands go through the stage 1 solver and are compared with the Rust finger and hand bones.
//   node --no-warnings web/hands/test/parity.mjs      (env as in web/body/NOTES.md; DUMP=<file> writes our outputs)
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FIX, SEQ_LEN, readJson, seqName, maxDiff, angleDeg, fixtureLocals, maxAngle } from '../../body/test/lib.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(here, '../..');
const WEBTEST = process.env.WEBTEST ?? path.join(process.env.TEMP, 'webtest');
const MODELS = process.env.MODELS ?? 'C:/Users/Lenovo/relstage/vtuber-tracker/models';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const { chromium } = createRequire(path.join(WEBTEST, 'x.js'))('playwright-core');
const body = (n) => import(pathToFileURL(path.join(webDir, 'body', n)));
const { buildRig, solve, BodyFollow, advanceBody, isHandBone } = await body('solve.js');
const { toAvatarSpace, dropArmsWithoutElbow } = await body('bodypose.js');

const routes = { '/ort/': path.join(WEBTEST, 'node_modules/onnxruntime-web/dist/'), '/models/': MODELS + '/', '/fix/': FIX + '/', '/web/': webDir + '/' };
const types = { '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json' };
const server = http.createServer((req, res) => {
  const url = req.url.split('?')[0];
  if (url === '/') { res.writeHead(200, { 'content-type': 'text/html' }); return res.end('<script type="module" src="/web/hands/test/page.mjs"></script>'); }
  for (const [p, dir] of Object.entries(routes)) {
    if (!url.startsWith(p)) continue;
    const file = path.join(dir, decodeURIComponent(url.slice(p.length)));
    if (!fs.existsSync(file)) { res.writeHead(404); return res.end(); }
    res.writeHead(200, { 'content-type': types[path.extname(file)] ?? 'application/octet-stream' });
    return fs.createReadStream(file).pipe(res);
  }
  res.writeHead(404); res.end();
}).listen(8769);

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage();
page.on('console', (m) => console.log('[page]', m.text()));
page.on('pageerror', (e) => console.log('[pageerror]', e.message));
await page.goto('http://localhost:8769/');
await page.waitForFunction(() => window.__done === true, null, { timeout: 1_200_000 });
const results = await page.evaluate(() => window.__results);
await browser.close();
server.close();
if (process.env.DUMP) fs.writeFileSync(process.env.DUMP, JSON.stringify(results));

const worst = {};
const note = (key, v, at, lim) => { if (!(key in worst) || v > worst[key].v || Number.isNaN(v)) worst[key] = { v, at, lim }; };
const rig = buildRig(readJson('rig.json').nodes);
const rest = (k) => [rig.nodes[k].restLocal, isHandBone(rig.nodes[k].name)];
const ids = ['photo_claws', 'photo_fists', 'photo_thumbs', 'priv_a', 'priv_b', 'priv_c', 'blank'];
const items = [
  ...results.stills.map((r, k) => ({ r, fx: readJson(`hands/${ids[k]}.json`), bfx: readJson(`body/${ids[k]}.json`), at: ids[k], seq: false })),
  ...results.seq.map((r, k) => ({ r, fx: readJson(`seq/hands/${seqName(k)}.json`), bfx: readJson(`seq/body/${seqName(k)}.json`), at: 'seq ' + seqName(k), seq: true })),
];
let follow = new BodyFollow();
for (const { r, fx, bfx, at, seq } of items) {
  const side = Math.max(bfx.w, bfx.h);
  // a. palm detections
  note('a palm detection count mismatch', r.palm.length === fx.palm.length ? 0 : 1, at, 0);
  fx.palm.forEach((d, i) => {
    const o = r.palm[i]; if (!o) return;
    note('a palm box (px)', maxDiff([o.cx * side, o.cy * side, o.w * side, o.h * side], [d.cx * side, d.cy * side, d.w * side, d.h * side]), at, 1);
    note('a palm keypoints (px)', maxDiff(o.keypoints.map((k) => k.map((v) => v * side)), d.keypoints.map((k) => k.map((v) => v * side))), at, 0.5);
    note('a palm score', Math.abs(o.score - d.score), at, null);
  });
  // b. hand crops
  fx.rois.forEach((c, i) => { if (c && r.rois[i]) note('b palm ROI cx,cy,w,h (px)', maxDiff([r.rois[i].cx, r.rois[i].cy, r.rois[i].width, r.rois[i].height], [c.cx, c.cy, c.width, c.height]), at, 1e-3); else if (!!c !== !!r.rois[i]) note('b palm ROI null mismatch', 1, at, 0); });
  fx.rois.forEach((c, i) => { if (c && r.rois[i]) note('b palm ROI angle (rad)', Math.abs(r.rois[i].angle - c.angle), at, 1e-4); });
  note('b hand count / source mismatch', r.found.length === fx.raw.length && fx.raw.every((h, i) => h.source === r.found[i].source) ? 0 : 1, at, 0);
  fx.raw.forEach((h, i) => {
    const o = r.found[i]; if (!o) return;
    note('b hand crop cx,cy,w,h (px)', maxDiff([o.rect.cx, o.rect.cy, o.rect.width, o.rect.height], [h.rect.cx, h.rect.cy, h.rect.width, h.rect.height]), at, 1e-3);
    note('b hand crop angle (rad)', Math.abs(o.rect.angle - h.rect.angle), at, 1e-4);
    // c. landmarks, raw
    note('c raw landmarks x,y (px)', maxDiff(r.raw[i].points.map((p) => [p[0], p[1]]), h.points.map((p) => [p[0], p[1]])), at, 0.1);
    note('c raw landmarks z', maxDiff(r.raw[i].points.map((p) => p[2]), h.points.map((p) => p[2])), at, 0.1);
    note('c model output in crop space (px, diag)', maxDiff(o.crop, h.crop), at, null);
    note('d raw score / handedness', Math.max(Math.abs(r.raw[i].score - h.score), Math.abs(r.raw[i].handedness - h.handedness)), at, null);
  });
  // c/d. smoothed, sides
  note('d hand count / side mismatch', r.smoothed.length === fx.smoothed.length && fx.smoothed.every((h, i) => h.side === r.smoothed[i].side) ? 0 : 1, at, 0);
  fx.smoothed.forEach((h, i) => {
    const o = r.smoothed[i]; if (!o) return;
    note('c smoothed landmarks x,y (px)', maxDiff(o.points.map((p) => [p[0], p[1]]), h.points.map((p) => [p[0], p[1]])), at, 0.1);
    note('c smoothed landmarks z', maxDiff(o.points.map((p) => p[2]), h.points.map((p) => p[2])), at, 0.1);
    note('d score (smoothed list)', Math.abs(o.score - h.score), at, null);
  });
  // body solve with our hands and the Rust pose: finger, hand and arm bones against the Rust solve and follow
  {
    const pose = bfx.pose;
    const bp = dropArmsWithoutElbow(toAvatarSpace(pose, r.smoothed, true, 0.8, 0.8));
    const want = fixtureLocals(rig, bfx.solved.mirrorTrue.locals.map((l) => ({ node: l.node, rotation: l.rotation })));
    const sol = solve(rig, bp);
    note('h solve all bones with our hands (deg)', maxAngle(sol.locals, want), at, 0.5);
    const fingers = rig.nodes.map((n, i) => (isHandBone(n.name) ? i : -1)).filter((i) => i >= 0);
    note('h solve finger and hand bones only (deg)', fingers.reduce((m, i) => Math.max(m, angleDeg(sol.locals[i], want[i])), 0), at, 0.5);
    if (!seq || at === 'seq 000') follow = new BodyFollow();
    const got = advanceBody(follow, sol.changed, rest, bfx.dt);
    const w = new Map(bfx.followed.map((o) => [o.node, o.rotation]));
    note('h advance_body with our hands (deg)', got.reduce((m, [i, q]) => Math.max(m, w.has(i) ? angleDeg(q, w.get(i)) : Infinity), got.length === w.size ? 0 : Infinity), at, 0.5);
  }
}
note('e no hand: empty list (palm none, no pose)', results.emptyWithoutPose, 'blank, pose null', 0);

let fail = 0;
console.log('\ncriterion'.padEnd(52) + 'max'.padStart(13) + '  limit   where');
for (const [key, { v, at, lim }] of Object.entries(worst).sort()) {
  const ok = lim === null || v <= lim;
  if (!ok) fail++;
  console.log(key.padEnd(51) + (Number.isFinite(v) ? v.toExponential(3) : String(v)).padStart(13) + '  ' + String(lim ?? 'diag').padEnd(7) + ' ' + at + (ok ? (lim && v > 0.9 * lim ? '   (margin < 10%)' : '') : '   FAIL'));
}
const med = (xs) => xs.slice().sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const sq = results.seq;
console.log(`\nf. timing (headless Chrome, 1 thread): init ${results.timing.initMs.toFixed(0)} ms; per step median of ${SEQ_LEN}: palm ${med(sq.map((r) => r.ms.palm)).toFixed(0)} ms (warp, model, decode), hand ${med(sq.map((r) => r.ms.hand)).toFixed(0)} ms (${med(sq.map((r) => r.found.length))} crops), total ${med(sq.map((r) => r.ms.palm + r.ms.hand)).toFixed(0)} ms`);
console.log(fail ? `\n${fail} criterion/criteria FAILED` : '\nall pass');
process.exit(fail ? 1 : 0);
