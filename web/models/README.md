# web/models: small models the page loads from this site

| file | what | size | SHA-256 | licence |
|---|---|---|---|---|
| face_detector.onnx | MediaPipe BlazeFace short range (ONNX port) | 418,437 | 02a04d5d37c3558dc4d5274f7f8f0f0f01ac94e46c5ffb2cee82395d47e23181 | Apache-2.0 (repo README models table) |
| face_landmarks.onnx | MediaPipe Face Mesh V2, 478 points | 4,920,995 | 7d6e82dee82a1dca5fbddb282b3cc74571833a530de317fc22ae325c3358beeb | Apache-2.0 |
| face_blendshapes.onnx | MediaPipe blendshape model, 52 outputs | 1,094,706 | 37cf54d3ccf671f8936427437a1a0b627799e16f20439de3cc35aa189ad40c66 | Apache-2.0 |
| palm_detection_mediapipe_2023feb.onnx | MediaPipe palm detector (opencv_zoo port) | 3,905,734 | 78ff51c38496b7fc8b8ebdb6cc8c1abb02fa6c38427c6848254cdaba57fcce7c | Apache-2.0 |
| handpose_estimation_mediapipe_2023feb.onnx | MediaPipe hand landmarks (opencv_zoo port) | 4,099,621 | db0898ae717b76b075d9bf563af315b29562e11f8df5027a1ef07b02bef6d81c | Apache-2.0 |

These are the same files as `models/` of the app (checksums match `models/SHA256SUMS`). They are kept in a separate commit so the owner can drop it; without them the page cannot start.

## Loaded at runtime from Hugging Face (never committed)
- InstantHMR nano network: `https://huggingface.co/momolesang/InstantHMR/resolve/main/instanthmr-nano.onnx` (80,499,078 bytes, SHA-256 a9e78fb89649d59bc8df294c6d23cd460168003b6419386bb053d353c3f85a69), SAM License, non-commercial, by @adjelmohamed.
- MHR body model: `https://huggingface.co/momolesang/InstantHMR/resolve/main/mhr-body.onnx` (12,676,712 bytes, SHA-256 eca2224d724f91b9e4f4bd7c86f6eafc74ab59ff0494bfb462e375cd04dac0fb), SAM License.
- RF-DETR nano person detector: `https://huggingface.co/onnx-community/rfdetr_nano-ONNX/resolve/main/onnx/model.onnx` (108,074,865 bytes, SHA-256 9CBAC6B11CE34A03034E4D5A24CFAC5F18632FD6761D1311DD640232088D7FEE), Apache-2.0. Input `pixel_values`, outputs `pred_boxes` and `logits`; numerically the app's detector (boxes within 9.5e-5, logits 1.3e-4, same winning query on 7 frames). Parity run with this file: `COMMUNITY=<dir> node web/body/test/parity.mjs`.
