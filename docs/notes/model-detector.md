# Face detector: fernandotonon/QtMeshEditor-models `mocap/face/face_detector.onnx`

Task 1.3, `cpu-face-tracking-spike`. No inference run.

## Sources and link to the Hub revision

Repo sources (GitHub, commit `760e7a390a2da197451bf37219397e2c15d5f021`, abbreviated `760e7a39`):
- `scripts/export-facecap-onnx.py` (converter, 37082 bytes). Cited as **[C]** with line numbers.
- `docs/MOCAP_SPIKE.md` (spike doc, 11643 bytes). Cited as **[S]**.

Both were fetched from `raw.githubusercontent.com/fernandotonon/QtMeshEditor/760e7a39/<path>` and re-checked line by line. Earlier, I cited local download names (`gh_export.py`, `gh_spike.md`) in place of repo paths. The content is identical to the earlier fetch, so the line numbers hold. The names were wrong.

Link between `760e7a39` and Hub revision `d14119a4`: **INFERRED** from matching I/O only (input 128x128, range [-1,1], outputs [1,896,16] and [1,896,1]). Not proven. `mocap_NOTICE.txt` names the converter and the repo but no commit.

Hub facts:
- Revision `d14119a40dfeef208e4e724dfaceb2640d2df95b`. Source: `X-Repo-Commit` on `https://huggingface.co/fernandotonon/QtMeshEditor-models/resolve/main/mocap/face/face_detector.onnx`, and `sha` at `https://huggingface.co/api/models/fernandotonon/QtMeshEditor-models`.
- Size 418437 bytes. SHA256 `02a04d5d37c3558dc4d5274f7f8f0f0f01ac94e46c5ffb2cee82395d47e23181`. Source: LFS metadata, matches the download.
- ONNX header: ir_version 8, producer `tf2onnx`, opset 18 (default domain), `ai.onnx.ml` opset 2. Source: wire-format read under `python -I`.

## I/O (graph read)

- Input `input`: float32 `[1,128,128,3]`, NHWC. Source: graph read. Also [C] line 10.
- Outputs: `regressors` float32 `[1,896,16]`; `classificators` float32 `[1,896,1]`. Source: graph read. Also [C] line 11.
- Graph ops: Add, Concat, Conv, MaxPool, Pad, Relu, Reshape, Transpose. No Sigmoid, NMS, threshold, or normalization op. The first op after the input is Transpose.

## Preprocessing

1. Letterbox [C] 245-261. Keep aspect ratio, `s = 128 / max(w,h)`, centre the image: `ox = (128 - w*s)/2`, `oy = (128 - h*s)/2`. Pad is equal on both leading sides (top and bottom for landscape, left and right for portrait). Fill is `borderValue=0` with `BORDER_CONSTANT`, so black RGB (0,0,0) before normalization. Resampling: `cv2.INTER_LINEAR`.
2. Normalize [C] 485: `x = lb/127.5 - 1.0`, range [-1,1]. [S] 80 says the same. [C] 41-42 gives `(v/255)*2-1`, a docstring equal to this.
3. Not in the graph. Source: op list.

## Anchors

- [S] 81-83 and [C] 471: strides `[8,16,16,16]`, min/max scale 0.1484375/0.75, anchor offset 0.5, aspect ratio 1.0, fixed size. Input 128.
- [C] 273-302 (`gen_ssd_anchors`): consecutive equal strides merge into one layer. Per cell, one anchor per scale, plus an interpolated scale. Anchor centre is `((x+0.5)/fm, (y+0.5)/fm)`, y-major. `fm = ceil(128/stride)`.
- Counts: stride 8 has `fm=16`, 2 anchors per cell, 512. Strides 16 (three layers merged) have `fm=8`, 6 anchors per cell, 384. Total 896.
- Only the centres are used. Scales affect the count only, because box size comes from `raw/128`.
- Converter asserts count equals model dim [C] 472-473.

## Decode

- [C] 305-329 (`decode_detections`), per anchor i:
  - `score = sigmoid(classificators[i])` [C] 308.
  - Keep if `score >= 0.5` [C] 309 (`keep = scores >= min_score`).
  - `cx = raw[0]/128 + anchor_x`, `cy = raw[1]/128 + anchor_y`, `w = raw[2]/128`, `h = raw[3]/128` [C] 315-318.
  - Keypoint k: `raw[4+2k : 6+2k]/128 + anchor` for k = 0..5 [C] 319-320, `num_keypoints=6` [C] 488. Same in [S] 83-85.
  - Box returned as `(cx - w/2, cy - h/2, w, h)`, normalized to the letterboxed 128 square, top-left origin [C] 325-326.
- Un-letterbox [C] 490-498 with `unletterbox_points` [C] 264-270: `x_orig = (x - pad_x)/fx`, `y_orig = (y - pad_y)/fy`, where `pad = (ox/128, oy/128, w*s/128, h*s/128)` [C] 261. Applied to box corners and keypoints. Result: box normalized to the original frame.
- Keypoint order [C] 783-784 (`face_detector.keypoints`): right_eye, left_eye, nose, mouth, right_ear, left_ear. [S] 84-85 gives the same list.
- Regressor layout: 4 box values, then 6 keypoints x 2 = 16.

## NMS

- Weighted NMS, default `min_suppression_threshold=0.3` [C] 332. Sorted by score [C] 343. Cluster test is `iou > threshold`, strict [C] 347-348.
- Weight = `score / sum(cluster scores)` [C] 350-354. Box and keypoints are accumulated as weighted sums [C] 355-356.
- Output: the best detection's dict with `box` and `keypoints` replaced by those weighted sums [C] 357-359. The output box is therefore the weighted average, not the best detection's own box.
- Runs in Python, not in the graph. [S] 85-86 agrees.

## Score

- `sigmoid(x) = 1/(1+exp(-x))`, input clipped to [-100,100] [C] 434-437 (`x = np.clip(x,-100,100)` at 435). [S] 85 agrees.

## Clipping

- The converter clips only the sigmoid input [C] 435.
- Box, keypoint, and ROI values are not clipped anywhere in the detector path [C] 315-326, 490-498. Behaviour outside [0,1] is therefore undefined by the converter, and boxes can extend past the frame. Not clamped in [C] or [S].

## ROI (for the next stage, same detector output)

- [C] 364-376 (`rect_from_face_detection`), inputs are the un-letterboxed box and keypoints, `W,H` = original frame size:
  - Eye vector `dx = (kp1.x - kp0.x)*W`, `dy = (kp1.y - kp0.y)*H`, with kp0 = right_eye, kp1 = left_eye.
  - `angle = normalize_angle(-atan2(-dy, dx))`, target 0 [C] 374. `normalize_angle` is `a - 2π·floor((a+π)/(2π))` [C] 393-394.
  - Square side `max(w*W, h*H) * 1.5`, centre `(cx*W, cy*H)` [C] 375-376.
- [S] 87-91 gives the same formula.
- Keypoint order, rotation keypoints `[0,1]`, target 0, scale 1.5, square_long: [C] 785-786.

## Decisions the Rust code must make

1. Score threshold, then weighted NMS at IoU 0.3, strict `>`. Recommended: use this, same as converter. Source: [C] 309, 332, 343-359; [S] 85-86. Channel order and resampler (from the same script): input is RGB (docstring lines 10 and 42; images converted with `.convert("RGB")`), and the letterbox warp uses `cv2.warpAffine(..., flags=cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT, borderValue=0)`, plain bilinear, NOT an antialiased resampler (docstring of `letterbox`, lines ~396-398). The Rust resize must be plain bilinear (no area or antialiased filter) to match the reference parity; the first-run check in task 4.7 compares detector scores on one image. Plain NMS would drop the keypoint averaging the converter relies on.
2. Anchors: generate at startup with the [C] 273-302 formula, assert count 896 (as [C] 472-473 does). Recommended over a hardcoded table, since the table is just the same output and the formula is short. Parameters: strides `[8,16,16,16]`, min 0.1484375, max 0.75, offset 0.5, aspect 1.0, input 128. Counts: 512 (stride 8) + 384 (stride 16 group) = 896.
3. Letterbox pad side: centred, half the remainder on each leading side, zero fill before normalization (black, becomes -1). Recommended: match [C] 245-261 exactly, bilinear, same pad values stored for un-letterboxing. Source: [C] 245-270.
4. Clip policy: clip only the sigmoid input to [-100,100]. Do not clamp boxes or keypoints, to match the converter's parity. If Rust needs bounded boxes, that is a new decision, not a parity match. Source: [C] 435; box path [C] 315-326, 490-498.

## NOT FOUND

- In-graph normalization, sigmoid, threshold, or NMS. Checked op list, absent.
- Hub I/O model card for `mocap/face/`. Root `README.md` has no contract. `mocap/face/README.md` returned "Entry not found".
- ONNX `metadata_props`. None in the header read.
- Commit linkage `760e7a39` to Hub `d14119a4`. Not proven, see INFERRED above.
