# Face landmarks model: senty-au/face_landmarks_detector-ONNX

Scope: task 1.4 of `cpu-face-tracking-spike`. Facts come from files read in this task. Every claim has a source. Items with no source are marked NOT FOUND. Items marked ASSUMPTION are decisions for later tasks, not facts.

## Identity

| Item | Value | Source |
|---|---|---|
| Hub repo | `senty-au/face_landmarks_detector-ONNX` | task brief |
| Hub commit (pinned) | `337d58218b5b1cc597ca3c67360880b920f6ce7b` | HF API `sha` field, as recorded in the task |
| File URL at pinned commit | https://huggingface.co/senty-au/face_landmarks_detector-ONNX/resolve/337d58218b5b1cc597ca3c67360880b920f6ce7b/onnx/model.onnx | constructed from the pinned commit and path below |
| File path in repo | `onnx/model.onnx` (not at repo root) | HF API `tree/337d58.../onnx` returns `"path":"onnx/model.onnx"`; root tree lists only `onnx/`, `.gitattributes`, `README.md` |
| Size | 4920995 bytes | `ls -la` on the downloaded file; HF tree `size` and `lfs.size` match |
| SHA256 | `7d6e82dee82a1dca5fbddb282b3cc74571833a530de317fc22ae325c3358beeb` | `hashlib.sha256` of the downloaded file; matches HF `lfs.oid` |
| ONNX library | onnx version of the inspection environment not re-run; graph facts are version-independent | n/a |
| ONNX model | IR 8, opset 17, producer tf2onnx 1.17.0 | ONNX model proto, read with the `onnx` package |
| Graph | 223 nodes, 257 initializers | same |

The model file was downloaded into an empty temp directory and never executed. Inspection used the `onnx` package only.

## Graph I/O (from the ONNX graph)

| Tensor | Role | dtype | Shape | Source |
|---|---|---|---|---|
| `input_12` | input | FLOAT | `[N,256,256,3]` | ONNX graph |
| `Identity` | output | FLOAT | `[N,1,1,1434]` | ONNX graph (1434 = 478 × 3) |
| `Identity_1` | output | FLOAT | `[N,1,1,1]` | ONNX graph |
| `Identity_2` | output | FLOAT | `[N,1]` | ONNX graph |

Observed graph structure:
- The first op is a Transpose (NHWC to NCHW).
- No normalization op (Sub, Mul, Div) appears before the first Transpose. Normalization in the graph: NOT FOUND.
- Op counts: Conv 106, PRelu 69, Add 34, MaxPool 6, Pad 3, Reshape 3, Transpose 1, Sigmoid 1 (total 223).
- The single Sigmoid node takes `model_5/conv2d_152/BiasAdd` as input. Its output feeds a Reshape that produces `Identity_2`. An ancestor walk shows the Sigmoid is an ancestor of `Identity_2` and is NOT an ancestor of `Identity_1`.
- `Identity_1` is produced by a Reshape whose data input is `Identity_1_raw_output___5:0`. That tensor is not downstream of the Sigmoid.
- Consequence: `Identity_2` is a sigmoid of a tensor that `Identity_1` does not depend on. The graph does not show `Identity_2 = sigmoid(Identity_1)`. Whether the two carry the same presence signal: NOT FOUND (checking would need inference, which the brief forbids).

## Model README claims (`README.md`, sha256 `1dda73b88fc65eb1500117c640dd4b6bd48af0222488496735fc0c7ea019083f`)

- Input: `float32 [1, 256, 256, 3]`, NHWC, RGB, values in [0, 1]. (README "Usage".)
- `Identity` is 478 × (x, y, z) in crop pixels (0..256). (README.) The "crop pixels, 0..256" unit is not verified from the graph.
- `Identity_1` is the presence logit; apply sigmoid. (README lines 32-33.)
- `Identity_2` is "an auxiliary output of the original graph". (README.)
- Mesh indices: 33/263 eye corners, 468-472 and 473-477 irises, 13/14 inner lips. (README comment.)
- Conversion: from `face_landmarks_detector.tflite` in the Face Landmarker `.task` bundle, bundle sha256 `64184e229b263107bc2b804c6625db1341ff2bb731874b0bcc2fe6544e0bc9ff`, TFLite sha256 `c7d54204ce0448474c7f3fa9af494787c0965cbdd6f20fc72867e43046bd43d5`. (README "Conversion".) Not verified here: neither file was downloaded.
- Conversion command: `tf2onnx==1.17.0`, `--opset 17`. (README.)
- License: Apache-2.0. (README.)

## Discrepancies and open points with the graph

- README calls `Identity_2` auxiliary. The graph shows it is a sigmoid output. The README calls `Identity_1` the presence logit, which is consistent with `Identity_1` not being behind the Sigmoid. The relationship between the two presence-like outputs: NOT FOUND.
- README says input is in [0, 1]. The graph has no normalization op, so scaling must be done by the caller. The README usage code does `/ 255.0`, which agrees. TFLite mean/std metadata: NOT FOUND (bundle not read).

## Crop geometry in MediaPipe (commit `800a56cc5004f6b0b0bb65134cc6c2ec12f0c88f`)

Fetched from `google-ai-edge/mediapipe` at that commit with curl, or read from the local clone.

### Input preprocessing (ImageToTensor)

- `mediapipe/tasks/cc/vision/face_landmarker/face_landmarks_detector_graph.cc`: `kNormRectTag[] = "NORM_RECT"` at line 67; `SanityCheckOptions(` at line 107; the node `AddNode("mediapipe.tasks.components.processors.ImagePreprocessingGraph")` at lines 264-273 (NORM_RECT in; IMAGE_SIZE, LETTERBOX_PADDING and TENSORS out).
- `mediapipe/tasks/cc/components/processors/image_preprocessing_graph.cc` lines 75-110 (paraphrase, not a quote):
  - Output tensor width and height from `image_tensor_specs`.
  - uint8 model: range 0 to 255. Float model: range `(0 - mean)/std` to `(255 - mean)/std`.
- Pixel value for this model: `/255` (README), range [0, 1]. ASSUMPTION: matches the model metadata, which was not read.

CPU crop path (`mediapipe/calculators/tensor/image_to_tensor_converter_opencv.cc` about lines 130-156; `image_to_tensor_converter_opencv.h` line 31 sets `flags = cv::INTER_LINEAR`; `flags_` passed in at `image_to_tensor_calculator.cc` line 43):
- Interpolation actually used by the graph: NOT FOUND (the flag is passed in, and no override was seen). ASSUMPTION: bilinear.
- Border mode: `mediapipe/calculators/tensor/image_to_tensor_calculator.proto` lines 84-85 say BORDER_REPLICATE is used by default. `image_to_tensor_utils.cc` lines 244-253 map UNSPECIFIED to replicate. No override seen in the fetched graph.
- Keep aspect ratio: `mediapipe/calculators/tensor/image_to_tensor_calculator.proto` lines 62-65: `optional bool keep_aspect_ratio = 3;` with no explicit default, so the value is unset. `image_preprocessing_graph.cc` line 183 is a comment only, and no assignment exists in the fetched graph. Effective value: false (unset field). ASSUMPTION: proto unset means false, as in the usual proto convention for bool.
- With `keep_aspect_ratio` false, `PadRoi` (`image_to_tensor_utils.cc` lines 56-90) adds zero padding, so no letterbox is added. Letterbox removal on landmarks (`landmark_letterbox_removal_calculator.cc` lines 96-126: `(x - left)/(1 - left_and_right)`, `(y - top)/(1 - top_and_bottom)`) is the identity, since left, right, top and bottom are 0.

### Crop algorithm, case (a): first frame, face detector rect

`mediapipe/tasks/cc/vision/face_detector/face_detector_graph.cc` (`ConfigureDetectionsToRectsCalculator` and `ConfigureRectTransformationCalculator`): keypoints 0 and 1, target angle 0, scale 1.5 on each axis, NO `square_long`, shift 0. Keypoint 0 is the observer's left eye, keypoint 1 the observer's right eye (`face_detection.py` lines 46-49 define RIGHT_EYE=0, LEFT_EYE=1 for the subject; these are the same two points described from the other side).

Inputs: relative box (xmin, ymin, bw, bh), keypoints k0 and k1 in relative coordinates, frame size W×H in pixels.

1. Center and size: cx = xmin + bw/2, cy = ymin + bh/2, w = bw, h = bh (RectFromBox; normalized).
2. Pixel keypoints: x0 = k0.x·W, y0 = k0.y·H, x1 = k1.x·W, y1 = k1.y·H.
3. Rotation: theta = 0 − atan2(−(y1 − y0), x1 − x0), wrapped to [−π, π) (`ComputeRotation`: `NormalizeRadians(target_angle − atan2(−(y1−y0), x1−x0))`, `rect_transformation_calculator.cc` line 324). Radians, positive clockwise on screen (y points down).
4. Scale: w ← 1.5·w, h ← 1.5·h (still normalized). No squaring: the detector path is not squared.
5. No second transformation: in `face_landmarker_graph.cc` the expanded face rects go straight to the landmark detector.

### Crop algorithm, case (b): later frames, previous landmarks

`face_landmarks_detector_graph.cc` lines 340-375 (`ConvertLandmarksToDetection` → `DetectionsToRects` → `RectTransformation`): `ConvertLandmarksToDetection` bounds all 478 projected normalized landmarks: box = (xmin, ymin, xmax − xmin, ymax − ymin).

1. Center and size as in case (a) step 1, from the landmark box.
2. Rotation as in case (a) steps 2-3, but start = landmark 33, end = landmark 263, target 0.
3. Transform: scale_x = scale_y = 1.5, `square_long` = true, shift 0.
   - Square first: L = max(w·W, h·H); w = L/W; h = L/H.
   - Then scale: w ← 1.5·w, h ← 1.5·h.
   - Shift: the proto shift fields (`shift_x`, `shift_y`) have no default, and neither face graph sets them, so they are 0. General nonzero form (`rect_transformation_calculator.cc` `TransformNormalizedRect` lines 216-230): x_shift = (W·w·sx·cos t − H·h·sy·sin t)/W; y_shift = (W·w·sx·sin t + H·h·sy·cos t)/H.
4. Use this rect for the next frame.

### Warp to the 256×256 crop (GetRoi + OpenCV converter)

Pixel rect: Cx = cx·W, Cy = cy·H, Pw = w·W, Ph = h·H, angle theta.

- `cv::boxPoints` order bottomLeft, topLeft, topRight, bottomRight maps to dst (0,256), (0,0), (256,0), (256,256). Source: `dst_corners {0,dst_height, 0,0, dst_width,0, dst_width,dst_height}` in `image_to_tensor_converter_opencv.cc` about line 143.
- The source uses `getPerspectiveTransform` + `warpPerspective`, which inverse-maps each destination pixel.
- Equivalent sampling, for each crop pixel (u, v) in 0..256: sample the frame at X = Cx + c·lx − s·ly, Y = Cy + s·lx + c·ly, with lx = (u/256 − 0.5)·Pw, ly = (v/256 − 0.5)·Ph, c = cos(theta), s = sin(theta).
- Border: replicate. Interpolation: NOT FOUND in the graph (see above); ASSUMPTION bilinear.
- Output: NHWC `[1,256,256,3]`, RGB (COLOR_RGBA2RGB drops alpha), value = pixel/255.

### Inverse mapping: model output to frame pixels

- A model landmark (u, v, zr) in crop pixels 0..256 maps to frame pixels X, Y by the warp formula above. `LandmarkProjection` receives IMAGE_DIMENSIONS, so the matrix branch is used (`landmark_projection_calculator.cc` lines 38-45 for ProjectXY, 111-127 for the matrix from `GetRotatedSubRectToRectTransformMatrix`).
- z_frame_px = (zr/256)·Pw. Z scaling is also in `CalculateZScale` (`landmark_projection_calculator.cc` lines 53-59, 121-122).
- Frame to crop, for tests: dx = X − Cx, dy = Y − Cy, u = ((c·dx + s·dy)/Pw + 0.5)·256, v = ((−s·dx + c·dy)/Ph + 0.5)·256.
- ASSUMPTION for task 4.7: the ONNX `Identity` output is in crop-pixel coordinates (0..256) in the frame described above. Not verified by inference.

Worked example (checked arithmetically, no inference): W=640, H=480, box (0.40, 0.30, 0.20, 0.25), k0=(0.45, 0.40), k1=(0.55, 0.42): theta = 0.148890 rad, rect Cx=320, Cy=204, Pw=192, Ph=180. Point (300, 200) maps to crop (100.837, 126.594) and back to (300.0, 200.0), error 0.0 px. Crop (0,0) lands on the boxPoints topLeft (238.413, 100.755).

### Z scaling

`mediapipe/calculators/tensor/tensors_to_landmarks_calculator.cc` lines 188-192:

```
norm_landmark->set_x(landmark.x() / options_.input_image_width());
norm_landmark->set_y(landmark.y() / options_.input_image_height());
norm_landmark->set_z(landmark.z() / options_.input_image_width() / options_.normalize_z());
```

- `normalize_z` default is 1.0 (`tensors_to_landmarks_calculator.proto` line 58).
- The Tasks landmarks graph sets `input_image_width` and `input_image_height` from the tensor spec (`face_landmarks_detector_graph.cc` lines 134-135). It does not set `normalize_z` (no match in that file).
- So MediaPipe divides raw z by the input width (256) and applies no other factor. Whether this ONNX's raw z is in crop pixels: NOT FOUND (the README says crop pixels, unverified).

### Presence

`mediapipe/tasks/cc/vision/face_landmarker/face_landmarks_detector_graph.cc`:
- Line 90: `kFaceLandmarksOutputTensorsNum = 2`.
- Lines 123-128: `SplitVectorCalculator` with ranges `[0, 1)` (landmarks) and `[1, 2)` (presence).
- Lines 312-322: presence is `TensorsToFloatsCalculator` with `SIGMOID`, then `ThresholdingCalculator` with threshold = `min_detection_confidence` (set from `min_face_presence_confidence` in `face_landmarker.cc` line 123).
- Default value of `min_face_presence_confidence`: NOT FOUND in the fetched files.

Mapping to this ONNX:
- MediaPipe's graph expects 2 output tensors. This ONNX has 3 outputs (`Identity`, `Identity_1`, `Identity_2`). Which ONNX output MediaPipe's presence tensor corresponds to: NOT FOUND.
- ASSUMPTION for task 4.7: presence = sigmoid output `Identity_2` (the only output behind the Sigmoid node), threshold 0.5 (README claim), i.e. a face is present when `Identity_2 >= 0.5`.

### Legacy module (`modules/face_landmark`)

Not re-fetched in this revision. Earlier notes about the legacy `modules/face_landmark/*.pbtxt` files are UNVERIFIED and are not used for any decision here. The legacy 192×192 model is a different model from this 256×256 ONNX.

### README threshold claim

- README line 33: "MediaPipe keeps the face above about 0.5". The Tasks threshold is `min_face_presence_confidence` (see above). Its default is NOT FOUND in fetched sources. The 0.5 claim is unchecked against MediaPipe.

## README accuracy claim: "0.00024 px"

- Source: README lines 68-71: "checked against the TFLite model run through TensorFlow Lite on three photographs. The largest absolute difference is 0.00024 px on the landmarks and 2.2e-5 on the presence logit."
- What it measures: ONNX output vs TFLite output (conversion fidelity), not landmark error against ground truth.
- Inputs: "three photographs", not named, not included. Photo sizes, crops, preprocessing: NOT FOUND.
- Coordinate space of the 0.00024 px (crop pixels or image pixels): NOT FOUND.
- Runtimes for the check (TensorFlow version, TF Lite runtime, onnxruntime version): NOT FOUND. The README's speed numbers are timing only.
- Not reproduced here: no inference run (per the brief).
- Assessment: a conversion-parity claim on an unnamed set. It cannot be checked from the repo or the README alone.

## Unknowns for the first run (task 4.7)

Run on a centred face image and a non-face image. Print all outputs. No claim here is final until these checks pass.

1. Crop coordinate range: print min and max of `Identity` x and y. Expect 0..256 for a centred face.
2. Presence: print `Identity_1`, `Identity_2` and sigmoid(`Identity_1`). Check whether `Identity_2` is about sigmoid(`Identity_1`). Presence decision: the ASSUMPTION above.
3. Input range: run frame/255 and frame/127.5 − 1. Compare the landmark 33 to 263 distance against the frame. Keep the stable one.
4. Rotation sign: on a tilted head, print theta and the frame positions of landmarks 33 and 263. After the warp they should be horizontal in the crop (|v33 − v263| < 3 px).
5. Z scale: print the z range of landmarks 1 and 234 in frame pixels. Expect the order of the face width.

## NOT FOUND (summary)

1. Normalization in the ONNX graph (none before the first Transpose); TFLite mean/std metadata.
2. MediaPipe-side source of the 256 size (metadata not read).
3. Mapping of MediaPipe's presence tensor to this ONNX's `Identity_1` or `Identity_2`; whether `Identity_2 = sigmoid(Identity_1)`.
4. Raw z units in the ONNX output.
5. Default `min_face_presence_confidence`; therefore whether the README's "about 0.5" matches MediaPipe.
6. Identity of the three photographs, their sizes, and the coordinate space of the 0.00024 px figure.
7. TensorFlow and TF Lite versions used for the README's parity check.
8. Bundle and TFLite hashes: taken from the README; not independently verified.
9. Interpolation flag actually passed to the converter in the graph (bilinear is an ASSUMPTION).
10. Model-metadata mean/std and range for this model (value range [0, 1] is from README only).
11. Exact symbol-by-symbol match of the affine inverse to `GetRotatedSubRectToRectTransformMatrix`.
