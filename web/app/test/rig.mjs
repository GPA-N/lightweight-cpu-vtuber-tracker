// The rig built from web/avatar.vrm by three-vrm equals the Bevy-convention rig of web-fixtures/rig.json (node order, names,
// parents, rest rotations and positions), up to 1e-6. Run from the repo root: node --no-warnings web/app/test/rig.mjs
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { readJson, maxDiff } from '../../body/test/lib.mjs';

const webDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const WEBTEST = process.env.WEBTEST ?? path.join(process.env.TEMP, 'webtest');
const { chromium } = createRequire(path.join(WEBTEST, 'x.js'))('playwright-core');
const types = { '.mjs': 'text/javascript', '.js': 'text/javascript', '.html': 'text/html', '.json': 'application/json' };
const server = http.createServer((req, res) => {
  const file = path.join(webDir, decodeURIComponent(req.url.split('?')[0].replace(/^\//, '')));
  if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { 'content-type': types[path.extname(file)] ?? 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
}).listen(8771);
const browser = await chromium.launch({ executablePath: process.env.CHROME ?? 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });
const page = await browser.newPage();
const errors = [];
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
page.on('pageerror', (e) => errors.push(e.message));
await page.goto('http://localhost:8771/app/test/rig.html');
await page.waitForFunction(() => window.__rig, null, { timeout: 60000 });
const got = await page.evaluate(() => window.__rig);
await browser.close();
server.close();

const want = readJson('rig.json').nodes;
const worst = { rotLocal: 0, rotGlobal: 0, pos: 0 };
let bad = got.length === want.length ? 0 : 1;
const q = (a, b) => Math.min(maxDiff(a, b), maxDiff(a, b.map((v) => -v))); // q and -q are one rotation
want.forEach((w, i) => {
  const g = got[i];
  if (!g || g.name !== w.name || g.parent !== w.parent) { bad++; console.log('node', i, 'differs:', g?.name, w.name, g?.parent, w.parent); return; }
  worst.rotLocal = Math.max(worst.rotLocal, q(g.restLocal, w.restLocal));
  worst.rotGlobal = Math.max(worst.rotGlobal, q(g.restGlobalRot, w.restGlobalRot));
  worst.pos = Math.max(worst.pos, maxDiff(g.restGlobalPos, w.restGlobalPos));
});
console.log(`nodes ${got.length} (rig.json ${want.length}), name/parent/order mismatches ${bad}; max diff rest local ${worst.rotLocal.toExponential(2)}, rest global rot ${worst.rotGlobal.toExponential(2)}, rest global pos ${worst.pos.toExponential(2)}`);
const ok = bad === 0 && Math.max(...Object.values(worst)) < 1e-6 && errors.length === 0;
if (errors.length) console.log('console errors:', errors.slice(0, 3));
console.log(ok ? 'rig parity pass' : 'rig parity FAIL');
process.exit(ok ? 0 : 1);
