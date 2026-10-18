# VRM parse spike (task 3.1, trial-kiss3d-renderer)

Throwaway crate: `fixtures/private/spikes/vrm-parse` (serde_vrm 0.5.0, `default-features = false`, no bevy). Parse only. Run with the four sample paths as arguments.

Result: 3 of 4 models parse. Godette (VRM 0.x) fails. Task 3.1 is NOT ticked.

## Per-model table

| Model | Spec | Parse | head | neck | leftEye | rightEye | lookAt | Springs | Constraints | License |
|---|---|---|---|---|---|---|---|---|---|---|
| Seed-san.vrm | 1.0 | OK | node 45 (`head 1`) | node 44 (`neck`) | absent | absent | Expression | 9 springs, 8 colliders | 23 | licenseUrl `https://vrm.dev/licenses/1.0/` |
| VRM1_Constraint_Twist_Sample.vrm | 1.0 | OK | node 25 (`J_Bip_C_Head`) | node 24 (`J_Bip_C_Neck`) | node 26 (`J_Adj_L_FaceEye`) | node 27 (`J_Adj_R_FaceEye`) | Bone | 22 springs, 13 colliders | 14 | licenseUrl `https://vrm.dev/licenses/1.0/` |
| Godette.vrm | 0.x (exporterVersion `saturday06_blender_vrm_exporter_experimental_1.10.0`) | FAIL | not reached | not reached | not reached | not reached | not reached | not reached | not reached | not reached (parse failed before meta) |
| 100Avatars_031_Devil.vrm | 0.x | OK (migrated) | node 39 (`mixamorig:Head`) | node 38 (`mixamorig:Neck`) | absent | absent | Bone (raw `Bone`) | 0 (boneGroups 0) | 0 | raw licenseName `CC0` |

Expression presets after migration (0.x) or as parsed (1.0), with binds `(node, index, weight)`:

- Seed-san (1.0, as parsed): happy `(2,33,1.0)`, angry `(2,34,1.0)`, sad `(2,35,1.0)`, relaxed `(2,36,1.0)`, surprised `(2,38,1.0)`, aa `(2,25,1.0)`, ih `(2,26,1.0)`, ou `(2,27,1.0)`, ee `(2,28,1.0)`, oh `(2,29,1.0)`, blink `(2,1,1.0)`,`(2,2,1.0)`, blinkLeft `(2,1,1.0)`, blinkRight `(2,2,1.0)`, lookUp `(2,39,1.0)`, lookDown `(2,40,1.0)`, lookLeft `(2,41,1.0)`, lookRight `(2,42,1.0)`, neutral none.
- VRM1_Constraint_Twist (1.0, as parsed): happy `(1,3,1.0)`, angry `(1,1,1.0)`, sad `(1,4,1.0)`, relaxed `(1,2,1.0)`, surprised `(1,5,1.0)`, aa `(1,36,1.0)`, ih `(1,37,1.0)`, ou `(1,38,1.0)`, ee `(1,39,1.0)`, oh `(1,40,1.0)`, blink `(1,12,1.0)`, blinkLeft `(1,13,1.0)`, blinkRight `(1,14,1.0)`, neutral `(1,0,1.0)`. lookUp/lookDown/lookLeft/lookRight have no binds.
- Devil (0.x, migrated): aa `(65,2,1.0)`, ee `(65,5,1.0)`, ih `(65,7,1.0)`, oh `(65,10,1.0)`, ou `(65,11,1.0)`, blink `(65,1,1.0)`.

## 0.x to 1.0 migration check (Devil)

Raw 0.x JSON compared with migrated output:

| Raw presetName | Raw mesh | Raw index | Raw weight | Migrated expression | Migrated node | Migrated index | Migrated weight |
|---|---|---|---|---|---|---|---|
| a | 0 | 2 | 100 | aa | 65 | 2 | 1.0 |
| e | 0 | 5 | 100 | ee | 65 | 5 | 1.0 |
| i | 0 | 7 | 100 | ih | 65 | 7 | 1.0 |
| o | 0 | 10 | 100 | oh | 65 | 10 | 1.0 |
| u | 0 | 11 | 100 | ou | 65 | 11 | 1.0 |
| blink | 0 | 1 | 100 | blink | 65 | 1 | 1.0 |

Mesh 0 maps to exactly one node (65). Index is unchanged. Weight is divided by 100. Preset names are mapped by hand. Blink, Joy and Sorrow mappings were not exercised by Devil (Devil has no joy/sorrow group).

## What serde_vrm 0.5.0 does not cover

- Whole-document 0.x to 1.0 migration. Only `BoneName` to `HumanBoneName` (thumb shift) and `FirstPersonFlag` to `FirstPersonType` are provided.
- Preset name mapping (a to aa, joy to happy, sorrow to sad, fun to relaxed, and so on).
- Blend bind mesh index to node index. Needs a glTF scan for the node that references the mesh.
- Weight scaling 0..100 to 0..1.

All four are implemented by hand in the spike. The migrated values were checked against raw JSON as above.

## Parse errors (exact)

- Godette.vrm: `VRM: invalid length 12, expected fewer elements in array`
  - Likely cause: serde_vrm `LookAtCurve.curve` is `Option<[f32; 8]>` (vrm0/mod.rs), and Godette's look-at curves have more than 8 entries (12 in `lookAtHorizontalOuter.curve` and `lookAtVerticalUp.curve`, per an earlier raw-JSON check). Not re-checked in this pass.
  - serde stops at the first error, so the other sections of Godette (humanoid, blend shapes, spring bones) are still unverified.

No parse errors for the other three models.

## Gaps and notes

- Eyes are absent from the humanoid map in Seed-san and Devil. Both are optional in the spec. The spike reports them as absent and does not invent a fallback.
- Constraints: Seed-san 23, VRM1_Constraint_Twist 14. All parsed through `VrmcNodeConstraint`.
- 0.x spring count uses `secondaryAnimation.boneGroups` length (raw count, not typed).
- Devil's 0.x `lookAtTypeName` is `Bone`.

## Build measurements (spike crate, CARGO_BUILD_JOBS=4, spike-owned target dir)

| Build | Wall time | exe size |
|---|---|---|
| debug (cold; includes dependency compile) | about 11 s | 1,653,760 bytes |
| debug (incremental, spike only) | 3 s | same |
| release (serde_vrm + spike) | 19.6 s | 840,192 bytes |

Debug time is approximate: the first debug run also compiled dependencies and hit a spike compile error at the same time.

## Verdict

- Parser approach (manual GLB JSON-chunk split, then serde_vrm per extension) works for VRM 1.0 and VRM 0.x Devil.
- serde_vrm cannot read Godette (VRM 0.x) because of the 8-element look-at curve. A workaround (for example a custom curve type) is needed before the renderer can rely on 0.x files.
- Task 3.1 remains open: not all four parse.
