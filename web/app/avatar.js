// three.js scene, VRM loading, the rig.json-shaped rig of the loaded VRM, and the avatar side of the app:
// body bones (solve + advanceBody output), head (head.rs), expressions, brow morphs (brows.rs), gaze (gaze.rs).
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { VRMLoaderPlugin, VRMUtils } from '@pixiv/three-vrm';
import { buildRig } from '../body/solve.js';

export function createScene(canvas) {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
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
  const resize = () => {
    const { clientWidth: w, clientHeight: h } = canvas;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  };
  addEventListener('resize', resize);
  resize();
  return { renderer, scene, camera };
}

/** Loads a VRM (0.x or 1.0) from a URL or an ArrayBuffer and returns `{ vrm, rig, objs, gltf }`. Nothing leaves the browser. */
export async function loadVrm(source) {
  const loader = new GLTFLoader();
  loader.register((parser) => new VRMLoaderPlugin(parser));
  const gltf = await (typeof source === 'string' ? loader.loadAsync(source) : loader.parseAsync(source, ''));
  const vrm = gltf.userData.vrm;
  if (!vrm) throw new Error('This file is not a VRM model.');
  VRMUtils.rotateVRM0(vrm); // no-op for VRM 1.0: puts a 0.x model facing +z like a 1.0 one
  vrm.scene.traverse((o) => (o.frustumCulled = false));
  vrm.humanoid.autoUpdateHumanBones = false; // the solver writes the raw bones itself
  const { rig, objs } = rigFromVrm(gltf, vrm);
  return { vrm, rig, objs, gltf };
}

/** The `rig.json` shape (see web-fixtures README) from the loaded model, taken at rest: humanoid bones in glTF node order,
 *  each preceded by its not-yet-listed ancestors; lower-case bone names; global rotation and position from the scene. */
export function rigFromVrm(gltf, vrm) {
  vrm.scene.updateMatrixWorld(true);
  const nodeIndex = (o) => gltf.parser.associations.get(o)?.nodes;
  const bones = Object.keys(vrm.humanoid.humanBones)
    .map((name) => ({ name, obj: vrm.humanoid.getRawBoneNode(name) }))
    .filter((b) => b.obj)
    .sort((a, b) => nodeIndex(a.obj) - nodeIndex(b.obj));
  const names = new Map(bones.map((b) => [b.obj, b.name.toLowerCase()]));
  const order = [];
  for (const { obj } of bones) {
    const chain = [];
    for (let o = obj; o && o !== vrm.scene; o = o.parent) chain.push(o);
    for (const o of chain.reverse()) if (!order.includes(o)) order.push(o);
  }
  const nodes = order.map((o) => {
    const p = new THREE.Vector3(), q = new THREE.Quaternion(), s = new THREE.Vector3();
    o.matrixWorld.decompose(p, q, s);
    const parent = order.indexOf(o.parent);
    return {
      name: names.get(o) ?? null, parent: parent < 0 ? null : parent,
      restLocal: o.quaternion.toArray(), restGlobalRot: q.toArray(), restGlobalPos: p.toArray(),
    };
  });
  return { rig: buildRig(nodes), objs: order };
}

const quat = (a) => new THREE.Quaternion(a[0], a[1], a[2], a[3]);

/** Writes `[nodeIndex, [x, y, z, w]][]` (advanceBody output) as local rotations of the rig's nodes. */
export function applyBody(av, followed) {
  for (const [i, q] of followed) av.objs[i].quaternion.set(q[0], q[1], q[2], q[3]);
}

/** head.rs: local rotation that moves the head bone by the tracked world rotation, minus what its parent already turned. */
export function applyHead(av, headRotation) {
  const { rig, objs } = av;
  const h = rig.find('head');
  if (h == null) return;
  const node = rig.nodes[h];
  let delta = quat(headRotation);
  if (node.parent != null) {
    objs[node.parent].updateWorldMatrix(true, false);
    const now = objs[node.parent].getWorldQuaternion(new THREE.Quaternion());
    const motion = now.multiply(quat(rig.nodes[node.parent].restGlobalRot).invert()); // now * rest^-1
    delta = motion.invert().multiply(delta);
  }
  const g = quat(node.restGlobalRot);
  objs[h].quaternion.copy(quat(node.restLocal).multiply(g.clone().invert()).multiply(delta).multiply(g));
}

/** Expression weights (0..1) by VRM expression name; unknown names are ignored by three-vrm. */
export function applyExpressions(av, expressions) {
  const m = av.vrm.expressionManager;
  if (!m) return;
  for (const [name, w] of Object.entries(expressions)) if (!Number.isNaN(w)) m.setValue(name, Math.min(Math.max(w, 0), 1));
}

// brows.rs brow_weight: weight for a morph target name, or null when it is not a brow or lip target
function browWeight(name, b) {
  const n = name.toLowerCase();
  const direct = { mouth_wide: b.wide, lip_o: b.funnel, lip_u: b.pucker, lip_up: b.lipUp, lip_down: b.lipDown };
  if (n in direct) return direct[n];
  if (!n.includes('brow')) return null;
  const down = n.includes('down'), up = n.includes('up');
  if (down === up) return null;
  const [l, r] = down ? [b.downLeft, b.downRight] : [b.upLeft, b.upRight];
  if (/(_l|\.l|left)$/.test(n)) return l;
  if (/(_r|\.r|right)$/.test(n)) return r;
  return (l + r) / 2;
}

/** Direct brow and lip morph control for models that ship such morph targets (after vrm.update, which would reset them). */
export function applyBrows(av, brows) {
  av.vrm.scene.traverse((o) => {
    if (!o.morphTargetDictionary) return;
    for (const [name, i] of Object.entries(o.morphTargetDictionary)) {
      const w = browWeight(name, brows);
      if (w !== null) o.morphTargetInfluences[i] = w;
    }
  });
}

/** gaze.rs: for bone-type look-at models a target sits in front of the face and moves with the tracked gaze. */
export function applyGaze(av, gaze) {
  const la = av.vrm.lookAt;
  if (!la || la.applier?.type !== 'bone') return; // expression-type models take gaze through the lookLeft/Right/Up/Down expressions
  const head = av.vrm.humanoid.getRawBoneNode('head');
  av.gazeTarget ??= new THREE.Object3D();
  const p = head.getWorldPosition(new THREE.Vector3());
  av.gazeTarget.position.set(p.x + gaze.x, p.y + gaze.y, p.z + 1);
  av.gazeTarget.updateMatrixWorld(true);
  la.target = av.gazeTarget;
}

export const lookAtType = (av) => av.vrm.lookAt?.applier?.type ?? 'none';
