# Blendshape model notes (task 1.5, cpu-face-tracking-spike)

Downloaded inputs were untrusted; read only, no inference run. Python used with `-I`.

## Model artifact
- Hub repo: `fernandotonon/QtMeshEditor-faceblendshapes-onnx`
- Hub revision (sha): `495864b0bae88b0b926b8da8cb85f9022ceaa402` (lastModified 2026-07-13T13:16:07Z). Source: Hub `/api/models/...` (`hf_model_api.json`).
- File: `face_blendshapes.onnx`, 1094706 bytes
- SHA256: `37cf54d3ccf671f8936427437a1a0b627799e16f20439de3cc35aa189ad40c66` (computed locally; matches `sha256sum`)
- Hub LFS oid: NOT FOUND (Hub siblings list omits lfs metadata; only the local hash is recorded)
- ONNX: IR version 8, opset `ai.onnx` 18, `ai.onnx.ml` 2, 307 nodes, no metadata_props. Source: `onnx` package via `python -I` in a separate venv.
- Graph input: `serving_default_input_points:0`, float32, shape `[1, 146, 2]`. Source: ONNX graph inspection.
- Graph output: `StatefulPartitionedCall:0`, float32, shape `[52]`. Source: ONNX graph inspection.
- Graph contains a `Sigmoid` op, consistent with scores in [0,1]. Source: ONNX op list.

## Input units and coordinates
- Input is 2D pixel coordinates, not normalized, z dropped. Source: `face_blendshapes_graph.cc` (`ConvertLandmarksToTensor`: `AddOption X`, `AddOption Y`, `set_flatten(false)`); Hub README ("pixel coords (`x·imgW, y·imgH`)"); `export-facecap-onnx.py` lines 507-508 (`[:, :2]`).
- Scale: x multiplied by image width, y by image height. Source: `landmarks_to_tensor_calculator.cc` `GetAttributeScale` (X and Z use `image_size.first`, Y uses `image_size.second`); `GetAttribute` returns raw `x()`/`y()`.
- Origin and reference frame: no offset is applied in `ConvertLandmarksToTensor`, only the scale above, so the origin is the image origin. The blendshape subgraph takes `NORM_LANDMARKS` from the face landmarks detector and `IMAGE_SIZE` from `ImagePropertiesCalculator` on the full input image (`flg.cc` lines 438 and 444, 536). So the pixels are full-image pixels, not face-crop pixels. Inferred from these wiring lines. The projection of crop landmarks back to full-image coords happens in `face_landmarks_detector_graph.cc`, which was not fetched, so that step is unverified. Relevant to task 3.14 crop math.
- Layout: `[1, 146, 2]` with columns (x, y), landmark order = subset order below. Source: `flatten=false`, attribute order X then Y.

## Output order (52 scores)
Index i = `kBlendshapeNames[i]`, taken from `face_blendshapes_graph.cc` (commit `ce3dff2c`) lines 63-115. Label map ids set to i, `top_k=0`, `min_score_threshold=-1`. Source: `ConvertTensorToBlendshapes` in the same file, `TensorsToClassificationCalculator` (`classification->set_index(i)`, `set_score(raw_scores[i])`).

0=_neutral, 1=browDownLeft, 2=browDownRight, 3=browInnerUp, 4=browOuterUpLeft, 5=browOuterUpRight, 6=cheekPuff, 7=cheekSquintLeft, 8=cheekSquintRight, 9=eyeBlinkLeft, 10=eyeBlinkRight, 11=eyeLookDownLeft, 12=eyeLookDownRight, 13=eyeLookInLeft, 14=eyeLookInRight, 15=eyeLookOutLeft, 16=eyeLookOutRight, 17=eyeLookUpLeft, 18=eyeLookUpRight, 19=eyeSquintLeft, 20=eyeSquintRight, 21=eyeWideLeft, 22=eyeWideRight, 23=jawForward, 24=jawLeft, 25=jawOpen, 26=jawRight, 27=mouthClose, 28=mouthDimpleLeft, 29=mouthDimpleRight, 30=mouthFrownLeft, 31=mouthFrownRight, 32=mouthFunnel, 33=mouthLeft, 34=mouthLowerDownLeft, 35=mouthLowerDownRight, 36=mouthPressLeft, 37=mouthPressRight, 38=mouthPucker, 39=mouthRight, 40=mouthRollLower, 41=mouthRollUpper, 42=mouthShrugLower, 43=mouthShrugUpper, 44=mouthSmileLeft, 45=mouthSmileRight, 46=mouthStretchLeft, 47=mouthStretchRight, 48=mouthUpperUpLeft, 49=mouthUpperUpRight, 50=noseSneerLeft, 51=noseSneerRight

ARKit check: the "51 + _neutral" claim is LOCAL-ONLY. Apple's `ARFaceAnchor.BlendShapeLocation` page (developer.apple.com/documentation/arkit/arfaceanchor/blendshapelocation) returned a JS shell with no name list on fetch (2026-10-17). The `jawopen` sub-page confirms `jawOpen` only. The full 52-name ARKit list is NOT sourced. Membership of names 1..51 in ARKit is not verified against an external list.

Consequence for this project: output has `_neutral` at index 0 and no `tongueOut`; ARKit names 1..51 map one-to-one.

Cross-checks (re-run 2026-10-17, `python -I`):
- Export script `BLENDSHAPE_NAMES` (`export-facecap-onnx.py` lines 99-113, local copy): 52 names, equal in order to the MediaPipe list. Verified.
- Hub README (`hf_readme.md`, revision `495864b0`): names only 7 entries with ellipses (`_neutral`=0, `browDownLeft`=1, `jawOpen`=25, `mouthSmileLeft`=44, `mouthSmileRight`=45, `eyeBlinkLeft`=9, `eyeBlinkRight`=10). All 7 match the list above. The full order is NOT stated in the README, so the README cross-check is partial.
- Export script source URL: NOT CONFIRMED. The raw GitHub URL returned 14 bytes (404). The checked copy is the local temp file.

## 146-landmark subset
- Saved at `data/landmark_subset_146.json`: JSON array of 146 integers, sorted, unique, range 0-477.
- Source: `kLandmarksSubsetIdxs` in MediaPipe `mediapipe/tasks/cc/vision/face_landmarker/face_blendshapes_graph.cc` at commit `ce3dff2c58dc711482e4730125e117d569ce343c`, declared on line 52 and closed on line 62 (re-fetched at that commit, 2026-10-17). Verified programmatically: identical to the list. Also identical to `BLENDSHAPE_LANDMARK_SUBSET` in `export-facecap-onnx.py` (lines 86-97, header comment line 85). Export source path: see Cross-checks.

## Licenses
- Model (fernandotonon Hub repo): apache-2.0 (Hub `cardData.license`; Hub README License section).
- MediaPipe code and weights: Apache-2.0 (`MOCAP_SPIKE.md` line 29; MediaPipe `landmarks_to_tensor_calculator.proto` header).
- py-feat `mp_blendshapes` card: apache-2.0 (`pyfeat_api.json`), lastModified 2024-09-02, sha `1c8bbe252efbf7afd0535a9881b62ec195e73d29`.
- Training data license: NOT FOUND. Not stated in the model Hub card or the README; MediaPipe training data not documented in the fetched sources.

## Not found / open
- Training data license: NOT FOUND (see above).
- Hub LFS oid: NOT FOUND (see above).
- py-feat `feat.utils` subset cross-check: not performed (not required for the recorded list).
