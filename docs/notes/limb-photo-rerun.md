# Four photos re-run with the CIGPose-m limb stage (task 11.6)

Date: 2026-10-18. No camera was used. Photo path only: `APP_STILL_BODY` with the new `APP_LIMB=cigpose` option in `crates/app/src/main.rs`, run through `scripts/photo_to_avatar.ps1` (debug build, ONNX Runtime) and `scripts/photo_to_avatar_compose.py`.

## What the option does

`APP_LIMB=cigpose` replaces the shoulder, elbow and wrist points of the BlazePose result with CIGPose-m keypoints lifted to 3D by `avatar::arm_lift` (task 11.5). The person box comes from the face landmarks (`infer::person_box`); when the face tracker finds no face (hands_17 and hands_19) the whole frame is the box. Hands and fingers still come from the MediaPipe 2023 hand model, and the index and pinky points still come from BlazePose. Mixing the two sources is a shortcut for this check, not the final design.

Composites: `fixtures/private/out/photo_to_avatar/composite_{seed,twist}_*.png` (CIGPose) against `fixtures/private/out/photo_to_avatar_blaze/` (BlazePose, the earlier run). Seed-san composites were viewed one by one. Twist printed identical limb numbers (same pose input) and was not viewed again.

## Limb angles (degrees, Seed-san and Twist identical)

| Photo | Arm | Upper-arm elevation Blaze / CIG | Forearm elevation Blaze / CIG | Elbow bend Blaze / CIG | 2D estimate of the bend from the photo |
|---|---|---|---|---|---|
| hands_15 | left | -54 / -59 | 21 / 36 | 76 / 102 | not measured |
| hands_15 | right | -59 / -62 | 28 / 28 | 92 / 119 | about 135 |
| hands_16 | left (raised) | -83 / -86 | -87 / -88 | 9 / 3 | small |
| hands_16 | right (hanging) | -21 / -11 | 23 / 19 | 53 / 31 | about 0 (straight down) |
| hands_17 | left | 15 / 12 | 47 / 46 | 35 / 50 | not measured |
| hands_17 | right | 0 / 4 | 28 / 42 | 38 / 61 | not measured |
| hands_19 | left | -19 / -23 | 19 / 40 | 73 / 132 | about 151 |
| hands_19 | right | -69 / -83 | -81 / -78 | 12 / 5 | small |

The bend estimate is by hand from the overlay pixels in the composites (angle between the upper-arm and forearm 2D directions), so it is rough. Upper-arm and forearm elevations agree within about 15 degrees between the two stages. The difference is the elbow bend: CIGPose-m is closer to the photo on hands_15 right, hands_19 left and hands_16 right; the others are too small to judge.

## By eye

- hands_15, hands_19: the CIGPose arm folds the forearm up towards the hand as in the photo; BlazePose leaves it half open.
- hands_16: both similar; the hanging arm is straighter with CIGPose.
- hands_17: both poses of the left (screen-left) arm look right. The screen-right arm in both renders goes up and over behind the head, away from the photo, although its angles are almost the same as the left arm's. That is not caused by the limb stage: it is the same in the BlazePose render. Suspect the retarget or the avatar's right arm (Seed-san), to be checked separately.
- No render looks worse with CIGPose-m than with BlazePose.

## Verdict (design.md D10 revert rule)

The CIGPose-m stage is not worse than BlazePose on the four photos, by eye or by the numbers, so it stays the default candidate. The budget half of the rule (50 ms median, 30 fps, 1024 MB) is only checked live in task 11.7.

## Caveats

- Four still photos, no human labels, so "closer" is my reading of the images, not a score.
- Depth is a forward guess (design D10 cost): arms pointing at or behind the camera stay wrong (docs/notes/arm-lift.md).
- hands_17 and hands_19 have no face from the real tracker, so the first box was the whole frame. A cold start in a head-tilted pose still needs that fallback in the live path.
- Debug build timings were not recorded; the live gate measures speed.
- The hands_17 screen-right arm was checked afterwards. It is a Seed-san asset quirk, not a retarget bug. Twist renders both arms symmetric and correct for the same pose input. Seed-san carries a separate mechanical arm skeleton on its left side (nodes `robo_shoulder.L`, `robo_upper_arm.L`, `robo_forearm.L`, `robo_hand.L`, mesh `robo_arm`) next to the humanoid `shoulder.L`, `upper_arm.L`, `forearm.L`, `hand.L`. Our solver poses the humanoid bones (the RENDERED line matches the tracked angles), so the robo arm mesh does not follow them. Fixing it means copying the humanoid left-arm rotations to the `robo_*.L` bones for this model only; not planned. Use Twist for arm checks.

## BlazePose heatmap refinement (2026-10-18)

Our Rust BlazePose port and the Python port (l1) agreed within 0.004 shoulder widths, so the gap to official MediaPipe (median 0.055, mean 0.065, max 0.241 over six arm joints on four photos, fixtures/private/out/blaze_port_gap.py) was not a Rust bug. Cause: the landmark model has a 64x64x39 heatmap output that official MediaPipe uses (RefineLandmarksFromHeatmapCalculator, kernel 7, sigmoid, min confidence 0.5, from tensors_to_pose_landmarks_and_segmentation.pbtxt) and the port ignored. `refine_from_heatmap` in crates/infer/src/upper_body.rs now does the same. Result on the same photos: median 0.026, mean 0.032, max 0.108 (about half the gap). `VT_NO_HEATMAP=1` switches it off for A/B checks. Still different from official: the official graph also smooths landmarks over time and may differ in ROI details. Cost: a 7x7 window per landmark, negligible.
