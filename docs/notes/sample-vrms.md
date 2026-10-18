# Sample VRM models for testing

Three freely redistributable VRM models were downloaded for testing the tracker. Each is in `samples/<name>/<name>.vrm` with a `LICENSE-NOTES.txt` beside it. Only one is VRM 1.0.

| Name | Source URL | Licence | VRM version | Size | Expressions | Notes |
|---|---|---|---|---|---|---|
| VRM1_Constraint_Twist_Sample | https://raw.githubusercontent.com/vrm-c/vrm-specification/master/samples/VRM1_Constraint_Twist_Sample/vrm/VRM1_Constraint_Twist_Sample.vrm | VRM Public License 1.0 (allows redistribution and modification) | 1.0 (specVersion 1.0) | 10,776,032 bytes (10.28 MB) | 18 presets: aa, angry, blink, blinkLeft, blinkRight, ee, happy, ih, lookDown, lookLeft, lookRight, lookUp, neutral, oh, ou, relaxed, sad, surprised | Author pixiv Inc. 13 MToon materials, 3 meshes, 24,951 vertices (not deduplicated), 18 textures, 171 nodes. Also mirrored in pixiv/three-vrm. |
| Godette | https://raw.githubusercontent.com/V-Sekai/godot-vrm/master/vrm_samples/Godette_vrm_v4.vrm | CC BY 3.0 | 0.x (exporter saturday06_blender_vrm_exporter_experimental_1.10.0) | 591,288 bytes (0.56 MB) | 17 blendshape groups: a, angry, blink, blink_l, blink_r, e, fun, i, joy, lookdown, lookleft, lookright, lookup, neutral, o, sorrow, u | Original model SirRichard94 (https://github.com/SirRichard94/low-poly-godette), VRM adaptation Lyuma. 2 meshes, 8,067 vertices, 4 materials, 5 textures, 71 nodes. The `VRMC_materials_mtoon` name appears in extensionsUsed, but no material-level MToon extension was counted. Check this before relying on MToon. |
| 100Avatars_031_Devil | https://arweave.net/gfVzs1oH_aPaHVxpQK86HT_rqzyrFPOUKUrDJ30yprs (302 redirect, use curl -L). Registry: https://github.com/ToxSam/open-source-avatars | CC0 1.0 | 0.x (exporter saturday06_blender_vrm_exporter_experimental_2.4.8) | 1,393,048 bytes (1.33 MB) | 6 blendshape groups: a, blink, e, i, o, u | Author Polygonal Mind (100Avatars R1). 1 mesh, 2,645 vertices, 1 material, 1 texture, 67 nodes. |

## Baseline (not part of the download)

`samples/Seed-san.vrm` (10,917,800 bytes) is the existing project model. It was not modified.

## Rejected candidates

- The four official VRM feature-test samples are under 100 KB.
- Alicia Solid: the licence does not clearly permit redistribution, and its terms are in Japanese and are not sublicensable.
- Elmer.vrm in bevy_vrm1: no licence or credit information.
- VRoid CC0 sets found by search: VRM 0.x, and no verified licence file.
- Gemma: paid.

## Gaps

No clearly licensed fourth VRM 1.0 model was found. The only VRM 1.0 sample is the Twist sample.
