# Model and dependency licenses (task 1.9, cpu-face-tracking-spike)

Research only, not legal advice. Every row is backed by a fetched source URL. Anything not verified is marked NOT FOUND, UNVERIFIED, or INFERRED.

Status key: CLEAR = primary source confirms both code and weights license. CLEAR (code only, no weights) = primary source confirms the code license; weights are not part of the claim. CONDITIONAL = license claim found only in mirrors or secondary text; sign-off pending. UNRESOLVED = missing source, conflicting source, or prohibited.

## Model table

| # | Model / artifact | License id | Commercial use | Redistribute (embed in .exe) | Attribution required | Status | Source URL |
|---|---|---|---|---|---|---|---|
| 1a | senty-au/face_landmarks_detector-ONNX | apache-2.0 (Hub YAML `License: apache-2.0`); card: "Apache-2.0, the same as the original model. The model and its weights are © Google LLC." | Allowed under Apache-2.0 terms (Hub claim only) | Allowed under Apache-2.0 terms; ship license and NOTICE (Apache section 4) | Yes: "© Google LLC" line | CONDITIONAL (no Google primary source) | https://huggingface.co/senty-au/face_landmarks_detector-ONNX |
| 1b | Google MediaPipe Face Landmarker bundle (upstream of 1a, 2, 3a, 3b) | NOT FOUND in any Google page. Third-party text only: "Apache-2.0, code AND models" (THIRD_PARTY_AI_MODELS.md, unsourced assertion); MOCAP_SPIKE.md: "all five MediaPipe models Apache-2.0 (code AND weights)" (unsourced assertion) | Unconfirmed | Unconfirmed | Unconfirmed | UNRESOLVED | https://developers.google.com/edge/mediapipe/solutions/vision/face_landmarker |
| 2 | mocap/face in fernandotonon/QtMeshEditor-models: face_landmarks.onnx, face_detector.onnx, face_blendshapes.onnx | apache-2.0 per mocap/NOTICE.txt, which names only the Face Landmarker and Pose Landmarker graphs. File-to-graph mapping INFERRED. | Allowed under Apache-2.0 terms (if mapping and upstream hold) | Same, with NOTICE copy | Yes: NOTICE text | CONDITIONAL (depends on 1b) | https://huggingface.co/fernandotonon/QtMeshEditor-models/resolve/main/mocap/NOTICE.txt |
| 3 | fernandotonon/QtMeshEditor-faceblendshapes-onnx (blendshape MLP-Mixer) | apache-2.0 (Hub YAML `license: apache-2.0`); README: "Apache-2.0 — this graph is a direct ONNX conversion of a Google MediaPipe model (Apache-2.0 code AND weights)." | Allowed under Apache-2.0 terms (Hub claim only) | Same | Yes: Apache-2.0 notices | UNRESOLVED (training data NOT FOUND; depends on 1b) | https://huggingface.co/fernandotonon/QtMeshEditor-faceblendshapes-onnx |
| 3a | fernandotonon/QtMeshEditor-blazeface-onnx (BlazeFace short range; likely origin of face_detector.onnx, INFERRED) | apache-2.0 (Hub YAML); README: "Apache-2.0 — this graph is a direct ONNX conversion of a Google MediaPipe model (Apache-2.0 code AND weights)." | Allowed under Apache-2.0 terms (Hub claim only) | Same | Yes: Apache-2.0 notices | CONDITIONAL (depends on 1b; training data NOT FOUND) | https://huggingface.co/fernandotonon/QtMeshEditor-blazeface-onnx/raw/main/README.md |
| 3b | fernandotonon/QtMeshEditor-facemesh-onnx (Face Mesh V2; likely origin of face_landmarks.onnx, INFERRED) | apache-2.0 (Hub YAML); README: "Google MediaPipe's Face Landmarks Detector (Face Mesh V2), converted to ONNX."; same Apache-2.0 code AND weights line | Allowed under Apache-2.0 terms (Hub claim only) | Same | Yes: Apache-2.0 notices | CONDITIONAL (depends on 1b; training data NOT FOUND) | https://huggingface.co/fernandotonon/QtMeshEditor-facemesh-onnx/raw/main/README.md |
| 4 | karie666666/face-au-arkit-blendshapes (fallback) | No YAML license field. Body "## License" line 120: "MIT License". Repo-level text only. | MIT for the weights is unsupported | MIT for the weights is unsupported | Unsupported for weights | UNRESOLVED (DECA/EMOCA lineage unresolved) | https://huggingface.co/karie666666/face-au-arkit-blendshapes/raw/main/README.md |
| 4a | DECA (YadiraF/DECA), cited by 4 | "Software Copyright License for non-commercial scientific research purposes" | Prohibited | Prohibited without Max-Planck written permission | Cite paper | UNRESOLVED (non-commercial; rejected) | https://github.com/YadiraF/DECA/blob/master/LICENSE |
| 4b | EMOCA (radekd91/emoca), cited by 4 | Same non-commercial research text | Prohibited | Prohibited | Cite paper | UNRESOLVED (non-commercial; rejected) | https://github.com/radekd91/emoca/blob/master/LICENSE |
| 4c | AUBlendNet (wslh852/AUBlendNet), cited by 4 | Code only: "MIT License" / "Copyright (c) 2023 DoubleXING". Weights license NOT FOUND (Baidu Pan link, no stated terms). | MIT for code; weights NOT FOUND | Code MIT text; weights NOT FOUND | MIT notice for code | UNRESOLVED (weights) | https://raw.githubusercontent.com/wslh852/AUBlendNet/main/LICENSE |
| 5 | seed-san.vrm (VRM 1.0 avatar, bundled asset) | VRM Public License 1.0 | Meta `commercialUsage`: "corporation" (conditional) | Meta `allowRedistribution`: true | Meta `creditNotation`: "required" | CONDITIONAL (see seed-san.md) | docs/notes/seed-san.md (local note; verified through raw HTML; upstream URL NOT recorded here) |

### Per-model record from THIRD_PARTY_AI_MODELS.md and the models README

- THIRD_PARTY_AI_MODELS.md lists five #869 graphs (lines 19 to 23): blazeface, facemesh, faceblendshapes, blazepose, poselandmarks, with their HF repos. Line 24 adds a sixth: depthanything.
- Face Landmarker section (lines 516 to 526): "Models: Google MediaPipe `face_landmarker.task` (BlazeFace short-range detector + Face Mesh V2 478-landmark model + 52-blendshape MLP-Mixer)"; "License: **Apache-2.0, code AND models**". This is a third-party assertion. It cites no Google page and no Google model-license text.
- Lines 513 to 514: "MediaPipe FaceMesh (Apache-2.0) is the clean choice." Same unsourced assertion.
- Line 534: DECA, EMOCA, SPECTRE rejected as "research-only license".
- Models README (https://huggingface.co/fernandotonon/QtMeshEditor-models/raw/main/README.md): YAML `license: other`, `license_name: per-model`. Its per-folder table has no mocap or face row. Per-folder licenses listed: pbrify CC0-1.0; realesrgan BSD-3-Clause; unirig, skintokens, triposr, triposg MIT; rmib, segment CC-BY-4.0; t2m CC0-1.0; u2net Apache-2.0; smolvlm Apache-2.0.
- docs/MOCAP_SPIKE.md (https://raw.githubusercontent.com/fernandotonon/QtMeshEditor/master/docs/MOCAP_SPIKE.md) line 29: "all five MediaPipe models Apache-2.0 (code AND weights)". Unsourced assertion; no dataset named.

## Dependencies and assets

| Component | License id | Status | Source URL |
|---|---|---|---|
| ort (pykeio/ort) | MIT OR Apache-2.0 (Cargo.toml) | CLEAR (code only, no weights) | https://raw.githubusercontent.com/pykeio/ort/main/Cargo.toml |
| ONNX Runtime | MIT ("Copyright (c) Microsoft Corporation") | CLEAR (code only, no weights) | https://raw.githubusercontent.com/microsoft/onnxruntime/main/LICENSE |
| bevy | MIT OR Apache-2.0 (Cargo.toml); assets under CC0, CC-BY-4.0, OFL-1.1 (see CREDITS row) | CLEAR (code only, no weights) | https://raw.githubusercontent.com/bevyengine/bevy/main/Cargo.toml |
| bevy_vrm1 0.10.0 | MIT verified (LICENSE-MIT: "Copyright (c) 2025 elm"). Apache-2.0 half UNVERIFIED: LICENSE and LICENSE-APACHE both 404 at main. | CONDITIONAL (Apache half unverified) | https://raw.githubusercontent.com/not-elm/bevy_vrm1/main/LICENSE-MIT |
| nalgebra | Apache-2.0 (Cargo.toml line 13 `license = "Apache-2.0"`) | CLEAR (code only, no weights) | https://raw.githubusercontent.com/dimforge/nalgebra/main/Cargo.toml |
| nokhwa 0.10.11 (pinned) | Apache-2.0 per crates.io API for 0.10.11. Fork note: the `senpai` branch Cargo.toml says `license = "MIT OR Apache-2.0"`; that branch is a fork and is NOT used. | CLEAR (code, pinned release) | https://crates.io/api/v1/crates/nokhwa/0.10.11 |
| windows-rs (windows 0.58.0) | MIT OR Apache-2.0 (crates.io API). Main-branch Cargo.toml fetch returned empty; not cited. | CLEAR (code only, no weights) | https://crates.io/api/v1/crates/windows/0.58.0 |
| Bevy CREDITS assets | CC0 1.0 (Generic RPG Pack, HDRI maps, Kenney kits, freesound CC0 guitar); CC-BY 4.0 (glTF fox by @tomkranis, MorphStressTest, freesound orchestra); SIL OFL 1.1 (FiraMono) | CLEAR per asset, only if shipped | https://raw.githubusercontent.com/bevyengine/bevy/main/CREDITS.md |

Bevy CREDITS note: these asset licenses apply only if the app ships those assets. Check the shipped asset list before relying on this.

## Recorded facts and quoted lines

- senty-au Hub YAML `License: apache-2.0`. Card: "Apache-2.0, the same as the original model. The model and its weights are © Google LLC." (re-fetched; confirmed at line 80 of the raw README) "This repository only changes the file format." Source: https://huggingface.co/senty-au/face_landmarks_detector-ONNX
- Google Face Landmarker page footer: "content of this page is licensed under the Creative Commons Attribution 4.0 License, and code samples are licensed under the Apache 2.0 License." No model-specific license on the page. Source: https://ai.google.dev/edge/mediapipe/solutions/vision/face_landmarker
- mocap/NOTICE.txt: "These ONNX graphs are converted from Google MediaPipe's Face Landmarker and Pose Landmarker models (https://developers.google.com/mediapipe), released under the Apache License, Version 2.0: https://www.apache.org/licenses/LICENSE-2.0". Names no file names. Source: https://huggingface.co/fernandotonon/QtMeshEditor-models/resolve/main/mocap/NOTICE.txt
- mocap/face/ contains face_blendshapes.onnx (1.09 MB), face_detector.onnx (418 kB), face_landmarks.onnx (4.92 MB). No README or LICENSE inside this folder. Root NOTICE.txt: NOT FOUND.
- faceblendshapes TRAINING DATA: NOT FOUND. README has no dataset section; YAML has tags only (onnx, mediapipe, blendshapes, arkit, face-capture, qtmesheditor), no `datasets:` field. Google blendshape model card: NOT FOUND (PDF URL guessed, 404; Blendshape V2 PDF binary, unreadable).
- karie Hub body: line 13 "↓ ResNet-18 (ImageNet pretrained)"; lines 114 to 116 cite "EMOCA (CVPR 2022)", "AUBlendNet (ICCV 2025)", "DECA (CVPR 2021)"; line 120 "MIT License". No dataset named. Card has no YAML license field. Any research-only lineage into the weights is INFERENCE from citations, not stated by the card. Source: https://huggingface.co/karie666666/face-au-arkit-blendshapes/raw/main/README.md
- AUBlendNet LICENSE (code): "MIT License" / "Copyright (c) 2023 DoubleXING". Copyright holder differs from paper authors cited on the karie card (Li et al., ICCV 2025); link unverified. Source: https://raw.githubusercontent.com/wslh852/AUBlendNet/main/LICENSE
- DECA LICENSE: "Software Copyright License for non-commercial scientific research purposes". "Any other use, in particular any use for commercial, pornographic, military, or surveillance purposes is prohibited." "This includes, without limitation, incorporation in a commercial product". Source: https://github.com/YadiraF/DECA/blob/master/LICENSE
- EMOCA LICENSE: same non-commercial research text and same bans. Source: https://github.com/radekd91/emoca/blob/master/LICENSE
- bevy_vrm1 LICENSE-MIT: "MIT License" / "Copyright (c) 2025 elm". LICENSE and LICENSE-APACHE both 404 at main. Source: https://raw.githubusercontent.com/not-elm/bevy_vrm1/main/LICENSE-MIT
- nalgebra LICENSE text starts "Apache License Version 2.0, January 2004". Source: https://raw.githubusercontent.com/dimforge/nalgebra/main/Cargo.toml
- nokhwa: crates.io 0.10.11 `"license":"Apache-2.0"`. Source: https://crates.io/api/v1/crates/nokhwa/0.10.11. Senpai fork `license = "MIT OR Apache-2.0"`. Source: https://raw.githubusercontent.com/l1npengtul/nokhwa/senpai/Cargo.toml
- windows-rs: crates.io windows 0.58.0 `"license":"MIT OR Apache-2.0"`. Source: https://crates.io/api/v1/crates/windows/0.58.0 (raw main Cargo.toml fetch was empty; not cited).
- ONNX Runtime LICENSE: "MIT License", "Copyright (c) Microsoft Corporation". Source: https://raw.githubusercontent.com/microsoft/onnxruntime/main/LICENSE
- bevy: "dual-licensed under either: MIT License ... Apache License, Version 2.0 ... at your option." Source: https://github.com/bevyengine/bevy
- ort Cargo.toml `license = "MIT OR Apache-2.0"`. Source: https://github.com/pykeio/ort
- Apache-2.0 section 4: "(a) You must give any other recipients of the Work or Derivative Works a copy of this License; and" "(c) You must retain ... all copyright, patent, trademark, and attribution notices". "(d) If the Work includes a "NOTICE" text file ... then any Derivative Works that You distribute must include a readable copy of the attribution notices contained within such NOTICE file". Source: https://www.apache.org/licenses/LICENSE-2.0.txt
- Seed-san: see docs/notes/seed-san.md (verified through raw HTML): README "VRM Public License 1.0"; meta `commercialUsage` "corporation"; `creditNotation` "required"; `allowRedistribution` true; `modification` "allowModificationRedistribution". This file is the source for row 5 and the ship item 4.

## What an .exe must ship

Apply each item only where the component is actually bundled. Items for UNRESOLVED components are blocked, not cleared.

1. Apache-2.0 full text and NOTICE copy for mocap/face weights and the senty-au, faceblendshapes, BlazeFace, Face Mesh graphs (CONDITIONAL; blocked on 1b). Source: https://www.apache.org/licenses/LICENSE-2.0.txt (sections 4(a), 4(d)); https://huggingface.co/fernandotonon/QtMeshEditor-models/resolve/main/mocap/NOTICE.txt; THIRD_PARTY_AI_MODELS.md line "Ship the Apache-2.0 notice next to the hosted weights" (third-party assertion).
2. MIT copyright and permission notice for ONNX Runtime, ort (if MIT is chosen), bevy, bevy_vrm1 (LICENSE-MIT). Source: https://raw.githubusercontent.com/microsoft/onnxruntime/main/LICENSE ; https://raw.githubusercontent.com/not-elm/bevy_vrm1/main/LICENSE-MIT.
3. CC-BY 4.0 attribution for any bevy CREDITS CC-BY asset that ships; OFL-1.1 notice for FiraMono if shipped. Source: https://raw.githubusercontent.com/bevyengine/bevy/main/CREDITS.md.
4. VRM avatar credit notation for seed-san (`creditNotation` "required"). Source: docs/notes/seed-san.md.
5. Google "© Google LLC" attribution line from the senty-au card, if senty-au weights ship. Source: https://huggingface.co/senty-au/face_landmarks_detector-ONNX

## NOT FOUND

- Model-specific license text for Google MediaPipe Face Landmarker, BlazeFace, Face Mesh V2, Blendshape V2. Tried: developers.google.com/edge/mediapipe page (no model license; footer gives CC-BY 4.0 for content and Apache 2.0 for code); https://developers.google.com/edge/terms (404); https://ai.google.dev/edge/terms (no license lines matched); storage.googleapis.com Model Card PDF guess (404); Blendshape V2 PDF (binary, unreadable); google/mediapipe repo LICENSE (Apache-2.0, code only, not model terms).
- faceblendshapes training data and dataset license. Tried: Hub README and YAML, docs/MOCAP_SPIKE.md, Google blendshape model card.
- karie666666 YAML license field. Tried: Hub body only.
- AUBlendNet weights license and dataset usage terms (Baidu Pan, no stated terms). LICENSE text (code) is verified; weights are NOT FOUND.
- Root NOTICE.txt in fernandotonon/QtMeshEditor-models. Tried: hf:// root path (not found). Only mocap/NOTICE.txt exists.
- bevy_vrm1 Apache-2.0 half (LICENSE-APACHE 404 at main; LICENSE 404 at main).
- github.com/fernandotonon/QtMeshEditor-models, github.com/google-ai-edge/mediapipe-models: 404.

## What blocks a public release

1. Google model-specific terms (1b) are unconfirmed from a primary source. Every MediaPipe-derived graph (2, 3, 3a, 3b, and 1a) stays CONDITIONAL until this is fixed. The only Apache-2.0 claims for weights are Hub mirrors and an unsourced third-party doc.
2. faceblendshapes (3) training data NOT FOUND. Blocks provenance for those weights.
3. karie666666 fallback (4): no YAML license; card body says MIT for the repo. Its citations (DECA, EMOCA) are non-commercial research licenses. Whether those terms reach the weights is INFERRED. Blocked until cleared. Do not present MIT as clean for the weights.
4. DECA and EMOCA (4a, 4b): non-commercial. Must not ship in any build.
5. AUBlendNet (4c): code MIT verified; weights NOT FOUND; copyright holder differs from cited paper authors. Blocked for weights.
6. bevy_vrm1 (dependency): Apache-2.0 half UNVERIFIED. Blocked until LICENSE-APACHE is found.
7. Mocap file-to-graph mapping (2) is INFERRED; the models repo is `license: other` / per-model, and its README table has no mocap or face row.
8. Seed-san (5): VRM Public License 1.0 with `commercialUsage` "corporation" is conditional. Confirm conditions before shipping (vrm.dev fetch failed; see docs/notes/seed-san.md).

## CIGPose-m (task 11.3)

- File: models/upper_body/cigpose-m_coco_256x192.onnx, from github.com/namas191297/cigpose-onnx (release 2026-03-16), an ONNX export of CIGPose by 53mins. Apache-2.0 per the wheel header of cigpose-onnx 1.0.1 ("inference.py: "Licensed under Apache 2.0"). The weights' training data (COCO) terms were not checked here. SHA256 is in models/SHA256SUMS.


## OpenVINO runtime (task 11.2)

- Files shipped next to live.exe: openvino.dll, openvino_c.dll, openvino_intel_cpu_plugin.dll, openvino_onnx_frontend.dll, tbb12.dll, tbbmalloc.dll (about 66 MB), taken from the OpenVINO 2026.4.1 Python wheel libs folder. OpenVINO is Apache-2.0 (Intel); oneTBB is Apache-2.0. Rust bindings: crate `openvino` 0.11.1 (intel/openvino-rs, Apache-2.0), loaded at run time. The licence texts of the wheel were not re-read in this task; confirm before a public release.
