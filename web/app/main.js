// The page: webcam -> AutoBrightness -> body, face and hand workers (each at its own pace) -> the avatar, as the Rust app does it.
import { loadModels } from './loader.js';
import { createScene, loadVrm, applyBody, applyHead, applyExpressions, applyBrows, applyGaze, lookAtType } from './avatar.js';
import { AutoBrightness } from '../face/autobrightness.js';
import { HandSmoother } from '../hands/pipeline.js';
import { toAvatarSpace, dropArmsWithoutElbow } from '../body/bodypose.js';
import { solve, BodyFollow, advanceBody, isHandBone } from '../body/solve.js';
import { VRMUtils } from '@pixiv/three-vrm';

const params = new URLSearchParams(location.search);
const abs = (u) => new URL(u, location.href).href;
const ORT_BASE = abs(params.get('ort') ?? 'https://cdn.jsdelivr.net/npm/onnxruntime-web@1.31.1/dist/');
const HF_BASE = abs(params.get('hf') ?? 'https://huggingface.co/momolesang/InstantHMR/resolve/main/');
const RFDETR = abs(params.get('rfdetr') ?? 'https://huggingface.co/onnx-community/rfdetr_nano-ONNX/resolve/main/onnx/model.onnx');
const LOCAL = abs('./models/');
const $ = (id) => document.getElementById(id);
const FRAME_W = 640, FRAME_H = 480;

// expected sizes in bytes: only for the progress bar
const FILES = [
  { label: 'nano', urls: [HF_BASE + 'instanthmr-nano.onnx'], size: 80_499_078 },
  { label: 'mhr', urls: [HF_BASE + 'mhr-body.onnx'], size: 12_676_712 },
  { label: 'rfdetr', urls: [RFDETR], size: 108_074_865 },
  ...[['face_detector', 418_437], ['face_landmarks', 4_920_995], ['face_blendshapes', 1_094_706], ['palm_detection_mediapipe_2023feb', 3_905_734], ['handpose_estimation_mediapipe_2023feb', 4_099_621]]
    .map(([n, size]) => ({ label: n, urls: [`${LOCAL}${n}.onnx`], size })),
];

const settings = { mirror: true, faceConfidence: 0.8, bodyConfidence: 0.8, handConfidence: 0.8, debug: false };
const state = { pose: null, xy: null, box: null, hands: [], face: null, errors: [] };
const stats = {};
for (const name of ['body', 'face', 'hands']) stats[name] = { sent: 0, done: 0, busy: false, lastTs: null, ms: 0, latency: 0, window: [] };
let av = null, follow = new BodyFollow(), handSmoother = new HandSmoother();
let renderFps = 0;
const workers = {};

// ---- scene and avatar
const { renderer, scene, camera } = createScene($('view'));
const status = (text, bad = false) => { $('status').textContent = text; $('status').style.color = bad ? '#ff8080' : ''; };
async function setAvatar(source, label) {
  const next = await loadVrm(source);
  if (av) { scene.remove(av.vrm.scene); VRMUtils.deepDispose(av.vrm.scene); }
  av = next;
  scene.add(av.vrm.scene);
  follow = new BodyFollow();
  $('avatarName').textContent = label;
}
setAvatar(abs('./avatar.vrm'), 'VRM1_Constraint_Twist_Sample').catch((e) => status(`Avatar failed to load: ${e.message ?? e}`, true));

async function useFile(file) {
  try { await setAvatar(await file.arrayBuffer(), file.name); } catch (e) { status(`Could not read ${file.name}: ${e.message ?? e}`, true); }
}
$('vrmFile').addEventListener('change', (e) => e.target.files[0] && useFile(e.target.files[0]));
addEventListener('dragover', (e) => e.preventDefault());
addEventListener('drop', (e) => { e.preventDefault(); const f = e.dataTransfer.files[0]; if (f?.name.toLowerCase().endsWith('.vrm')) useFile(f); });

// ---- settings
$('mirror').addEventListener('change', (e) => { settings.mirror = e.target.checked; workers.face?.postMessage({ type: 'config', mirror: settings.mirror }); $('cam').classList.toggle('flip', settings.mirror); $('overlay').classList.toggle('flip', settings.mirror); });
$('faceConf').addEventListener('input', (e) => { settings.faceConfidence = +e.target.value; $('faceConfV').textContent = e.target.value; workers.face?.postMessage({ type: 'config', faceConfidence: settings.faceConfidence }); });
$('bodyConf').addEventListener('input', (e) => { settings.bodyConfidence = +e.target.value; $('bodyConfV').textContent = e.target.value; });
$('handConf').addEventListener('input', (e) => { settings.handConfidence = +e.target.value; $('handConfV').textContent = e.target.value; });
$('debug').addEventListener('change', (e) => { settings.debug = e.target.checked; if (!settings.debug) { const c = $('overlay'); c.getContext('2d').clearRect(0, 0, c.width, c.height); } });

// ---- workers
function startWorker(name, file, init) {
  return new Promise((resolve, reject) => {
    const w = new Worker(new URL(file, import.meta.url), { type: 'module' });
    workers[name] = w;
    w.onerror = (e) => reject(new Error(`${name} worker: ${e.message}`));
    w.onmessage = ({ data: m }) => {
      if (m.type === 'ready') { w.onmessage = (e) => onResult(name, e.data); resolve(); }
      else if (m.type === 'error') reject(new Error(`${name}: ${m.message}`));
    };
    w.postMessage({ type: 'init', ortBase: ORT_BASE, ...init });
  });
}

function onResult(name, m) {
  const s = stats[name];
  s.busy = false;
  if (m.type === 'error') { state.errors.push(`${name}: ${m.message}`); status(`${name} stage error: ${m.message}`, true); return; }
  const now = performance.now();
  s.done++;
  s.latency = now - s.sentAt;
  s.window.push(now);
  while (s.window.length && now - s.window[0] > 3000) s.window.shift();
  if (name === 'body') {
    state.pose = m.pose; state.xy = m.xy; state.box = m.box; s.ms = m.ms.hmr + m.ms.mhr; s.detMs = m.ms.detector;
  } else if (name === 'hands') {
    state.hands = handSmoother.apply(m.hands, s.sentTs); s.ms = m.ms.palm + m.ms.hand; s.palmMs = m.ms.palm; s.handMs = m.ms.hand;
  } else {
    state.face = m; s.ms = m.ms.detector + m.ms.landmarks + m.ms.blendshapes;
  }
}

// ---- camera frames
const video = $('cam');
const grab = document.createElement('canvas');
grab.width = FRAME_W; grab.height = FRAME_H;
const gctx = grab.getContext('2d', { willReadFrequently: true });
const auto = new AutoBrightness();

function capture() {
  if (!workers.body || !Object.values(stats).some((s) => !s.busy)) return;
  gctx.drawImage(video, 0, 0, FRAME_W, FRAME_H);
  const rgba = gctx.getImageData(0, 0, FRAME_W, FRAME_H).data;
  const rgb = new Uint8Array(FRAME_W * FRAME_H * 3);
  for (let i = 0, j = 0; i < rgba.length; i += 4, j += 3) { rgb[j] = rgba[i]; rgb[j + 1] = rgba[i + 1]; rgb[j + 2] = rgba[i + 2]; }
  const ts = performance.now() / 1000;
  auto.process(rgb, FRAME_W, FRAME_H, ts);
  for (const name of ['body', 'face', 'hands']) {
    const s = stats[name];
    if (s.busy || (name === 'hands' && !state.pose)) continue; // hands run on the pose body gave for an earlier frame, as the app does
    const dt = s.lastTs === null ? 1 / 30 : ts - s.lastTs;
    s.lastTs = ts; s.busy = true; s.sent++; s.sentAt = performance.now(); s.sentTs = ts;
    const buf = rgb.slice().buffer;
    workers[name].postMessage({ type: 'frame', id: s.sent, rgb: buf, w: FRAME_W, h: FRAME_H, dt, pose: name === 'hands' ? state.pose : undefined }, [buf]);
  }
}
function captureLoop() {
  if ('requestVideoFrameCallback' in video) video.requestVideoFrameCallback(() => { capture(); captureLoop(); });
  else setTimeout(() => { capture(); captureLoop(); }, 33);
}

// ---- render loop: the avatar follows the latest result of each stage
const rest = (k) => [av.rig.nodes[k].restLocal, isHandBone(av.rig.nodes[k].name)];
let last = performance.now(), fpsFrames = 0, fpsSince = last;
function frame(now) {
  requestAnimationFrame(frame);
  const dt = Math.min((now - last) / 1000, 0.1);
  last = now;
  fpsFrames++;
  if (now - fpsSince >= 1000) { renderFps = (fpsFrames * 1000) / (now - fpsSince); fpsFrames = 0; fpsSince = now; $('fps').textContent = renderFps.toFixed(0); }
  if (av) {
    if (state.pose) {
      const bp = dropArmsWithoutElbow(toAvatarSpace(state.pose, state.hands, settings.mirror, settings.bodyConfidence, settings.handConfidence));
      applyBody(av, advanceBody(follow, solve(av.rig, bp).changed, rest, dt));
    }
    if (state.face) {
      applyHead(av, state.face.headRotation);
      applyExpressions(av, state.face.expressions);
      applyGaze(av, state.face.gaze);
    }
    av.vrm.update(dt);
    if (state.face) applyBrows(av, state.face.brows);
  }
  renderer.render(scene, camera);
  if (settings.debug) drawOverlay();
}
requestAnimationFrame(frame);

function drawOverlay() {
  const c = $('overlay');
  if (c.width !== FRAME_W) { c.width = FRAME_W; c.height = FRAME_H; }
  const g = c.getContext('2d');
  g.clearRect(0, 0, c.width, c.height);
  g.lineWidth = 2;
  if (state.box) { g.strokeStyle = '#3ddc84'; g.strokeRect(state.box.x, state.box.y, state.box.w, state.box.h); }
  g.fillStyle = '#ffd24a';
  if (state.xy) for (let i = 0; i < 70; i++) { g.beginPath(); g.arc(state.xy[i * 2], state.xy[i * 2 + 1], 2.5, 0, 6.3); g.fill(); }
  g.fillStyle = '#ff6ad5';
  for (const h of state.hands) for (const p of h.points) { g.beginPath(); g.arc(p[0], p[1], 2, 0, 6.3); g.fill(); }
}

// ---- metrics panel: measured in this browser, nothing else
function showMetrics() {
  const rows = [['body (RF-DETR every 30th frame, InstantHMR, MHR)', 'body'], ['face (detector, landmarks, blendshapes)', 'face'], ['hands (palm, landmarks)', 'hands']];
  $('metrics').innerHTML = '<tr><th>stage</th><th>compute ms</th><th>latency ms</th><th>updates/s</th></tr>' + rows.map(([label, k]) => {
    const s = stats[k];
    const rate = s.window.length > 1 ? ((s.window.length - 1) * 1000) / (s.window[s.window.length - 1] - s.window[0]) : 0;
    return `<tr><td>${label}</td><td>${s.done ? s.ms.toFixed(0) : '-'}</td><td>${s.done ? s.latency.toFixed(0) : '-'}</td><td>${rate.toFixed(1)}</td></tr>`;
  }).join('');
}
setInterval(showMetrics, 500);

// ---- start
$('start').addEventListener('click', async () => {
  $('start').disabled = true;
  try {
    status('Asking for the camera...');
    video.srcObject = await navigator.mediaDevices.getUserMedia({ video: { width: FRAME_W, height: FRAME_H }, audio: false });
    await video.play();
    $('prog').hidden = false;
    status('Downloading models (first visit only, about 215 MB)...');
    const mb = (n) => (n / 1048576).toFixed(0);
    const urls = await loadModels(FILES, ({ loaded, total, label, fromCache }) => {
      $('prog').max = total; $('prog').value = loaded;
      status(`${fromCache ? 'Reading cached' : 'Downloading'} models: ${mb(loaded)} of ${mb(total)} MB (${label})`);
    });
    $('prog').hidden = true;
    status('Starting the models (single-thread WebAssembly)...');
    const mapTable = await (await fetch(abs('./app/blendshape_map.json'))).json();
    await Promise.all([
      startWorker('body', './body-worker.js', { detectorInput: 'pixel_values', models: { rfdetr: urls.rfdetr, hmr: urls.nano, mhr: urls.mhr } }),
      startWorker('face', './face-worker.js', { models: { detector: urls.face_detector, landmarks: urls.face_landmarks, blendshapes: urls.face_blendshapes }, mapTable, mirror: settings.mirror, faceConfidence: settings.faceConfidence }),
      startWorker('hands', './hands-worker.js', { models: { palm: urls.palm_detection_mediapipe_2023feb, hand: urls.handpose_estimation_mediapipe_2023feb } }),
    ]);
    status('Tracking.');
    captureLoop();
  } catch (e) {
    $('start').disabled = false;
    $('prog').hidden = true;
    state.errors.push(String(e?.message ?? e));
    status(e.name === 'NotAllowedError' ? 'Camera permission was denied.' : `Could not start: ${e.message ?? e}`, true);
  }
});

// for the end-to-end test
window.__app = {
  stats, state, settings, urls: () => FILES,
  get ready() { return !!workers.body; },
  boneDelta(name) {
    const i = av.rig.find(name);
    const q = av.objs[i].quaternion, r = av.rig.nodes[i].restLocal;
    const d = Math.abs(q.x * r[0] + q.y * r[1] + q.z * r[2] + q.w * r[3]);
    return (2 * Math.acos(Math.min(1, d)) * 180) / Math.PI;
  },
  lookAtType: () => lookAtType(av),
};
