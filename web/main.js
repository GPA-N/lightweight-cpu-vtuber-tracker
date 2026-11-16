import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { VRMLoaderPlugin, VRMUtils } from '@pixiv/three-vrm';
import { FilesetResolver, FaceLandmarker, PoseLandmarker } from '@mediapipe/tasks-vision';

const MP = 'https://cdn.jsdelivr.net/npm/@mediapipe/tasks-vision@1.2.3/wasm';
const MODELS = 'https://storage.googleapis.com/mediapipe-models';
const FACE_MODEL = `${MODELS}/face_landmarker/face_landmarker/float16/1/face_landmarker.task`;
const POSE_MODEL = `${MODELS}/pose_landmarker/pose_landmarker_lite/float16/1/pose_landmarker_lite.task`;
const SMOOTH = 0.4; // per-frame slerp/lerp factor; the real tracker uses One Euro filters
const MIN_VISIBILITY = 0.5;

const video = document.getElementById('cam');
const canvas = document.getElementById('view');
const status = document.getElementById('status');
const startButton = document.getElementById('start');

const renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x202428);
scene.add(new THREE.HemisphereLight(0xffffff, 0x444444, 1.6));
const sun = new THREE.DirectionalLight(0xffffff, 1.4);
sun.position.set(1, 2, 2);
scene.add(sun);
const camera = new THREE.PerspectiveCamera(30, 4 / 3, 0.1, 20);
camera.position.set(0, 1.25, 1.9);
camera.lookAt(0, 1.15, 0);

function resize() {
  const { clientWidth: w, clientHeight: h } = canvas;
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}
addEventListener('resize', resize);
resize();

let vrm = null;
const loader = new GLTFLoader();
loader.register((parser) => new VRMLoaderPlugin(parser));
loader.load(
  './avatar.vrm',
  (gltf) => {
    vrm = gltf.userData.vrm;
    VRMUtils.removeUnnecessaryVertices(gltf.scene);
    VRMUtils.combineSkeletons(gltf.scene);
    VRMUtils.combineMorphs(vrm);
    vrm.scene.traverse((o) => (o.frustumCulled = false));
    VRMUtils.rotateVRM0(vrm); // no-op for VRM 1.0
    scene.add(vrm.scene);
  },
  undefined,
  (e) => (status.textContent = `Avatar failed to load: ${e.message ?? e}`),
);

// Targets written by the tracker, eased onto the avatar every frame.
const target = {
  head: new THREE.Quaternion(),
  expressions: {},
  arms: {}, // bone name -> local quaternion
};
const REST = {
  left: new THREE.Vector3(1, 0, 0),
  right: new THREE.Vector3(-1, 0, 0),
};
const RELAXED = {
  left: new THREE.Vector3(0.3, -1, 0).normalize(),
  right: new THREE.Vector3(-0.3, -1, 0).normalize(),
};

function score(blend, name) {
  return blend[name] ?? 0;
}

// The preview is mirrored like the real tracker: the avatar's left side follows the user's right side.
function applyFace(result) {
  const matrix = result.facialTransformationMatrixes?.[0];
  const categories = result.faceBlendshapes?.[0]?.categories;
  if (!matrix || !categories) return;
  const blend = Object.fromEntries(categories.map((c) => [c.categoryName, c.score]));
  const q = new THREE.Quaternion().setFromRotationMatrix(new THREE.Matrix4().fromArray(matrix.data));
  target.head.set(q.x, -q.y, -q.z, q.w); // reflect across the x axis
  const e = target.expressions;
  e.blinkLeft = score(blend, 'eyeBlinkRight');
  e.blinkRight = score(blend, 'eyeBlinkLeft');
  e.aa = score(blend, 'jawOpen');
  e.ou = score(blend, 'mouthPucker');
  e.oh = score(blend, 'mouthFunnel');
  e.happy = (score(blend, 'mouthSmileLeft') + score(blend, 'mouthSmileRight')) / 2;
}

function aim(side, shoulder, elbow, wrist) {
  const visible = [shoulder, elbow, wrist].every((p) => (p.visibility ?? 1) >= MIN_VISIBILITY);
  // MediaPipe world axes: x to the image right, y down, z away from the camera. The avatar faces +z, y up, mirrored.
  const dir = (a, b) => new THREE.Vector3(-(b.x - a.x), -(b.y - a.y), -(b.z - a.z)).normalize();
  const upper = visible ? dir(shoulder, elbow) : RELAXED[side];
  const lower = visible ? dir(elbow, wrist) : RELAXED[side];
  const qUpper = new THREE.Quaternion().setFromUnitVectors(REST[side], upper);
  const qLower = new THREE.Quaternion().setFromUnitVectors(REST[side], lower);
  target.arms[`${side}UpperArm`] = qUpper;
  target.arms[`${side}LowerArm`] = qUpper.clone().invert().multiply(qLower); // local to the upper arm
}

function applyPose(result) {
  const w = result.worldLandmarks?.[0];
  const v = result.landmarks?.[0];
  if (!w || !v) {
    aimRelaxed();
    return;
  }
  const withVisibility = (i) => ({ ...w[i], visibility: v[i].visibility });
  // Mirror: avatar left uses the user's right landmarks (12, 14, 16), avatar right the user's left (11, 13, 15).
  aim('left', withVisibility(12), withVisibility(14), withVisibility(16));
  aim('right', withVisibility(11), withVisibility(13), withVisibility(15));
}

function aimRelaxed() {
  for (const side of ['left', 'right']) {
    const q = new THREE.Quaternion().setFromUnitVectors(REST[side], RELAXED[side]);
    target.arms[`${side}UpperArm`] = q;
    target.arms[`${side}LowerArm`] = new THREE.Quaternion();
  }
}
aimRelaxed();

function drive(dt) {
  if (!vrm) return;
  const humanoid = vrm.humanoid;
  const neck = humanoid.getNormalizedBoneNode('neck');
  const head = humanoid.getNormalizedBoneNode('head');
  const identity = new THREE.Quaternion();
  const part = (f) => identity.clone().slerp(target.head, f);
  neck?.quaternion.slerp(part(0.4), SMOOTH);
  head?.quaternion.slerp(part(0.6), SMOOTH);
  for (const [name, q] of Object.entries(target.arms)) {
    humanoid.getNormalizedBoneNode(name)?.quaternion.slerp(q, SMOOTH);
  }
  const manager = vrm.expressionManager;
  for (const [name, value] of Object.entries(target.expressions)) {
    const now = manager.getValue(name) ?? 0;
    manager.setValue(name, now + (value - now) * SMOOTH);
  }
  vrm.update(dt);
}

let face = null;
let pose = null;
let lastVideoTime = -1;
let last = performance.now();
let fpsWindow = [];

function frame(now) {
  requestAnimationFrame(frame);
  const dt = Math.min((now - last) / 1000, 0.1);
  last = now;
  if (face && video.readyState >= 2 && video.currentTime !== lastVideoTime) {
    lastVideoTime = video.currentTime;
    const t0 = performance.now();
    applyFace(face.detectForVideo(video, now));
    applyPose(pose.detectForVideo(video, now));
    fpsWindow.push(performance.now() - t0);
    if (fpsWindow.length === 30) {
      const ms = fpsWindow.reduce((a, b) => a + b, 0) / fpsWindow.length;
      status.textContent = `Tracking: ${ms.toFixed(1)} ms per frame (face + pose) in this browser.`;
      fpsWindow = [];
    }
  }
  drive(dt);
  renderer.render(scene, camera);
}
requestAnimationFrame(frame);

async function createLandmarker(Task, fileset, modelAssetPath, options) {
  for (const delegate of ['GPU', 'CPU']) {
    try {
      return await Task.createFromOptions(fileset, { baseOptions: { modelAssetPath, delegate }, runningMode: 'VIDEO', ...options });
    } catch (e) {
      if (delegate === 'CPU') throw e;
    }
  }
}

startButton.addEventListener('click', async () => {
  startButton.disabled = true;
  try {
    status.textContent = 'Asking for the camera...';
    video.srcObject = await navigator.mediaDevices.getUserMedia({ video: { width: 640, height: 480 }, audio: false });
    await video.play();
    status.textContent = 'Loading tracking models (about 10 MB)...';
    const fileset = await FilesetResolver.forVisionTasks(MP);
    [face, pose] = await Promise.all([
      createLandmarker(FaceLandmarker, fileset, FACE_MODEL, {
        outputFaceBlendshapes: true,
        outputFacialTransformationMatrixes: true,
        numFaces: 1,
      }),
      createLandmarker(PoseLandmarker, fileset, POSE_MODEL, { numPoses: 1 }),
    ]);
    status.textContent = 'Tracking.';
  } catch (e) {
    startButton.disabled = false;
    status.textContent = e.name === 'NotAllowedError' ? 'Camera permission was denied.' : `Could not start: ${e.message ?? e}`;
  }
});
