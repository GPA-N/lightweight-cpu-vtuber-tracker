# Head fit residual gate and blendshape weakness: research notes

Read-only research. No code changed. Citations are URLs (MediaPipe master branch, Hugging Face) or file:line in this repo. Items not verified are marked UNVERIFIED.

## Problem 1: weighted Procrustes fit rejects photos at RESIDUAL_LIMIT = 0.1

### 1a. Does MediaPipe gate on a fit residual?

No. Neither MediaPipe source file has a residual threshold.

- `geometry_pipeline.cc`: https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/modules/face_geometry/libs/geometry_pipeline.cc. The pipeline builds a perspective frustum (`height_at_near = 2*near*tan(0.5*fov)`, `width_at_near = frame_width*height_at_near/frame_height`), applies `MoveAndRescaleZ` (`z = (z - depth_offset + near)/scale`) and `ChangeHandedness` (`z *= -1`), then solves the pose. No residual check.
- `procrustes_solver.cc`: https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/modules/face_geometry/libs/procrustes_solver.cc. Only degeneracy guards (`kAbsoluteErrorEps = 1e-9`) and a determinant correction that flips column 2 of the postrotation. No residual threshold.
- Our own note already records the 0.1 value as a "starting value, tune on the first run": `docs/notes/canonical-face.md:123`. It is not a MediaPipe constant.

Conclusion: the 0.1 gate is our invention, not a MediaPipe rule.

### 1b. Is the residual metric the cause?

Our residual is `(err2/w_sum).sqrt() / (size2/w_sum).sqrt()` (`crates/pose/src/fit.rs:100`). It is divided by the RMS radius of the destination cloud, so it is scale-invariant and blind to any per-axis mismatch in z.

The canonical mesh has a z:x extent ratio of about 2.01 (`docs/notes/canonical-face.md:129`: 4.413 / 8.892). Our fit uses one uniform scale `s` (`fit.rs:93`) with no separate z scale. Any photo where the landmark model's z is compressed relative to x (for example z ratio 1.16) leaves a systematic error in the z component of every point. That error is not absorbed by rotation or uniform scale, so the residual rises even when the head pose is fine.

Frontal fits at 0.08 to 0.09 sit close to the 0.1 limit, so small changes in expression or framing push them over. The gate is therefore reacting to a model-scale mismatch, not to a bad rotation.

Caveat: the measured z ratio of 1.16 is from the earlier run. The source of the compression is UNVERIFIED. Candidates are landmark-model z normalization, perspective in the photo, or our crop scaling (`crates/infer/src/landmarks.rs`, `z = p[2] / INPUT_SIZE * rect.width`).

### 1c. Z units and handling in MediaPipe

- Raw z in `tensors_to_landmarks_calculator.cc` is normalized as `z / input_image_width / normalize_z`, the same unit as x: https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/calculators/tensor/tensors_to_landmarks_calculator.cc (comment: "Scale Z coordinate as X + allow additional uniform normalization").
- MediaPipe's face_geometry does not use a per-axis z scale. It handles depth through the perspective frustum and `MoveAndRescaleZ`, which is why it does not need a residual gate: the perspective model absorbs the depth distortion that our orthographic-like fit cannot.
- Our `landmarks_to_frame` (`crates/infer/src/landmarks.rs:82-90`) scales z by `rect.width / 256`, which is consistent with the x unit. Its correctness against MediaPipe is UNVERIFIED without a numerical comparison.

### 1d. Basis check

- The pbtxt lists 33 `procrustes_landmark_basis` entries with the same weights we use: https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/modules/face_geometry/data/geometry_pipeline_metadata_landmarks.pbtxt. Our 11-point subset (`crates/pose/src/fit.rs:7-19`) is a subset of this set and uses identical weights.
- MediaPipe fits on 33 points. Ours uses 11, so the fit has less averaging and is more sensitive to individual landmark errors (UNVERIFIED impact, not measured).

### 1e. Candidate fixes (ranked)

1. Remove the hard gate, or raise it and log it. Keep the residual as a quality signal. This is the smallest change and removes the rejection of photos that are otherwise usable.
2. Normalize the residual by the eye distance (`|x263 - x33|`) instead of the dst RMS radius. This gives a stable, interpretable unit and avoids the z-compression blind spot in the denominator.
3. Fit the 33-point basis instead of 11, or use robust (trimmed or Huber) weights so that a few bad landmarks do not dominate.
4. Estimate a z scale as a separate parameter, or fit x,y only (weak perspective), and compare the result to the full fit.

Risks: dropping the gate lets bad fits through, which is the reason the gate was added (`fit.rs:95-96`, the mirror case in `canonical-face.md:123`). Keep the sign check at `fit.rs:62-68` (assumption A2) in place either way. The mirror test `x_mirrored_input_is_err` (`fit.rs:200-206`) depends on the sign check, not the residual, so it should still pass.

## Problem 2: blendshape weights look weak (most < 0.3, smile ~0.65)

### 2a. Input convention matches MediaPipe

- Our wrapper takes 478 image-pixel landmarks, selects the 146 subset indices, and passes x,y only (`crates/infer/src/blendshapes.rs:76-87`). Shape `[1,146,2]` (`blendshapes.rs:116`).
- MediaPipe's `face_blendshapes_graph.cc` feeds `LandmarksToTensor` with X and Y attributes, `flatten=false`, `top_k=0`, `min_score_threshold=-1`, and no activation: https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/tasks/cc/vision/face_landmarker/face_blendshapes_graph.cc
- `landmarks_to_tensor_calculator.cc` scales X by image width and Y by image height, with multiplication only and no centering: https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/calculators/tensor/landmarks_to_tensor_calculator.cc
- The Hugging Face model card for the same ONNX export states input `[1,146,2]` in pixel coordinates (`x*imgW`, `y*imgH`) and output 52 values in [0,1] MediaPipe order: https://huggingface.co/fernandotonon/QtMeshEditor-faceblendshapes-onnx
- The subset in `data/landmark_subset_146.json` is identical to MediaPipe's `kLandmarksSubsetIdxs` (`docs/notes/model-blendshapes.md`).

So the coordinate convention is not the likely cause, provided the caller passes full-frame pixels (`blendshapes.rs:3-5`).

### 2b. Clamp hides raw output

`decode_output` clamps every raw value to [0,1] and maps NaN to 0 (`blendshapes.rs:91-100`). If the ONNX graph does not apply a sigmoid, negative logits become 0, and large logits saturate at 1. This would hide the real distribution, but it would not make the values uniformly weak. Whether this ONNX output is already sigmoid-activated is UNVERIFIED. The model card does not say (it states [0,1] output, which may be the export's own activation).

### 2c. Where weakness could come from (UNVERIFIED, not measured)

- Landmark drift or crop error: the blendshape model is sensitive to landmark positions. Our crop path (`crates/facelogic/src/crop.rs`, `rect_from_landmarks` with 1.5x width, `warp_to_tensor` bilinear, /255) differs from MediaPipe's face detector path. A small offset in x,y shifts the mouth and brow geometry.
- Mirroring: a horizontally flipped frame swaps left and right blendshapes. This would not reduce magnitudes, but it would show up as mismatched left/right channels. Check the frame orientation.
- Landmark model variant: our landmark model may differ in its output scale or pixel range from MediaPipe's. Not verified.
- One Euro filter (min_cutoff 1.0, beta 0.007) smooths the output but does not change its scale.

### 2d. Recommended check

Run the blendshape ONNX on one reference frame that MediaPipe scores (for example, through the MediaPipe Tasks Python API, run offline). Compare per-channel values for the same frame. If the ONNX output is sigmoid-shaped, the raw values will be more informative than the clamped ones; log the raw output before `decode_output`. This check needs an offline run, which the no-local-compute rule forbids on this PC, so it should run on Kaggle or Colab.

## Summary

- Problem 1: the 0.1 gate is not a MediaPipe rule. MediaPipe has no residual gate and uses perspective depth handling instead of a per-axis z scale. Our residual is blind to z compression. Fix by removing or relaxing the gate, normalizing by eye distance, and considering a z scale or a 33-point basis.
- Problem 2: input convention matches MediaPipe. Clamping may hide raw output. The cause of weak weights is UNVERIFIED and needs a reference comparison on a cloud run.

## Unverified items

- Measured z ratio 1.16 source (landmark model vs crop vs perspective).
- Whether the blendshape ONNX output is already sigmoid-activated.
- Effect of 11-point vs 33-point basis on fit quality.
- Whether our landmark model output scale matches MediaPipe's.
- ARKit 52-name ordering (not checked against an external source in this run).
