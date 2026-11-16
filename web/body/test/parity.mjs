// Stage 1 parity: runs the JS body pipeline in headless system Chrome on the web-fixtures frames and compares with
// the Rust reference. Run from the repo root: node web/body/test/parity.mjs
// Needs playwright-core and onnxruntime-web 1.31.1 (WEBTEST dir, default %TEMP%/webtest/node_modules), the local models
// (MODELS, default C:/Users/Lenovo/relstage/vtuber-tracker/models) and the fixtures (WEB_FIXTURES).
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FIX, STILLS, SEQ_LEN, readJson, seqName, maxDiff, boxArr, bodyPoseDiff, fixtureLocals, maxAngle, angleDeg } from './lib.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(here, '../..');
const WEBTEST = process.env.WEBTEST ?? path.join(process.env.TEMP, 'webtest');
const MODELS = process.env.MODELS ?? 'C:/Users/Lenovo/relstage/vtuber-tracker/models';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const { chromium } = createRequire(path.join(WEBTEST, 'x.js'))('playwright-core');
const { buildRig, solve, BodyFollow, advanceBody, isHandBone } = await import(pathToFileURL(path.join(here, '../solve.js')));
const { squareCrop, cliffCond } = await import(pathToFileURL(path.join(here, '../instanthmr.js')));
const { toAvatarSpace, dropArmsWithoutElbow } = await import(pathToFileURL(path.join(here, '../bodypose.js')));

// ---- serve the page, ort, models, fixtures and web/
const routes = { '/ort/': path.join(WEBTEST, 'node_modules/onnxruntime-web/dist/'), '/models/': MODELS + '/', '/fix/': FIX + '/', '/web/': webDir + '/', '/chunks/': process.env.CHUNKS ?? '', '/community/': process.env.COMMUNITY ?? '' }; // COMMUNITY=<dir with model.onnx>: run with the onnx-community RF-DETR (pixel_values)
const types = { '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json' };
const server = http.createServer((req, res) => {
  const url = req.url.split('?')[0];
  if (url === '/') {
    res.writeHead(200, { 'content-type': 'text/html' });
    return res.end('<script type="module" src="/web/body/test/' + (process.env.PAGE ?? 'page.mjs') + '"></script>');
  }
  for (const [p, dir] of Object.entries(routes)) {
    if (!url.startsWith(p)) continue;
    const file = path.join(dir, decodeURIComponent(url.slice(p.length)));
    if (!fs.existsSync(file)) { res.writeHead(404); return res.end(); }
    res.writeHead(200, { 'content-type': types[path.extname(file)] ?? 'application/octet-stream' });
    return fs.createReadStream(file).pipe(res);
  }
  res.writeHead(404);
  res.end();
}).listen(8767);

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage();
page.on('console', (m) => console.log('[page]', m.text()));
page.on('pageerror', (e) => console.log('[pageerror]', e.message));
await page.goto('http://localhost:8767/' + [process.env.RUSTBOX && 'rustbox=1', process.env.COMMUNITY && 'community=1'].filter(Boolean).reduce((s, x) => s + (s ? '&' : '?') + x, ''));
await page.waitForFunction(() => window.__done === true, null, { timeout: 1_200_000 });
const results = await page.evaluate(() => window.__results);
if (process.env.PAGE) { console.log(JSON.stringify(results, null, 1)); process.exit(0); } // a side check page: just print what it found
await browser.close();
server.close();
if (process.env.DUMP) fs.writeFileSync(process.env.DUMP, JSON.stringify(results)); // for offline digging, not in the repo

// ---- compare
const rig = buildRig(readJson('rig.json').nodes);
const restOf = (k) => [rig.nodes[k].restLocal, isHandBone(rig.nodes[k].name)];
const handsOf = (path_) => readJson(path_).smoothed;
const worst = {}; // criterion -> { v, at }
const note = (key, v, at) => { if (!(key in worst) || v > worst[key].v || Number.isNaN(v)) worst[key] = { v, at }; };
const seqDt = (fx) => fx.dt;

// Each item: our result, fixture body json, hands fixture path, follow state (stills: fresh, seq: carried).
const items = [
  ...results.stills.map((r) => ({ r, fx: readJson(`body/${r.id}.json`), hands: handsOf(`hands/${r.id}.json`), follow: new BodyFollow(), seq: false })),
];
const seqFollow = new BodyFollow();
for (const r of results.seq) items.push({ r, fx: readJson(`seq/body/${r.id}.json`), hands: handsOf(`seq/hands/${r.id}.json`), follow: seqFollow, seq: true });

for (const { r, fx, hands, follow, seq } of items) {
  const at = (seq ? 'seq ' : '') + r.id;
  const flat2 = (a) => a.flat();
  // a. box
  note('a box used (px)', maxDiff(boxArr(r.box), boxArr(fx.detector.used)), at);
  if (fx.detector.ran && fx.detector.detected) note('a detected box (px)', maxDiff(boxArr(r.box), boxArr(fx.detector.detected)), at);
  note('a fresh flag mismatches', r.boxFresh === fx.detector.fresh ? 0 : 1, at);
  // b. crop, cliff_cond
  note('b crop x0,y0,size, end to end (diag)', maxDiff([r.crop.x0, r.crop.y0, r.crop.size], [fx.crop.x0, fx.crop.y0, fx.crop.size]), at);
  note('b cliff_cond, end to end (diag)', maxDiff(r.cond, fx.cliffCond), at);
  // the same functions on Rust's own box: separates the code from the detector's model noise
  const used = Object.fromEntries(Object.entries(fx.detector.used).map(([k, v]) => [k, Math.fround(v)])); // JSON doubles back to the f32 they were
  const c2 = squareCrop(used);
  note('b crop x0,y0,size from the Rust box', maxDiff([c2.x0, c2.y0, c2.size], [fx.crop.x0, fx.crop.y0, fx.crop.size]), at);
  note('b cliff_cond from the Rust box', maxDiff(cliffCond(used, fx.w, fx.h), fx.cliffCond), at);
  // c. joints
  note('c raw xy (px)', maxDiff(r.raw.xy, flat2(fx.joints.raw.xy)), at);
  note('c raw xyz (m)', maxDiff(r.raw.xyz, flat2(fx.joints.raw.xyz)), at);
  note('c smoothed xy (px)', maxDiff(r.joints.xy, flat2(fx.joints.smoothed.xy)), at);
  note('c smoothed xyz (m)', maxDiff(r.joints.xyz, flat2(fx.joints.smoothed.xyz)), at);
  // d. pose slots and BodyPose (mirror true from the tracker, mirror false from the same pose)
  let slots = 0, slotsPx = 0;
  fx.pose.forEach((p, i) => {
    const q = r.pose[i];
    slots = Math.max(slots, maxDiff([p.z, p.visibility, ...p.world], [q.z, q.visibility, ...q.world]));
    slotsPx = Math.max(slotsPx, maxDiff([p.x, p.y], [q.x, q.y]));
  });
  note('d pose slots, z/visibility/world', slots, at);
  note('d pose slots, x/y (px)', slotsPx, at);
  const bpTrue = r.body;
  const bpFalse = dropArmsWithoutElbow(toAvatarSpace(r.pose, hands, false, 0.8, 0.8));
  note('d BodyPose mirror true', bodyPoseDiff(bpTrue, fx.bodyPose.mirrorTrue), at);
  note('d BodyPose mirror false', bodyPoseDiff(bpFalse, fx.bodyPose.mirrorFalse), at);
  // e. solved rotations (our pose -> our solve), plus the solver alone on Rust's own BodyPose
  for (const [name, ours, theirs] of [['true', bpTrue, fx.solved.mirrorTrue], ['false', bpFalse, fx.solved.mirrorFalse]]) {
    const want = fixtureLocals(rig, theirs.locals.map((l) => ({ node: l.node, rotation: l.rotation })));
    note(`e solve mirror ${name} (deg)`, maxAngle(solve(rig, ours).locals, want), at);
    note(`e' solver alone on Rust pose, mirror ${name} (deg)`, maxAngle(solve(rig, fx.bodyPose[name === 'true' ? 'mirrorTrue' : 'mirrorFalse']).locals, want), at);
  }
  // e. advance_body (live mirror), per-bone One Euro on the rotation
  const dt = seq ? seqDt(fx) : fx.dt;
  const got = advanceBody(follow, solve(rig, bpTrue).changed, restOf, dt);
  const want = new Map(fx.followed.map((o) => [o.node, o.rotation]));
  let fo = got.length === want.size ? 0 : Infinity;
  for (const [i, q] of got) fo = Math.max(fo, want.has(i) ? angleDeg(q, want.get(i)) : Infinity);
  note('e advance_body (deg)', fo, at);
  // f. no person: only the blank still
  if (r.id === 'blank') {
    note('f blank: null detection, whole-frame box', !r.boxFresh && fx.detector.detected === null && maxDiff(boxArr(r.box), [0, 0, fx.w, fx.h]) === 0 ? 0 : 1, at);
  }
}

const limit = (k) => {
  if (k.includes('(diag)') || k.startsWith("e'") || k.startsWith('g')) return null;
  if (k.startsWith('a fresh') || k.startsWith('f ')) return 0;
  if (k.startsWith('a')) return 1;
  if (k.startsWith('b')) return 1e-4;
  if (k.startsWith('c') || k.startsWith('d pose slots, x/y')) return k.endsWith('(px)') ? 0.5 : 1e-3;
  if (k.startsWith('d')) return 1e-3;
  return 0.5; // e
};let fail = 0;
console.log('\ncriterion'.padEnd(52) + 'max'.padStart(14) + '  limit   where');
for (const [key, { v, at }] of Object.entries(worst).sort()) {
  const lim = limit(key);
  const ok = lim === null || v <= lim;
  if (!ok) fail++;
  console.log(key.padEnd(52) + (Number.isFinite(v) ? v.toExponential(3) : String(v)).padStart(14) + '  ' + String(lim ?? 'diag').padEnd(7) + at + (ok ? '' : '   FAIL'));
}

// g. timing
const med = (xs) => xs.slice().sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const seqMs = results.seq.map((r) => r.ms);
const ran = results.seq.filter((r, k) => k % 30 === 0).map((r) => r.ms.detector);
console.log(`\ng. timing (headless Chrome, 1 thread): init ${results.timing.initMs.toFixed(0)} ms (3 sessions + worker); per step, median of ${SEQ_LEN}: ` +
  `hmr ${med(seqMs.map((m) => m.hmr)).toFixed(0)} ms, mhr ${med(seqMs.map((m) => m.mhr)).toFixed(0)} ms, whole process() ${med(seqMs.map((m) => m.total)).toFixed(0)} ms (detector awaited on steps 0 and 30)`);
console.log(`   RF-DETR in the worker (preprocess + run + decode) on stride steps: ${ran.map((x) => x.toFixed(0)).join(', ')} ms`);
console.log(`   non-stride steps (process() without detector): ${med(results.seq.filter((_, k) => k % 30 !== 0).map((r) => r.ms.total)).toFixed(0)} ms median`);
console.log(fail ? `\n${fail} criterion/criteria FAILED` : '\nall pass');
process.exit(fail ? 1 : 0);
