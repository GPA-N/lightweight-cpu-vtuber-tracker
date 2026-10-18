# Running the spike checks (tasks 1.10, 4.7, 5.4, 6.3, 7.6, 8.2, 8.3)

All code tasks are built and tested without a model, camera, or window. The checks below need your laptop, your camera, and your face, so you run them. Paste each printed output back; task 8.3 (`docs/spike-results.md`) is written from those outputs.

Pass/fail targets (from the specs): median tracking latency under 50 ms, at least 30 rendered FPS, peak working set at most 1024 MB, all on your laptop.

## 0. Build once, in release mode

```powershell
cargo build --release -p bench -p app
```

Use release builds for every measurement below. Debug builds are many times slower and will fail the latency target for the wrong reason. The first build compiles Bevy and ONNX Runtime and takes a while.

Models are already downloaded and hash-checked in `models\` (`scripts\fetch_models.ps1` re-checks them). Seed-san is in `samples\Seed-san.vrm` (`scripts\fetch_seed_san.ps1` re-checks it). Both are git-ignored.

## 1. Task 1.10: hardware notes

Fill in `docs\notes\hardware.md` with CPU model, RAM, camera model, power plan, and whether DirectX 12 or Vulkan is available (run `dxdiag`, look at the Display tab for DirectX feature levels). The bench report also prints CPU, RAM, and power state, but not the camera model.

## 2. Task 4.7: one image, real models

Take a front-facing photo of your own face and save it as `fixtures\private\face.jpg` (that folder is git-ignored).

```powershell
.\target\release\bench.exe --check-image fixtures\private\face.jpg --models models
```

Expected: two final lines `PASS 478 landmarks inside image bounds: yes` and `PASS 52 blendshape weights within 0.0..1.0: yes`. Also paste all the `CHECK ...` lines. They settle the open assumptions: raw z sign, z scale ratio (canonical about 2.01), x handedness, which landmark input range works ([0,1] vs [-1,1]), and which of the three presence outputs to use.

## 3. Task 5.4: camera error messages

Run each case, then paste the one-line message (no crash expected, exit code 1):

```powershell
.\target\release\bench.exe --seconds 5 --models models
```

- Camera unplugged or disabled in Device Manager: expect "no camera was found".
- Camera held by another program (open the Windows Camera app first): expect "... is in use by another program". The in-use detection is text-matching only and not verified against a real Media Foundation error, so tell me the exact text if it differs.
- Camera blocked in Windows Settings > Privacy & security > Camera: expect "camera access is blocked".

## 4. Task 6.3: 60-second measured run

Plugged in, good light, face in view:

```powershell
.\target\release\bench.exe --seconds 60 --models models
```

Paste the whole report. It lists median and 95th percentile tracking latency, delivered camera FPS, tracking FPS, peak working set and private bytes, thread count, average CPU percent, and the three PASS/FAIL lines.

## 5. Tasks 7.6 and 8.2: the avatar

```powershell
.\target\release\app.exe --vrm samples\Seed-san.vrm
```

Expected: a window titled "Avatar spike" with the avatar. Turn your head left and right and open your mouth. Check:

- The avatar follows (mirror is on by default: the avatar acts like a mirror image of you; add `--no-mirror` to flip it).
- The head turns the right way for each axis. If yaw or pitch is reversed, tell me, since the axis signs are listed as expected-but-unverified in `docs/notes/canonical-face.md`.
- Close the window after about 60 seconds and paste the printed report (render FPS, memory, latency).
- Error path: `.\target\release\app.exe --vrm README.md` (not a VRM file) must print an error naming the file and exit without a crash.

`--seconds` is parsed but not enforced; close the window yourself.

## Assumptions the first live run settles

1. Raw landmark z sign and scale (task 4.7 CHECK lines).
2. Which landmark model input range is right (task 4.7).
3. Which output is the presence probability (task 4.7).
4. Whether the blendshape model's pixel coordinates are full-frame or crop-relative (judged by whether mouth and eye blendshapes respond sensibly in 8.2).
5. Interpolation used by the reference crop (bilinear assumed).
6. Axis signs of yaw, pitch, roll on the avatar (8.2).

## Licensing reminder

Seed-san is under the VRM Public License 1.0 and is for this private spike only. The model licenses are summarised in `docs\notes\licenses.md`: no model is fully cleared for a public release.
