# CIGPose-m limb stage (OpenSpec task 11.3)

Date: 2026-10-18. Machine: AMD Ryzen 5 7430U, release build, 4 threads.

## What was added

- `crates/infer/src/cigpose.rs`: trait `LimbStage` (person box in, 17 COCO keypoints out) and `CigPose`, which runs `models/upper_body/cigpose-m_coco_256x192.onnx` through `ModelRun` (task 11.1), so ort or OpenVINO per `VT_ENGINE`. BlazePose stays in `UpperBody`, untouched and selectable. Nothing is wired into `live` yet (that is 11.4 onward).
- Port of the `cigpose-onnx` 1.0.1 wheel, `inference.py`. Not an affine warp: the box is expanded to the 192:256 aspect, padded by 1.25, the centre-based rectangle is truncated to integers and clamped to the frame, the crop is resized with bilinear interpolation (OpenCV half-pixel centres, rounded to u8), converted to RGB, normalised with mean 123.675, 116.28, 103.53 and std 58.395, 57.12, 57.375, and laid out NCHW [1,3,256,192]. Outputs are SimCC x [1,17,384] and y [1,17,512]; keypoint = argmax / 2.0, score = min of the two logit maxima (a logit, not a probability), then mapped back with the crop origin and the crop-to-input scale.
- Box input is x, y, w, h in frame pixels. The reference boxes (x1, y1, x2, y2) are converted.

## Verification

Test `crates/infer/tests/cigpose_parity.rs` (`cargo test --release -p infer --test cigpose_parity -- --nocapture`) runs the four user photos with the reference boxes from `l2_cigpose_m_coco.json` and compares all 17 joints. It skips with a message when the private fixtures or the model are missing. Six unit tests in `cigpose.rs` cover the crop, decode, remap and normalisation.

| Photo | Max px error (any joint, x or y) | Joint |
|---|---|---|
| hands_15 | 0.288 | 7 |
| hands_16 | 0.310 | 6 |
| hands_17 | 0.714 | 2 |
| hands_19 | 0.877 | 8 |

Identical on ort and OpenVINO. Limit was 2 px. The residual comes from OpenCV's fixed-point bilinear resize and the 2-decimal reference boxes, and is below the SimCC step (0.5 px in input units).

## Timing

Whole call (crop, resize, normalise, run, decode) on hands_17 (547x365 frame, 40 timed calls after 5 warm-up): median 16.8 ms on ort (min 15.2, max 19.7), 17.1 ms on OpenVINO (min 15.9, max 18.2). The network alone was 15.3 ms (ort) and 15.7 ms (OpenVINO) earlier, so the pre- and post-processing add about 1.5 ms. Run-to-run noise on this laptop is 10 to 40 percent.
