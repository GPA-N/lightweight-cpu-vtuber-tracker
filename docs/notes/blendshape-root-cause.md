# Blendshape weights "too low / skewed": root-cause investigation (2026-10-17)

## Verdict

- The way we feed the blendshape model is NOT the cause. Measured below: the model ignores translation and uniform scale, so pixel, crop-relative and centred inputs all give identical outputs. Our input follows the MediaPipe convention (isotropic full-frame pixels; `docs/notes/model-blendshapes.md` lines 17-19).
- Mirror is NOT the cause. Mirroring runs AFTER the model and swaps output weights only (`crates/infer/src/tracker.rs:129` runs `blend.run(&px)` on unflipped landmarks, then `:139` calls `apply_mirror`). Landmarks are never flipped.
- The subset order is NOT the cause. It was already verified identical to MediaPipe `kLandmarksSubsetIdxs` at commit `ce3dff2c` and to `export-facecap-onnx.py` (`model-blendshapes.md` line 38). I did not re-diff it.
- What remains is landmark geometry (the senty-au landmark model or our crop), or the expectation itself. UNVERIFIED. Looking at test10.png by eye: the mouth is a wide grin with the corners pulled up and the upper teeth showing, and a beard covers the jaw. So smile 0.87/0.92 and mouthUpperUp 0.45/0.67 look plausible. "jawOpen should be high, eyeWide should be high" is an unproven expectation until MediaPipe's own output on this photo has been measured.
- No code change is proven. Do NOT change the input convention: see the norm0..1 row below.

## Evidence table (measured with `fixtures/private/out/bs_probe.py`, onnxruntime CPU, landmarks from `bench.exe --check-image` with DUMP_LM=1)

test10.png (678x452):

| input variant | jawOpen | eyeWideL | eyeWideR | eyeLookDownL | smileL | smileR | lowerDownL |
|---|---|---|---|---|---|---|---|
| pixels (current) | 0.464 | 0.012 | 0.041 | 0.536 | 0.873 | 0.921 | 0.728 |
| +500 px shift | 0.464 | 0.012 | 0.041 | 0.536 | 0.873 | 0.921 | 0.728 |
| scale x2 | 0.464 | 0.012 | 0.041 | 0.536 | 0.873 | 0.921 | 0.728 |
| centred (minus mean) | 0.464 | 0.012 | 0.041 | 0.536 | 0.873 | 0.921 | 0.728 |
| x/W, y/H (0..1) | 0.952 | 0.013 | 0.097 | 0.596 | 0.834 | 0.893 | 0.051 |
| flip x, no index swap | 0.757 | 0.027 | 0.078 | 0.486 | 0.827 | 0.900 | 0.693 |
| y up | 0.626 | 0.035 | 0.026 | 0.555 | 0.697 | 0.835 | 0.778 |

test8.png (552x556, calm): current jawOpen 0.136, eyeWide 0.002/0.003, smile 0.000/0.000, eyeLookDownL 0.464. The 0..1 variant is almost the same (0.125), because test8 is almost square.

What the table shows:
1. The outputs are bit-identical under shift, uniform scale and centring. The graph normalises internally: node names include `tf.compat.v1.norm` and `GhumMarkerPoserMlpMixerGeneral/tf.math.truediv` (probe "first ops" print). So pixel, crop-relative and normalised conventions cannot matter, as long as the aspect ratio is kept.
2. The x/W, y/H row raises jawOpen to 0.95 only because it squashes the face anisotropically (678/452 = 1.5). mouthLowerDown falls from 0.73 to 0.05 in the same row, so this is a distortion, not a fix. MediaPipe scales x by width and y by height (`landmarks_to_tensor_calculator.cc` `GetAttributeScale`, cited in `model-blendshapes.md` line 18). On normalised landmarks that gives isotropic pixels, which is what we already pass.
3. eyeLookDown is about 0.46-0.54 on both photos (calm and surprised). That is a consistent bias, which points to iris or eyelid landmark placement. UNVERIFIED.

## Side finding (not proven related)
`bench` prints `CHECK presence: probability 0.0339 sigmoid(logit) 1.0000 -> UNEXPECTED` on test10 (`fixtures/private/out/test10_dump.txt` line 6). The two presence outputs of the landmark ONNX disagree (`model-landmarks.md` lines 51, 138). If the landmark model or its output mapping is off, every blendshape inherits the error.

## External search
No GitHub or Reddit report was found for "MediaPipe eyeWide/jawOpen always low". The single web search turned up only third-party per-channel gain workarounds (e.g. https://docs.warudo.app/docs/mocap/openseeface, https://pylips.readthedocs.io/en/latest/tutorials/teleoperate.html). No fernandotonon source beyond what `model-blendshapes.md` already records was re-fetched.

## How to prove the remaining hypothesis (Kaggle, under 5 min, CPU)
Run MediaPipe FaceLandmarker (`face_landmarker.task`, `output_face_blendshapes=True`) on test8 and test10. Save (a) its blendshapes and (b) its 478 landmarks in pixels. Then feed (b) through our `face_blendshapes.onnx`:
- If MediaPipe's own blendshapes are about the same as ours (jawOpen about 0.5, eyeWide about 0), the model behaves this way on this photo and there is no pipeline bug. The fix is per-channel gain or a calibration in `pose` (a product decision for the user).
- If MediaPipe gives a high jawOpen or eyeWide AND our ONNX on MediaPipe landmarks reproduces it, the root cause is our landmark stage (the senty-au model, the crop rect or the `landmarks_to_frame` warp). Diff MediaPipe landmarks against `test10_dump.txt` per point to locate it.

```python
# kaggle: pip install mediapipe onnxruntime ; upload test8.png test10.png face_blendshapes.onnx landmark_subset_146.json
import mediapipe as mp, numpy as np, json, urllib.request, onnxruntime as ort
urllib.request.urlretrieve("https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/1/face_landmarker.task","fl.task")
from mediapipe.tasks.python import vision, BaseOptions
det = vision.FaceLandmarker.create_from_options(vision.FaceLandmarkerOptions(base_options=BaseOptions(model_asset_path="fl.task"), output_face_blendshapes=True, num_faces=1))
sub = json.load(open("landmark_subset_146.json")); s = ort.InferenceSession("face_blendshapes.onnx")
for f in ["test8.png","test10.png"]:
    img = mp.Image.create_from_file(f); r = det.detect(img); W,H = img.width,img.height
    bs = {c.category_name: round(c.score,3) for c in r.face_blendshapes[0]}
    lm = np.array([[p.x*W, p.y*H] for p in r.face_landmarks[0]], np.float32)
    np.savetxt(f+".mp_lm.txt", lm)
    o = s.run(None, {s.get_inputs()[0].name: lm[sub][None]})[0].reshape(-1)
    print(f, "MP:", {k: bs[k] for k in ["jawOpen","eyeWideLeft","eyeWideRight","eyeLookDownLeft","mouthSmileLeft"]})
    print(f, "ONNX on MP landmarks: jawOpen", o[25], "eyeWideL", o[21], "eyeWideR", o[22], "lookDownL", o[11], "smileL", o[44])
```

## Exact code change
None proven. Keep `crates/infer/src/blendshapes.rs` `pick_subset` / `run_raw` and the mirror order in `crates/infer/src/tracker.rs` as they are. Decide after the Kaggle run above.
