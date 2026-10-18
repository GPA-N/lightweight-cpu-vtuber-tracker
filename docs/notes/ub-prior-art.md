# Upper-body webcam tracking: prior art

Date: 2026-10-17. Scope: open-source and widely used webcam VTuber trackers, checked for models, reported cost, arm and hand problems, lower-body handling, and lightness tactics.

Verification key: "Verified" means the claim was read from a source on the date shown. "UNVERIFIED" means the claim comes from memory, a secondary summary, or an author statement that was not reproduced. GitHub API calls hit a rate limit during this session, so repo push and release dates marked from the API are the ones fetched before the limit; the rest are marked UNVERIFIED.

## Table

| Project | Models / pipeline | Reported cost | Lessons for us | URL | Verified? |
|---|---|---|---|---|---|
| MediaPipe Pose (BlazePose) | Pose detector 224x224 then landmarker 256x256; 33 landmarks; lite/full/heavy variants; `num_poses` default 1; VIDEO running mode. Repo last pushed 2026-10-16, release v1.1.1 dated 2026-10-14. | Google reports 30+ fps on a Pixel 2 (2020 blog, hardware and date stale). Current desktop CPU ms/frame: UNVERIFIED. | Use the lite variant at low resolution for the every-frame pose pass. Set `num_poses` to 1. Use VIDEO mode so the tracker reuses its ROI between frames. | https://developers.google.com/mediapipe/solutions/vision/pose_landmarker ; https://github.com/google-ai-edge/mediapipe | Partly. API facts verified 2026-10-17 (the ai.google.dev URL redirected to developers.google.com). Pixel 2 number is a 2020 claim, stale. |
| MediaPipe Holistic (legacy solutions) | Pose first, then hand and face ROIs derived from pose each frame. Re-crop models cost about 10% of the model they feed. Hand and face inference is skipped when those parts leave the frame. | No current numbers. | This is the closest existing design to what we want: pose as the ROI prior for hands. Pose-to-hand ROI is cheap, so hands only run where wrists are. Status of Holistic as a supported API: UNVERIFIED (it is in the legacy solutions set). | https://ai.google.dev/edge/mediapipe/solutions/guide (redirects to developers.google.com) | Partly. ROI and 10% facts from docs read 2026-10-17; legacy status UNVERIFIED. |
| OpenSeeFace | Face only. ONNX face landmarks, models -1 to 3. Pushed 2026-09-18; release v1.20.5 dated 2026-09-14. | Author gives fps per model on one CPU core. Exact figures not copied into this note: UNVERIFIED for our hardware. | Shows that a light ONNX face model at low model index runs fine on one core. It has no body, hand, or arm tracking, so it is a face-side reference only. | https://github.com/emilianavt/OpenSeeFace | Repo metadata verified 2026-10-17 via API before rate limit. Per-model fps: UNVERIFIED in this note. |
| Warudo | Built-in MediaPipe. Pose gives wrists and fingers only (no full arm chain in the handbook). CPU or GPU, GPU is the default. | Handbook says tracking FPS 15 to 30. Page last updated 2024-07-24 (stale for a 2026 build). | Hands need the user centered and fully visible. Wrists and fingers from pose plus hand model is a real product choice. Tracking FPS is capped by the webcam. | https://docs.warudo.app (Korean handbook page; last updated 2024-07-24) | Partly. Fps and wrist claims read from handbook; date stale. Current version: UNVERIFIED. |
| VSeeFace | Face tracking on webcam. Hands come from a Leap Motion device, not the webcam. | Default camera cap 30 fps. Raising to 60 fps "will double the CPU usage of the tracking process" (FAQ, undated). Version label 1.13.38c5; release date UNVERIFIED. | Hard lesson: the author says webcam hand tracking "don't work well enough to warrant spending the time" (FAQ, undated). Cap the camera at 30 fps by default. Doubling fps doubles tracker CPU. | https://www.vseeface.icu/ | Partly. Quotes read 2026-10-17; page has no dates. |
| VNyan | Not checked in this session. | UNVERIFIED | UNVERIFIED | UNVERIFIED | No |
| XR Animator (ButzYung/SystemAnimatorOnline) | Webcam pose and fingers. License CC BY-NC-SA 4.0. Repo pushed 2026-09-20. | Author claim: 20+ fps for pose and fingers, 30 fps body, on a GTX 1650-class GPU. Author claim, not measured here: UNVERIFIED. | Shows that GPU-class numbers are the only ones authors publish. Our target is CPU on a desk webcam, so these numbers do not transfer. Non-commercial license blocks reuse of code. | https://github.com/ButzYung/SystemAnimatorOnline | Repo metadata verified 2026-10-17 via API before rate limit. Fps claims UNVERIFIED. |
| VMagicMirror | Webcam full-body and hand tracking; exact model list UNVERIFIED. Repo pushed 2026-10-01; release v5.0.0 dated 2026-06-06. | UNVERIFIED | Active project, recent release. Read its tracker code only if we need a reference for arm IK. Do not run downloaded binaries from it in our tree. | https://github.com/malaybaku/VMagicMirror | Repo metadata verified 2026-10-17 via API before rate limit. Model list UNVERIFIED. |
| Kalidokit | Web pose, face, and hand solvers on top of MediaPipe. Repo pushed 2025-08-18, marked deprecated. | UNVERIFIED | STALE. Last push 2025-08-18 and marked deprecated. Successor: call MediaPipe Tasks directly and keep our own solver. Successor status UNVERIFIED beyond the deprecation notice. | https://github.com/yeemachine/kalidokit | Repo metadata verified 2026-10-17 via API before rate limit (pushed date). Deprecation flag read from repo. |
| Amica | Web VRM companion. Tracker models UNVERIFIED. Repo pushed 2026-09-22. | UNVERIFIED | Useful only as a web-side reference for rendering and avatar loading, not a tracking budget reference. | https://github.com/semperai/amica | Repo metadata verified 2026-10-17 via API before rate limit. Models UNVERIFIED. |
| DarkStar1997/vtubing | Three MediaPipe models run in VIDEO mode on CPU: FaceLandmarker, HandLandmarker, PoseLandmarker. Pose gives upper and lower arm quaternions by direction-vector matching. Hands give per-joint finger curl (radians). Shoulders not described. Lower body and idle not described. Says "All tracking runs on CPU". MIT license. | No numbers given. README says latency is dominated by webcam fps (15 to 30 typical). | Closest match to our design: three MediaPipe models on CPU, pose-driven arms. Use direction-vector arm matching instead of full IK. No lower-body or idle handling, so we add those ourselves if needed. | https://github.com/DarkStar1997/vtubing | README read 2026-10-17. Release or push date: UNVERIFIED (API rate-limited). |
| TxbiG/VAvatar | Native project listed as updated 2026-08 with MediaPipe use. | UNVERIFIED | Possible native reference. Check before relying on it. | https://github.com/TxbiG/VAvatar | UNVERIFIED (API rate-limited, MediaPipe use not read). |

## Stale or unmaintained flags

- Kalidokit: last push 2025-08-18, deprecated. Successor: MediaPipe Tasks called directly.
- Warudo handbook: last updated 2024-07-24. Current Warudo behavior UNVERIFIED.
- BlazePose 30+ fps on Pixel 2: 2020 phone-class claim. Not a desktop CPU number.
- VSeeFace FAQ: undated. Treat its numbers as author statements.

## Desk framing: no hips in frame

Most users will show only head, shoulders, arms, and hands. Hips (MediaPipe pose landmarks 23 and 24) are usually out of frame, so the design must not depend on them. Findings below are weighted to that case.

What works without hip landmarks:

- MediaPipe Pose (lite): still emits all 33 landmarks when hips are out of frame, with low `visibility` and `presence` scores. Use those scores to gate hips out and never read hip positions as data. Whether the detector still locks on reliably with the bottom half cropped: UNVERIFIED, needs a test clip.
- Shoulders (11, 12), elbows (13, 14), wrists (15, 16), and the hand points (17 to 22) are all upper-body. Arms and hands need no hip data.
- Scale and torso reference: use shoulder width (11 to 12 distance) as the body scale and shoulder midpoint as the torso anchor. Do not use hip-to-shoulder length.
- Torso lean and yaw: estimate from nose and shoulders only. This is weaker than with hips, so keep it small in amplitude and smooth it. Accuracy: UNVERIFIED.
- Holistic-style hand ROIs: derived from wrists, so they need no hips. The 10% re-crop cost figure still applies.
- DarkStar1997/vtubing arm matching: direction vectors shoulder to elbow to wrist. No hip input. Works as-is on desk framing.
- Hand model (HandLandmarker): independent of pose. Works on any visible hand. Quality on a desk webcam is the risk, per the VSeeFace author.

What does not work or needs care:

- Anything that uses hips for root position, sitting or standing detection, or body-centering. Drop it.
- Full-body detector fallback: if the pose detector was trained mostly on full bodies, a half-body crop may lower detection rate. UNVERIFIED. Test before relying on it.
- Warudo-style wrist and finger output needs no hips, so it is a valid desk-mode reference.
- XR Animator's 30 fps "body" number is a full-body claim. Do not use it to budget desk mode.

Test plan, in the cloud only (the project rule bans local compute): run the lite pose model on a desk-framed clip (head to hips cropped) on Kaggle or Colab. Record per-landmark visibility for 23 and 24, detection rate, and wrist and shoulder stability. This decides whether desk mode uses pose as-is or needs a half-body crop.

## Lessons for our pipeline

Our own measurements (from `docs/notes/heavy-onnx.md`) show the face ONNX tracker at about 17 ms per frame at about 15 fps, using about 25% of one core, with a 30 fps camera cap. The camera plus tracker without a window measured 13% CPU and 55 MB. Those are our numbers, not from prior art.

Adding pose plus hands roughly means one more model per frame. Using prior-art structure:

1. Run pose every frame at the lite variant and low input resolution. This is the only model that must run every frame.
2. Derive hand ROIs from the wrists. Run the hand model only when the wrist landmark is visible, and skip it when the wrist is out of frame (the Holistic rule). Alternate hand inference across frames if the budget is tight.
3. Drive arms from pose plus hands: shoulder and elbow from pose, finger curl from the hand model. Use direction-vector matching as in DarkStar1997/vtubing.
4. No lower body. Desk framing with the camera at chest height means legs are not in frame. No idle sway for the body.
5. Keep the 30 fps camera cap. Do not raise it for hands, because doubling fps doubles tracker CPU (VSeeFace FAQ).
6. Budget in ms per frame, not fps. Our target numbers for pose and hands are UNVERIFIED until measured on the target PC.
7. Hand tracking from a desk webcam is weak. VSeeFace's author says so directly. Plan for hands to be optional, degrade cleanly to pose-only wrists, and measure before committing.

## Recommended strategy

Build pose-first with a small budget. Run the lite pose model every frame at low resolution. Gate the hand model on wrist visibility from pose, and alternate it when CPU is tight. Drive arms from pose and fingers from hands. Skip lower body and idle body motion. Keep the 30 fps camera cap. Measure ms per frame for pose alone, pose plus hands, and pose plus hands on alternate frames before choosing the default. Every budget number in that plan is UNVERIFIED until we measure it on our hardware, with no build or run on the PC outside the cloud rule in the project notes.
