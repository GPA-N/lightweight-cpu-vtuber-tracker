# Canonical face model for MediaPipe Face Landmarker (task 1.8)

Fetched 2026-10-17 from the `master` branch of google-ai-edge/mediapipe; no pinned release tag. Downloads went to new temp directories and were only hashed and grepped, never executed. Landmark indices are 0-based array indices unless marked "line" (1-based file line). Code citations give the function name and a verbatim quote; line numbers are shown only as a secondary hint for the fetched master snapshot.

## License

- Repo license: Apache License 2.0. Source: https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/LICENSE (SHA256 `8707eef0533987efc5b155d64761eeb6e20793f50b9bd1a68dad1cf4719d0ed8`, 12,331 bytes). Header matches Apache 2.0 text (`Version 2.0, January 2004`).
- The data files carry no separate license. `geometry_pipeline_metadata_landmarks.pbtxt` has the Apache 2.0 header ("Copyright 2020 The MediaPipe Authors").

## Files and hashes

| File | URL | Bytes | SHA256 | Vertices / faces | Last commit touching file (GitHub API) |
|---|---|---|---|---|---|
| `canonical_face_model.obj` | https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/modules/face_geometry/data/canonical_face_model.obj | 45,999 | `8bac80443397e113f41a8b565ea72c59390bc031d9defab289dba7bc0c54e618` | 468 `v`, 468 `vt`, 898 `f` | a908d668c730da128dfa8d9f6bd25d519d006692 |
| `face_model_with_iris.obj` | https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/modules/face_geometry/data/face_model_with_iris.obj | 46,711 | `cc5659c31aa4fa3412cb33629b0ebd6e16a75e79746daa07d1c44c61780cae5e` | 478 `v`, 478 `vt`, 906 `f` | 4ad67abd7093e064788f8b934a73ed3da67d57f8 |
| `geometry_pipeline_metadata_landmarks.pbtxt` | https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/modules/face_geometry/data/geometry_pipeline_metadata_landmarks.pbtxt | 117,675 | not hashed | 468-vertex `canonical_mesh`, 33 `procrustes_landmark_basis` entries | not checked |

Commit SHAs come from `https://api.github.com/repos/google-ai-edge/mediapipe/commits?path=<file>&per_page=1`.

Code fetched for the chain in this note (round 3, all from `master`):

- `mediapipe/modules/face_geometry/libs/geometry_pipeline.cc` (`ScreenToMetricSpaceConverter::Convert`, `ProjectXY`, `EstimateScale`, `MoveAndRescaleZ`, `UnprojectXY`, `ChangeHandedness`, `PerspectiveCameraFrustum`)
- `mediapipe/modules/face_geometry/protos/environment.proto` (`OriginPointLocation`, `PerspectiveCamera`, `Environment`)
- `mediapipe/tasks/cc/vision/face_geometry/face_geometry_from_landmarks_graph.cc` (default environment)
- `mediapipe/tasks/cc/vision/face_landmarker/face_landmarker_graph.cc` (`image_size` wiring)
- `mediapipe/calculators/tensor/tensors_to_landmarks_calculator.cc` and `.proto` (z normalization)
- `mediapipe/tasks/cc/vision/face_landmarker/tensors_to_face_landmarks_graph.cc` and `face_landmarks_detector_graph.cc` (input width)

Directory listing (https://github.com/google-ai-edge/mediapipe/tree/master/mediapipe/modules/face_geometry/data) also contains `canonical_face_model.fbx`, `canonical_face_model_uv_visualization.png`, `BUILD`, and `geometry_pipeline_metadata_including_iris_landmarks.pbtxt`. Not downloaded.

## 468 vs 478 points

- `canonical_face_model.obj` has 468 vertices (468 `v`, 468 `vt`).
- `face_model_with_iris.obj` has 478 vertices. Its first 468 `v` lines match `canonical_face_model.obj` (checked in round 1; not re-checked this round). Lines 469-478 (1-based) are indices 468-477.
- Iris layout (0-based), from coordinates in `face_model_with_iris.obj`:
  - 468 is the center of the ring 469-472 (center derived from coordinates; no source names it).
  - 473 is the center of the ring 474-477.
  - Ring 469-472 is at x < 0; ring 474-477 is at x > 0.
  - Connection sets, from an earlier fetch of https://github.com/google-ai-edge/mediapipe/blob/master/mediapipe/python/solutions/face_mesh_connections.py (not re-fetched): `FACEMESH_RIGHT_IRIS = [(469,470),(470,471),(471,472),(472,469)]`, `FACEMESH_LEFT_IRIS = [(474,475),(475,476),(476,477),(477,474)]`.
- The 478 count is stated in https://developers.google.com/edge/mediapipe/solutions/vision/face_landmarker ("The model outputs an estimate of 478 3-dimensional face landmarks"). The `ai.google.dev` URL redirects there.
- The pbtxt `canonical_mesh` matches OBJ vertices 1-468 on the positions (round 1).
- face_geometry.proto: "The face mesh vertex IDs are the same as the face landmark IDs." https://github.com/google-ai-edge/mediapipe/blob/master/mediapipe/modules/face_geometry/protos/face_geometry.proto

## Coordinate system and units

- Units: centimeters. Source: https://developers.googleblog.com/en/mediapipe-3d-face-transform/ ("A metric unit used by the default canonical face model is a centimeter."). The OBJ has no unit header.
- Handedness: right-handed metric space. Source: the same blog ("right-handed orthonormal system") and face_geometry.proto ("right-handed Metric 3D space").
- Camera: the virtual camera points "in the negative direction of the Z-axis" (same blog).
- Axis directions, from OBJ values (derived, not stated by a source):
  - index 0 (line 1): (0.000, -3.406, 5.980)
  - index 4 (nose tip): (0.000, -0.463, 7.587)
  - index 10 (forehead): y = 8.262
  - index 152 (chin): y = -9.403
  - index 33 / 263 (outer eye corners): x = -4.446 / +4.446, y = 2.664, z = 3.173
  - index 133 / 362: x = -1.856 / +1.856, y = 2.585, z = 3.758
  - index 129 / 358: x = -1.786 / +1.786, y = -0.978, z = 4.850
  - So the canonical OBJ is y-up and z-out (the nose tip has the largest z, and the face looks toward +z). x is symmetric. Which side is "+x" is not stated in any source.

### Frustum inputs (MediaPipe pipeline)

- Default environment, `face_geometry_from_landmarks_graph.cc`, used when no ENVIRONMENT side packet is given (`face_landmarker_graph.cc` passes ENVIRONMENT only if the caller provides one):
  - `options.mutable_environment()->set_origin_point_location(` / `proto::OriginPointLocation::TOP_LEFT_CORNER);`
  - `perspective_camera.set_vertical_fov_degrees(63.0 /*degrees*/);`
  - `perspective_camera.set_near_plane(1.0 /* 1cm */);`
  - `perspective_camera.set_far_plane(10000.0 /* 100m */);`
- Frame size: `face_landmarker_graph.cc`, `image_size >> face_geometry_from_landmarks.In(kImageSizeTag);`. This is the full input image, not the face crop. The crop aspect ratio is therefore not an input to the frustum.
- Frustum formula, `PerspectiveCameraFrustum` in geometry_pipeline.cc: `const float height_at_near = 2.f * perspective_camera.near() * std::tan(0.5f * kDegreesToRadians * perspective_camera.vertical_fov_degrees());` and `const float width_at_near = frame_width * height_at_near / frame_height;`.
- Values we would choose: none for our spike, because the weak-perspective recipe below has no frustum. If we reproduce the MediaPipe chain, use the defaults above (63 degrees, near 1.0 cm, far 10000 cm) and the full source frame size (frame width/height of our camera; not known yet).

### Z units (model output to crop pixels)

- `tensors_to_landmarks_calculator.cc`, `TensorsToLandmarksCalculator::Process` (normalized output): `norm_landmark->set_z(landmark.z() / options_.input_image_width() / options_.normalize_z());` with the comment `// Scale Z coordinate as X + allow additional uniform normalization.`
- `tensors_to_landmarks_calculator.proto`: `optional float normalize_z = 5 [default = 1.0];`
- Input width is set in `tensors_to_face_landmarks_graph.cc` / `face_landmarks_detector_graph.cc`: `options->set_input_image_width(input_image_tensor_spec.image_width);`. No `normalize_z` assignment was found in these two files, so the default 1.0 applies in the Tasks graph (checked by grep on the two fetched files only).
- Assumption A1 (not a result): model z uses the same units as x (crop pixels). The calculator code is consistent with this, since x, y, and z are all divided by the same input width. The docs only say "3-dimensional face landmarks". Task 4.7 checks A1 (see Unknowns below).

### Crop origin and the y flip

- The tensor-to-landmark stage emits x, y in input-image pixels and normalizes them by image width/height (`norm_landmark->set_y(landmark.y() / options_.input_image_height());`). The image origin convention (top-left) is the usual one but is not stated in a fetched source: NOT FOUND as an explicit statement.
- In the fetched files, the only y flip is in `ProjectXY` (geometry_pipeline.cc), guarded by `if (origin_point_location_ == OriginPointLocation::TOP_LEFT_CORNER) { landmarks.row(1) = 1.f - landmarks.row(1).array(); }`. The default is TOP_LEFT_CORNER (see above), so the pipeline expects top-left-origin, image-normalized y and flips it once, before unprojection.

## MediaPipe reference chain (faithful, for comparison)

Source: `ScreenToMetricSpaceConverter::Convert` in `mediapipe/modules/face_geometry/libs/geometry_pipeline.cc` (https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/modules/face_geometry/libs/geometry_pipeline.cc). Input is the Face Landmarker `NORM_LANDMARKS`. The landmarks input source is FACE_LANDMARK_PIPELINE by default (`metadata.input_source() == InputSource::DEFAULT ? InputSource::FACE_LANDMARK_PIPELINE`), so the FACE_DETECTION branches in Convert are skipped.

Steps, in the order Convert runs them:

1. ProjectXY (screen to frustum-scaled space). Quote: `landmarks = landmarks.array().colwise() * Eigen::Array3f(x_scale, y_scale, x_scale);`, after the TOP_LEFT_CORNER flip `landmarks.row(1) = 1.f - landmarks.row(1).array();`. z is scaled by `x_scale` (width at near), not by y. Then x, y get `left`, `bottom` added.
2. Depth offset. Quote: `const float depth_offset = screen_landmarks.row(2).mean();`
3. First scale estimate, on a flipped copy. Quote: `Eigen::Matrix3Xf intermediate_landmarks(screen_landmarks);` then `ChangeHandedness(intermediate_landmarks);` (ChangeHandedness call 1, on the copy only), then `ABSL_ASSIGN_OR_RETURN(const float first_iteration_scale, EstimateScale(intermediate_landmarks), ...)`.
4. Second pass on an unflipped copy. Quote: `MoveAndRescaleZ(pcf, depth_offset, first_iteration_scale, intermediate_landmarks);` then `UnprojectXY(pcf, intermediate_landmarks);` then `ChangeHandedness(intermediate_landmarks);` (ChangeHandedness call 2, copy only). MoveAndRescaleZ quote: `landmarks.row(2) = (landmarks.array().row(2) - depth_offset + pcf.near) / scale;`. UnprojectXY quote: `landmarks.row(0) = landmarks.row(0).cwiseProduct(landmarks.row(2)) / pcf.near;` (same for row 1).
5. Second scale estimate (no FACE_DETECTION branch). Quote: `ABSL_ASSIGN_OR_RETURN(const float second_iteration_scale, EstimateScale(intermediate_landmarks), ...)`.
6. Total scale. Quote: `const float total_scale = first_iteration_scale * second_iteration_scale;`
7. Final metric landmarks on the ORIGINAL screen copy: `MoveAndRescaleZ(pcf, depth_offset, total_scale, screen_landmarks);` (z is rescaled before any flip, so z is not negated first), then `UnprojectXY(pcf, screen_landmarks);`, then `ChangeHandedness(screen_landmarks);` (ChangeHandedness call 3, the only one that reaches the output). Quote for the flip: `landmarks.row(2) *= -1.f;`
8. Pose transform. Quote: `SolveWeightedOrthogonalProblem(canonical_metric_landmarks_, metric_landmarks, landmark_weights_, pose_transform_mat)`. The 33-point weighted similarity fit (Procrustes, uniform scale + rotation + translation; procrustes_solver.h: "Uniform scale, Rotation, Translation").
9. Inverse-pose step. Quote: `metric_landmarks = (pose_transform_mat.inverse() * metric_landmarks.colwise().homogeneous()).topRows(3);` This maps runtime metric landmarks into the canonical frame and is the last transform before output.

Scale estimate. Quote from EstimateScale: `return transform_mat.col(0).norm();`. `transform_mat` is the 4x4 pose from the Procrustes solve on the given landmarks, so the scale is the norm of the first column of the rotation-and-scale block.

Result: two scales (first from the flipped intermediate copy, second after the first pose step); total = product. Converted landmarks are metric (cm), right-handed; `pose_transform_mat` is the pose.

Reviewer note on ChangeHandedness: Convert calls ChangeHandedness three times (quoted above), not once. Two calls are on throwaway copies used for scale estimation; only the third touches the output. The order is MoveAndRescaleZ, then UnprojectXY, then the single flip on the output, as the reviewer said.

## Simplified recipe for our spike (weak perspective)

Approximations: orthographic projection after a uniform scale; no frustum, near plane or depth_offset. ASSUMPTION A1: model z uses the same units as x (crop pixels). ASSUMPTION A2: raw z grows away from the camera (nose tip z is smaller than eye-corner z). Task 4.7 checks both.

Frames:
- Input (image): x right, y down, origin at top-left of the 256x256 crop, z per A2. The frame is not mirrored.
- Output head frame H: right-handed. +x is the subject's left (the viewer's right in an unmirrored image), +y is up, +z points out of the face toward the viewer. ASSUMPTION A3: this is the canonical mesh frame (the direction of +x was derived from the OBJ coordinates, no source states it), so the rest pose is R = I.
- VRM 1.0: the avatar faces +z, +x is the avatar's left, +y is up. Apply R directly as the head bone's local rotation in normalized-bone space. VRM 0.x faces -z: use C R C with C = diag(-1, 1, -1).

Steps:
1. Change frame: p = F q with F = diag(1, -1, -1), det +1. Since F^-1 = F, the frame H is F applied to the input frame.
2. Check A2 on every frame: z of landmark 4 must be greater than the mean z of landmarks 33 and 263 (after step 1). If it is not, log Z_SIGN_ASSUMPTION_FAILED and skip the frame. Never substitute another matrix. (If z has the wrong sign the input is a mirror image and no rotation with det +1 can fix it.)
3. Weighted centroids c = sum(w_i p_i) / sum(w_i), for the canonical subset (c_src) and the converted points (c_dst). Subtract them.
4. H = sum(w_i src_i dst_i^T). SVD: H = U S V^T. d = sign(det(V U^T)), D = diag(1, 1, d), R = V D U^T, s = trace(S D) / sum(w_i |src_i|^2), t = c_dst - s R c_src.
5. R maps canonical points to observed points in frame H. Because F is applied to the data before the fit, R_out = R with no further conversion. Do not compute F R F: that gives the rotation in the image frame, with yaw and pitch negated.
6. Mirror setting: always fit on the unmirrored crop. If the user enables "avatar mirrors me", use R_out = M R M with M = diag(-1, 1, 1), which negates yaw and roll. Never flip pixels or x before the fit.
7. Decompose as R = R_y(yaw) R_x(pitch) R_z(roll): yaw = atan2(R02, R22), pitch = asin(-R12), roll = atan2(R10, R11).

Rust test (task 3.11): read the 11 subset points from the OBJ. For each R_true in {R_y(+20), R_x(+20), R_z(+20)}: Q = 0.5 R_true P; convert to image coordinates x_img = Qx + 128, y_img = -Qy + 128, z_img = -Qz; run the full pipeline, steps 1-7, including the F step and the step 2 sign check. Expect only that axis at +20 within 1 degree, the other two angles within 1 degree of 0, and s = 0.5 within 1e-3 relative. Add a negative test: the same data without F must fail (by the formulas above, F R_y(20) reads as yaw 160, pitch 0, roll 180; the step 2 sign check also rejects the frame first, so test the fit function directly with the check bypassed). Fit-residual safeguard: after step 4, compute the RMS of w_i |dst_i - (s R src_i + t)| over the subset, divided by the dst point-cloud size; if it exceeds 0.1 (starting value, tune on the first run), skip the frame and log FIT_RESIDUAL_HIGH, because an x-mirrored input passes step 2 silently and returns a wrong rotation.

Local subset (ours, from the 33 basis entries; MediaPipe does not use this subset) with pbtxt weights: 129 (0.120625), 358 (0.120625), 4 (0.070910), 136 (0.066890), 365 (0.066890), 33 (0.058724), 263 (0.058724), 133 (0.053286), 362 (0.053286), 205 (0.047252), 425 (0.047252). Selection: highest pbtxt weights and left/right symmetry. Stability under expression is not measured.

## Unknowns for the first run (task 4.7), user holds a still frontal face

1. z sign (A2): print raw z for landmarks 4, 33, 263 and z4 - mean(z33, z263). Expect negative in raw image coordinates.
2. z scale (A1): print raw (x263 - x33) and (z4 - z33) and their ratio. The canonical mesh gives 8.892 cm and 4.413 cm (ratio about 2.01; 4.413 = 7.586580 - 3.173422 from the OBJ). A ratio far from 2 means z needs an extra factor.
3. x handedness: print x of landmarks 33 and 263. Expect x33 < x263.
4. Axis signs: user turns head to their own left, looks up, tilts left ear down; print yaw, pitch, roll and s. Expect yaw > 0, pitch < 0, roll < 0 (EXPECTED, to verify).
5. VRM axes: with identity applied, the avatar faces the camera and +yaw turns its head toward its own left.

## Stable landmark indices for a rigid rotation fit

MediaPipe's pose basis is the 33 `procrustes_landmark_basis` entries in https://raw.githubusercontent.com/google-ai-edge/mediapipe/master/mediapipe/modules/face_geometry/data/geometry_pipeline_metadata_landmarks.pbtxt. These are the points MediaPipe uses for the weighted Procrustes solve in Convert (step 8 above). The full set, with weights (sum 1.055):

4 0.070910; 6 0.032100; 10 0.008447; 33 0.058724; 54 0.007667; 67 0.009078; 117 0.009792; 119 0.014565; 121 0.018591; 127 0.005198; 129 0.120625; 132 0.005560; 133 0.053286; 136 0.066890; 143 0.014817; 147 0.014263; 198 0.025462; 205 0.047252; 263 0.058724; 284 0.007667; 297 0.009078; 346 0.009792; 348 0.014565; 350 0.018591; 356 0.005198; 358 0.120625; 361 0.005560; 362 0.053286; 365 0.066890; 372 0.014817; 376 0.014263; 420 0.025462; 425 0.047252

No index >= 468 is in the basis, so the iris points are not in the pose fit.

## NOT FOUND

- Explicit statement of the image origin convention of the raw tensor x, y (top-left assumed; the pipeline's TOP_LEFT_CORNER handling is the only evidence).
- Names for iris centers 468 and 473: no source names them; ring-center conclusion is from coordinates only.
- Anatomical names for 129, 358, 136, 365, 205, 425: not verified against a source.
- Which side of the face is +x: no source found.
- `metric_scale` default 0.01: a search snippet claimed it, but `geometry_pipeline_calculator.proto` contains no such option. Not verified.
- Output units of `facial_transformation_matrix` and of the Face Landmarker `z` output as stated on the docs page: not stated there (units derived above from calculator code).
- `canonical_face_model.fbx` and the UV PNG: not downloaded or checked.
- Connection sets (`face_mesh_connections.py`): from an earlier fetch, not re-verified this round.
- Our own model's axis convention and whether its z-scale matches x, y: unknown; must be measured.
- `perspective_camera.proto` (path guessed) returned 404; the frustum values come from `environment.proto` and the default-environment code instead.
