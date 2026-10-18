# Accuracy baseline (change fix-head-fit-and-expressions, tasks 1.1 and 1.2)

Debug build, CPU, single frames, mirror default. Photos are git-ignored (`fixtures/private/`).
Raw means the blendshape ONNX output before clamping, printed with `DUMP_LM=1 bench --check-image`.

| photo | z ratio (canonical 2.01) | head fit | jawOpen | eyeWideL | mouthSmileL |
|---|---|---|---|---|---|
| test4 | 2.32 | ok, residual 0.080 | 0.00 | 0.09 | 0.64 |
| test6 | 2.05 | FitResidualHigh | 0.00 | 0.00 | 0.02 |
| test7 | 1.16 | FitResidualHigh | 0.03 | 0.00 | 0.00 |
| test8 | 2.25 | ok, residual 0.093 | 0.14 | 0.00 | 0.00 |
| test10 (wide-open mouth) | 1.68 | ok, residual 0.094 | 0.46 | 0.01 | 0.87 |

Findings:
- Raw outputs are already inside 0..1 on all 5 photos (0 of 52 outside) and `_neutral` is 0.0000. The model output is already activated, so a sigmoid on the output is not needed (design decision 3).
- test10: `jawOpen` 0.46 and `eyeWide` 0.01 look too low for a wide-open mouth and wide eyes. Only a reference run (MediaPipe Tasks, task 1.3) can say whether the model or our input is at fault.
- Head fit fails on test6 (z ratio normal, 2.05) and test7 (1.16). The z ratio alone does not explain test6.
- Passing fits have residuals 0.080 to 0.094, so the 0.1 limit sits just above them and is not a margin.
- Still missing: a neutral frontal photo, needed for the "`jawOpen` below 0.15" scenario.

## Head fit variants (tasks 2.1 to 2.3)

Residual per variant (`DUMP_LM=1`, `RES` lines). Eye-normalised is weighted RMS error over the eye distance.

| photo | 11pt shipped | 11pt eye xyz | 33pt eye xyz | 33pt eye xy |
|---|---|---|---|---|
| test4 | 0.080 | 0.043 | 0.040 | 0.037 |
| test6 | 0.142 | 0.075 | 0.069 | 0.065 |
| test7 | 0.107 | 0.062 | 0.061 | 0.054 |
| test8 | 0.093 | 0.048 | 0.046 | 0.043 |
| test10 | 0.094 | 0.054 | 0.052 | 0.048 |

Decision: ship the 33-point basis with the eye-normalised xyz residual and limit 0.12. The x,y-only variant is only about 0.004 lower, so it is not worth a separate z scale. The 33-point basis is about 5 to 8 percent lower than 11 points. No variant separates test6 and test7 cleanly from the good photos, so the old 0.1 limit was rejecting ordinary faces. All five photos now fit; test7 comes out at yaw -36.6, consistent with its compressed z.

## MediaPipe reference (task 1.3)

Kaggle kernel `gpannn/vtuber-bs-ref-run` (CPU): MediaPipe FaceLandmarker on the same two photos, then our ONNX on MediaPipe's landmarks.

| photo | weight | MediaPipe | ONNX on MP landmarks | ours |
|---|---|---|---|---|
| test8 | jawOpen | 0.136 | 0.136 | 0.136 |
| test8 | eyeLookDownLeft | 0.463 | 0.463 | 0.464 |
| test10 | jawOpen | 0.464 | 0.464 | 0.464 |
| test10 | eyeWideLeft | 0.012 | 0.012 | 0.012 |
| test10 | mouthSmileLeft | 0.874 | 0.874 | 0.873 |
| test10 | mouthLowerDownLeft | 0.730 | 0.730 | 0.728 |

Mean landmark difference to MediaPipe: 0.000 of the eye distance (max 0.001) on both photos. Conclusion: our tracker output equals MediaPipe's. The raw result is in `fixtures/private/kaggle/out/result.json` (git-ignored).
