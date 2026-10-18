# 2D to 3D arm lift (OpenSpec task 11.5)

Date: 2026-10-18. Code: `crates/avatar/src/arm_lift.rs`, 10 unit tests (written first, seen failing). Not wired into `live`; no model or camera involved.

## Where it lives and what it returns

In `avatar`, next to `BodyPose` / `ArmPose` (which use Bevy `Vec3`); `infer` does not depend on `avatar`, so the input is a plain `[Kp; 17]` (x, y in pixels, score) that the caller fills from the CIGPose stage. `ArmLifter::new(mirror).lift(&kp, LiftHints)` returns `Lifted { pose: BodyPose, left, right }` where `left` and `right` are the 2D scores of shoulder, elbow, wrist (0 when missing). `solve` and `apply_body_live` take the pose unchanged.

Axes match `to_avatar_space`: +x avatar left, +y up, +z toward the viewer (image z points away, avatar z is its negative). Mirror is applied once, as there: with `mirror` the person's left arm drives the avatar's right bones and x is negated. The mirror flag is fixed for the lifter's lifetime because the stored bone directions are in avatar space. Positions are in shoulder-width units (the solver only uses directions and the shoulder line angle). Only COCO 5 to 10 are read; hips are never used. Index, pinky and hand points are left empty.

## Method

- Unit: the pixel distance between the two shoulders, smoothed (alpha 0.2). With one shoulder missing the last scale is reused; with no scale yet the pose is empty.
- Bone lengths in shoulder widths: upper arm 0.75, forearm 0.62 (the values the task fixed; Drillis and Contini segment ratios of 0.186 H and 0.146 H against a biacromial width of about 0.259 H give 0.72 and 0.56, and COCO shoulder keypoints are joint centres that sit slightly wider than the acromia, so the task values are in range). The earlier handoff guess of 1.1 and 0.95 was not used; it was not checked against hands_15. The estimate grows toward the largest projected length seen (alpha 0.5 per frame, clamped to 0.7 to 1.4 times the prior) and never shrinks.
- Depth magnitude: `sqrt(max(0, L^2 - l2d^2))` per bone; elbow depth is the sum along the chain from the shoulder (z = 0).
- Sign, per bone: pick the candidate (+z or -z) with the lower cost, where cost = angle to the previous frame's direction of that bone in degrees, plus 8 degrees if it points away from the camera (forward prior). With no previous direction the prior alone decides (toward the camera). The previous direction is kept through missing frames; a flip happens only when the other branch is closer to the previous direction by more than the 8 degree prior, which in practice means near the image plane.
- Hook for the hand model: `LiftHints.left_forearm / right_forearm` (unit avatar-space forearm direction). If given it adds 0.5 degree of cost per degree of disagreement for the forearm sign. Not required; nothing produces it yet.
- Confidence: the joint is missing below score 0.45 (person-box.md: found joints score 0.49 to 0.85); a wrist needs its elbow, an elbow its shoulder.

## Measured error

Synthetic sequences: shoulder width 1 (100 px), both arms, bone lengths equal to the priors, upper arm uniform within 150 degrees of hanging down, forearm within 150 degrees of the upper arm (elbow flexion), 30 frames of smoothstep slerp between two random poses, 1500 sequences (3000 arms), fixed seed. Error is the angle between lifted and true bone direction, both bones, all frames. Classes (depth within 0.1 bone lengths of the image plane counts as neither side): Forward = no bone ever clearly behind the plane; Backward = a bone clearly behind and none crossing; Crossing = some bone clearly on both sides.

| Camera | Class | Arms | Median deg | p95 deg | Frames over 15 deg | Sequences with median over 15 deg |
|---|---|---|---|---|---|---|
| orthographic | Forward (gate) | 292 | 0.00 | 0.02 | 0.0% | 0.0% |
| orthographic | Backward | 308 | 68.99 | 147.84 | 88.6% | 92.5% |
| orthographic | Crossing | 2400 | 0.13 | 126.69 | 43.5% | 31.6% |
| orthographic | all | 3000 | 0.58 | 128.58 | 43.9% | 34.8% |
| perspective, camera 10 shoulder widths away | Forward (gate) | 292 | 4.65 | 14.86 | 4.9% | 0.0% |
| perspective | Backward | 308 | 73.30 | 149.43 | 93.2% | 96.4% |
| perspective | Crossing | 2400 | 11.07 | 128.37 | 46.3% | 37.5% |
| perspective | all | 3000 | 11.79 | 130.39 | 47.1% | 39.9% |

Gate (median and p95 within 15 degrees where the arm is not in the ambiguous case): met for Forward, orthographic and mild perspective. The perspective p95 of 14.86 is a thin margin. Real webcams are less mild: at about 60 cm a shoulder width of 40 cm is a camera distance of roughly 1.5 shoulder widths, so a hand near the camera is magnified much more than D = 10 and the error will be larger. This was not measured; the lift does not model perspective.

Forward-class robustness (informational, no gate):

| Condition | Median deg | p95 deg | Frames over 15 deg |
|---|---|---|---|
| 2D noise sigma 1 px (1% of shoulder width) | 1.07 | 6.25 | 0.2% |
| 2D noise sigma 2 px | 2.14 | 10.37 | 1.8% |
| true arms 10% longer than the prior | 4.80 | 19.11 | 10.6% |
| true arms 10% shorter than the prior | 7.43 | 22.48 | 18.6% |

A wrong length prior costs more than pixel noise: arms shorter than the prior get spurious depth when they are in plane, and the estimate cannot shrink. Per-person calibration of the two ratios (for example from a frame where the user holds both arms out sideways) would help; not built.

## Ambiguous cases

- Mirror branch (depth sign). A 2D projection fixes the depth magnitude but not its sign. The lift starts on the toward-camera branch and then follows it. If the arm really is behind the torso plane, or the user starts with the hand away from the camera, the whole bone stays on the wrong branch: 92.5% of Backward sequences are wrong (88.6% of their frames). Real arms are mostly in front of the plane; the random test over a 150 degree cone puts them behind far more often than a person at a desk would.
- Crossing the image plane. When a bone passes through the plane, "keep going" and "bounce back" give the same 2D track, so continuity cannot tell them apart. The lifter keeps the branch it was on. After a real crossing the arm is mirrored in depth until it returns to the plane: 31.6% of Crossing sequences have a median error over 15 degrees (37.5% with perspective). Raising the flip evidence would trade these for wrong flips on noise; the hand-model hint is the intended extra evidence.
- Arm pointing at the camera. The depth is then almost all of the bone and the 2D length is near zero, so the 2D direction carries almost nothing. Noise-free, the angle from the axis made no difference (worst error 0.00 degrees from 0 to 40 degrees off axis, on the forward branch); with noise the in-plane direction is jittery near the axis, and the sign is the same ambiguity as above. Not measured with noise.
- Torso yaw shrinks the shoulder width, which shrinks the scale and inflates the bone lengths in units of it. Not modelled or measured here.
- Missing joints: a bone whose joints go missing keeps its last direction as the sign memory, so a long dropout followed by a different pose can start on a stale branch.

## Open items

- Real keypoints are not tested here; run it on the CIGPose output of the four photos and the live run (11.6, 11.7) and look at the arms.
- Tune the three constants (length rise 0.5, away penalty 8 degrees, hint weight 0.5) on real footage.

## Hand-size depth cue (added 2026-10-18)

User report: the live avatar cannot tell an arm pulled backwards from one pushed forward (the 2D-only ambiguity above). crates/app/src/limb_body.rs::hand_depth_hints now feeds the lifter a forearm direction whose z sign comes from the apparent hand size: wrist to middle knuckle over shoulder width, reference 0.25 at shoulder depth, forward above 1.25 times that, backward below 0.75 times that, no hint in between or when no hand is detected. On the four photos the wrist depth relative to the shoulder comes out forward for hands_15, hands_16 (raised hand) and hands_19, and slightly backward for hands_17, which matches the pictures. Not measured on a real backward reach (no such frame yet); fists and foreshortened hands are weak cues, so a hint only wins when the hand size is clearly off. Thresholds are first guesses to tune on a live backward-arm take.

### Live data and retune (2026-10-18)

First live take (40 s, user reaching and pulling an arm back, VT_LIMB_LOG=1, 654 body frames): hand ratio was 0.28 to 0.30 relaxed, 0.38 to 0.47 reaching forward and 0.15 to 0.17 with the arm pulled back. The first thresholds (reference 0.25) flagged the backward frames correctly (hint backward) but the wrist depth stayed positive (1.2 to 1.7 shoulder widths), because only the forearm followed the hint and the upper arm kept the forward guess. Changes: the reference ratio is now 0.30 (far below 0.21, near above 0.375) and the forearm hint's depth sign also settles the upper arm (crates/avatar/src/arm_lift.rs). The retake (second 40 s run) recorded no person at the desk (all frames without hands), so the retuned code is verified only by unit tests and by the photos, not yet live. VT_LIMB_LOG=1 prints one line per body frame (hand ratios, hint signs, wrist depth) to stderr for the next take.
