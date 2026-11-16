// End-to-end test of the page in headless system Chrome with a fake webcam made from a fixture frame (stock photos and a blank frame).
//   node --no-warnings web/app/test/e2e.mjs            local files under the Hugging Face file names, then a second visit (cache)
//   REAL_HF=1 node --no-warnings web/app/test/e2e.mjs   one run against the real Hugging Face URLs (no CORS error, download time)
// Env: WEBTEST (playwright-core, onnxruntime-web), MODELS (local upper_body dir), RFDETR (community model.onnx), WEB_FIXTURES, SHOTS (screenshot dir), CHROME.
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { FIX, readJson } from '../../body/test/lib.mjs';

const webDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const WEBTEST = process.env.WEBTEST ?? path.join(process.env.TEMP, 'webtest');
const MODELS = process.env.MODELS ?? 'C:/Users/Lenovo/relstage/vtuber-tracker/models/upper_body';
const RFDETR = process.env.RFDETR ?? path.join(process.env.TEMP, 'rfdetr-community/model.onnx');
const SHOTS = process.env.SHOTS ?? os.tmpdir();
const FFMPEG = process.env.FFMPEG ?? 'C:/ProgramData/chocolatey/bin/ffmpeg.exe';
const { chromium } = createRequire(path.join(WEBTEST, 'x.js'))('playwright-core');
const types = { '.mjs': 'text/javascript', '.js': 'text/javascript', '.html': 'text/html', '.json': 'application/json', '.wasm': 'application/wasm' };

const requests = [];
const routes = {
  '/ort/': path.join(WEBTEST, 'node_modules/onnxruntime-web/dist/'),
  '/hf/': MODELS + '/', // the Hugging Face file names, served locally
  '/rfdetr/model.onnx': RFDETR,
};
const server = http.createServer((req, res) => {
  const url = decodeURIComponent(req.url.split('?')[0]);
  requests.push(url);
  let file = null;
  for (const [p, target] of Object.entries(routes)) if (url === p || (p.endsWith('/') && url.startsWith(p))) file = p.endsWith('/') ? path.join(target, url.slice(p.length)) : target;
  file ??= path.join(webDir, url === '/' ? 'index.html' : url.slice(1));
  if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { 'content-type': types[path.extname(file)] ?? 'application/octet-stream', 'content-length': fs.statSync(file).size });
  fs.createReadStream(file).pipe(res);
}).listen(8772);

function y4m(id) {
  const out = path.join(os.tmpdir(), `vt-e2e-${id}.y4m`);
  const meta = readJson(`frames/${id}.json`);
  execFileSync(FFMPEG, ['-y', '-v', 'error', '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-s', `${meta.w}x${meta.h}`, '-i', path.join(FIX, `frames/${id}.rgb`),
    '-vf', 'scale=640:480:force_original_aspect_ratio=decrease,pad=640:480:(ow-iw)/2:(oh-ih)/2,loop=loop=89:size=1,fps=30,format=yuv420p', '-frames:v', '90', out]);
  return out;
}

const results = [];
const check = (name, ok, detail = '') => { results.push(ok); console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? ': ' + detail : ''}`); };

async function launch(id) {
  const browser = await chromium.launch({
    executablePath: process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true,
    args: ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream', `--use-file-for-fake-video-capture=${y4m(id)}`],
  });
  return { browser, context: await browser.newContext({ viewport: { width: 1100, height: 1500 } }) };
}

async function visit(context, query, { until }) {
  const page = await context.newPage();
  const errors = [];
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(`http://localhost:8772/?${query}`);
  const t0 = Date.now();
  await page.click('#start');
  await page.waitForFunction(until, null, { timeout: 600000, polling: 500 });
  return { page, errors, seconds: (Date.now() - t0) / 1000 };
}

const tracked = () => { const a = window.__app; return a?.ready && a.stats.body.done >= 2 && a.stats.face.done >= 2 && a.stats.hands.done >= 2; };
const LOCALQ = 'ort=/ort/&hf=/hf/&rfdetr=/rfdetr/model.onnx';
const largeHits = () => requests.filter((u) => u.startsWith('/hf/') || u.startsWith('/rfdetr/') || u.endsWith('.onnx'));

if (process.env.REAL_HF) {
  const { browser, context } = await launch('photo_fists');
  const { page, errors, seconds } = await visit(context, 'ort=/ort/', { until: tracked });
  const cors = errors.filter((e) => /CORS|blocked|Failed to fetch|ERR_/i.test(e));
  check('real Hugging Face URLs: no CORS or network error in the console', cors.length === 0 && errors.length === 0, errors.slice(0, 2).join(' | '));
  console.log(`real HF: Start to first tracked frames ${seconds.toFixed(1)} s (download of about 215 MB plus model start)`);
  await browser.close();
  server.close();
  process.exit(results.every(Boolean) ? 0 : 1);
}

for (const id of ['photo_claws', 'photo_fists', 'blank']) {
  console.log(`\n== fixture ${id}`);
  const { browser, context } = await launch(id);
  requests.length = 0;
  const first = await visit(context, LOCALQ, { until: tracked });
  await first.page.waitForTimeout(8000); // let the avatar settle and the stages run in parallel for a while
  const a = await first.page.evaluate(() => {
    const s = window.__app.stats, st = window.__app.state;
    const rate = (k) => (s[k].window.length > 1 ? ((s[k].window.length - 1) * 1000) / (s[k].window.at(-1) - s[k].window[0]) : 0);
    return {
      face: st.face && { found: st.face.found, expressions: Object.keys(st.face.expressions).length },
      bodyOk: !!st.pose, handsDone: s.hands.done, hands: st.hands.length,
      head: window.__app.boneDelta('head'), left: window.__app.boneDelta('leftupperarm'), right: window.__app.boneDelta('rightupperarm'),
      timing: Object.fromEntries(['body', 'face', 'hands'].map((k) => [k, { computeMs: Math.round(s[k].ms), latencyMs: Math.round(s[k].latency), perSec: +rate(k).toFixed(2), done: s[k].done }])),
      fps: document.getElementById('fps').textContent, lookAt: window.__app.lookAtType(),
    };
  });
  console.log('timing in parallel:', JSON.stringify(a.timing), 'render fps', a.fps, 'lookAt', a.lookAt);
  check('first tracked frames reached', true, `${first.seconds.toFixed(1)} s from Start`);
  check('face stage output present', !!a.face && a.face.expressions > 0, JSON.stringify(a.face));
  check('body stage output present', a.bodyOk);
  check('hand stage ran', a.handsDone >= 2, `${a.handsDone} runs, ${a.hands} hands`);
  if (id !== 'blank') {
    check('face found and avatar head bone differs from rest', a.face?.found && a.head > 0.5, `head ${a.head.toFixed(1)} deg`);
    check('avatar arm bones differ from rest', a.left > 1 && a.right > 1, `left ${a.left.toFixed(1)} deg, right ${a.right.toFixed(1)} deg`);
  } else {
    check('no person: no face found, page still running', a.face?.found === false);
  }
  check('no console errors or page errors', first.errors.length === 0, first.errors.slice(0, 3).join(' | '));
  const hits = largeHits();
  const want = ['/hf/instanthmr-nano.onnx', '/hf/mhr-body.onnx', '/rfdetr/model.onnx', '/models/face_detector.onnx', '/models/face_landmarks.onnx', '/models/face_blendshapes.onnx', '/models/palm_detection_mediapipe_2023feb.onnx', '/models/handpose_estimation_mediapipe_2023feb.onnx'];
  check('models fetched from the expected URLs, once each', want.every((u) => hits.filter((h) => h === u).length === 1), JSON.stringify(hits.length));
  fs.mkdirSync(SHOTS, { recursive: true });
  await first.page.screenshot({ path: path.join(SHOTS, `e2e-${id}.png`), fullPage: false });

  if (id === 'photo_claws') {
    // second visit in the same browser profile: the large files come from the Cache API
    requests.length = 0;
    const second = await visit(context, LOCALQ, { until: tracked });
    check('second visit: no model re-downloaded', largeHits().length === 0, `${largeHits().length} model requests, ${second.seconds.toFixed(1)} s to tracking`);
    check('second visit: no console errors', second.errors.length === 0, second.errors.slice(0, 2).join(' | '));
  }
  await browser.close();
}
server.close();
console.log(results.every(Boolean) ? '\nall pass' : '\nFAILED');
process.exit(results.every(Boolean) ? 0 : 1);
