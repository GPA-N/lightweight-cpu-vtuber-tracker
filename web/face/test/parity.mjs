// Stage 2 parity: the JS face pipeline in headless system Chrome against the Rust reference. From the repo root:
//   node --no-warnings web/face/test/parity.mjs        (env as in web/body/NOTES.md: WEBTEST, MODELS, WEB_FIXTURES, CHROME; DUMP=<file>)
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { FIX, STILLS, SEQ_LEN, readJson, seqName, maxDiff, angleDeg } from '../../body/test/lib.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(here, '../..');
const WEBTEST = process.env.WEBTEST ?? path.join(process.env.TEMP, 'webtest');
const MODELS = process.env.MODELS ?? 'C:/Users/Lenovo/relstage/vtuber-tracker/models';
const CHROME = process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe';
const { chromium } = createRequire(path.join(WEBTEST, 'x.js'))('playwright-core');

const routes = { '/ort/': path.join(WEBTEST, 'node_modules/onnxruntime-web/dist/'), '/models/': MODELS + '/', '/fix/': FIX + '/', '/web/': webDir + '/', '/data/': path.resolve(webDir, '../data') + '/' };
const types = { '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm', '.json': 'application/json' };
const server = http.createServer((req, res) => {
  const url = req.url.split('?')[0];
  if (url === '/') { res.writeHead(200, { 'content-type': 'text/html' }); return res.end('<script type="module" src="/web/face/test/page.mjs"></script>'); }
  for (const [p, dir] of Object.entries(routes)) {
    if (!url.startsWith(p)) continue;
    const file = path.join(dir, decodeURIComponent(url.slice(p.length)));
    if (!fs.existsSync(file)) { res.writeHead(404); return res.end(); }
    res.writeHead(200, { 'content-type': types[path.extname(file)] ?? 'application/octet-stream' });
    return fs.createReadStream(file).pipe(res);
  }
  res.writeHead(404); res.end();
}).listen(8768);

const browser = await chromium.launch({ executablePath: CHROME, headless: true });
const page = await browser.newPage();
page.on('console', (m) => console.log('[page]', m.text()));
page.on('pageerror', (e) => console.log('[pageerror]', e.message));
await page.goto('http://localhost:8768/');
await page.waitForFunction(() => window.__done === true, null, { timeout: 1_200_000 });
const results = await page.evaluate(() => window.__results);
await browser.close();
server.close();
if (process.env.DUMP) fs.writeFileSync(process.env.DUMP, JSON.stringify(results));

// ---- compare
const worst = {};
const note = (key, v, at, lim) => { if (!(key in worst) || v > worst[key].v || Number.isNaN(v)) worst[key] = { v, at, lim }; };
const items = [
  ...results.stills.map((r) => ({ r, fx: readJson(`face/${r.id}.json`), at: r.id })),
  ...results.seq.map((r, k) => ({ r, fx: readJson(`seq/face/${seqName(k)}.json`), at: 'seq ' + seqName(k) })),
];
for (const { r, fx, at } of items) {
  // f. scheduler and found flags
  note('f detector ran / found flag mismatches', (r.ran === fx.detector.ran ? 0 : 1) + (r.found === fx.output.faceFound ? 0 : 1) + ((r.rect === null) === (fx.rect === null) ? 0 : 1), at, 0);
  // a. detector (frame pixels)
  if (fx.detector.ran) {
    const px = (d, w, h) => [d.box[0] * w, d.box[1] * h, d.box[2] * w, d.box[3] * h];
    note('a detections count mismatch', r.detections.length === fx.detector.detections.length ? 0 : 1, at, 0);
    fx.detector.detections.forEach((d, i) => {
      if (!r.detections[i]) return;
      note('a face box (px)', maxDiff(px(r.detections[i], fx.w, fx.h), px(d, fx.w, fx.h)), at, 1);
      note('a keypoints (px)', maxDiff(r.detections[i].keypoints.map(([x, y]) => [x * fx.w, y * fx.h]), d.keypoints.map(([x, y]) => [x * fx.w, y * fx.h])), at, 1);
    });
  }
  if (fx.rect && r.rect) note('a rect cx,cy,w,h (px) / angle (rad)', maxDiff([r.rect.cx, r.rect.cy, r.rect.width, r.rect.height, r.rect.angle], [fx.rect.cx, fx.rect.cy, fx.rect.width, fx.rect.height, fx.rect.angle]), at, null);
  if (!fx.landmarks || !r.landmarks) continue;
  // b. landmarks
  note('b presence', Math.abs(r.landmarks.presence - fx.landmarks.presence), at, 1e-3);
  if (fx.pointsPx) {
    note('b 478 points x,y (px)', maxDiff(r.pointsPx.map((p) => [p[0], p[1]]), fx.pointsPx.map((p) => [p[0], p[1]])), at, 0.05);
    note('b 478 points z (px, diag)', maxDiff(r.pointsPx.map((p) => p[2]), fx.pointsPx.map((p) => p[2])), at, null);
    // c. blendshapes, from our landmarks and from the Rust ones isolate nothing here: raw model output and filtered
    note('c raw model output, 52 (before clamp)', maxDiff(r.blendRaw, fx.blendRaw), at, 1e-3);
    note('c weights after mirror, before smoothing', maxDiff(r.mirrored.weights, fx.mirrored.weights), at, 1e-3);
    note('c weights after smoothing', maxDiff(r.weights, fx.output.weights), at, 1e-3);
    // d. head rotation
    note('d fit rotation, unsmoothed (deg)', fx.headFit.rotation && r.headFit.rotation ? angleDeg(r.headFit.rotation, fx.headFit.rotation) : (fx.headFit.error === r.headFit.error ? 0 : 180), at, 0.1);
    note('d head rotation after filter (deg)', angleDeg(r.headRotation, fx.output.headRotation), at, 0.1);
  }
  // e. targets
  const keys = new Set([...Object.keys(r.expressions), ...Object.keys(fx.expressions)]);
  let e = 0;
  for (const k of keys) e = Math.max(e, Math.abs((r.expressions[k] ?? Infinity) - (fx.expressions[k] ?? -Infinity)));
  note('e expressions', e, at, 1e-3);
  note('e brows', maxDiff(Object.values(r.brows), Object.values(fx.brows)), at, 1e-3);
  note('e gaze', maxDiff([r.gaze.x, r.gaze.y], [fx.gaze.x, fx.gaze.y]), at, 1e-3);
}
// f. face-lost replay through the real tracker (blank frames after the sequence) against seq/facelost
results.lost.forEach((r, k) => {
  const fx = readJson(`seq/facelost/${seqName(k + 1)}.json`);
  const at = 'lost ' + (k + 1);
  note('f face-lost weights (replay)', maxDiff(r.weights, fx.weights), at, 1e-3);
  note('f face-lost rotation (deg)', angleDeg(r.headRotation, fx.headRotation), at, 0.1);
  note('f face-lost found flag mismatches', r.found === fx.faceFound ? 0 : 1, at, 0);
});

let fail = 0;
console.log('\ncriterion'.padEnd(50) + 'max'.padStart(13) + '  limit   where');
for (const [key, { v, at, lim }] of Object.entries(worst).sort()) {
  const ok = lim === null || v <= lim;
  if (!ok) fail++;
  console.log(key.padEnd(49) + (Number.isFinite(v) ? v.toExponential(3) : String(v)).padStart(13) + '  ' + String(lim ?? 'diag').padEnd(7) + ' ' + at + (ok ? (lim && v > 0.9 * lim ? '   (margin < 10%)' : '') : '   FAIL'));
}
const med = (xs) => xs.slice().sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const ms = results.seq.map((r) => r.ms);
console.log(`\ng. timing (headless Chrome, 1 thread): init ${results.timing.initMs.toFixed(0)} ms; per step median of ${SEQ_LEN}: landmarks ${med(ms.map((m) => m.landmarks)).toFixed(0)} ms, blendshapes ${med(ms.map((m) => m.blendshapes)).toFixed(1)} ms, whole process() ${med(ms.map((m) => m.total)).toFixed(0)} ms`);
const dms = ms.filter((m, k) => results.seq[k].ran).map((m) => m.detector);
console.log(`   detector on its ${dms.length} steps: median ${med(dms).toFixed(1)} ms; process() on those steps ${med(ms.filter((m, k) => results.seq[k].ran).map((m) => m.total)).toFixed(0)} ms, on the others ${med(ms.filter((m, k) => !results.seq[k].ran).map((m) => m.total)).toFixed(0)} ms`);
console.log(fail ? `\n${fail} criterion/criteria FAILED` : '\nall pass');
process.exit(fail ? 1 : 0);
