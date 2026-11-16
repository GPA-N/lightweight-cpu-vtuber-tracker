# web/app: how the page works (stage 4 of ticket 001)

`index.html` + `app/main.js` replace the old MediaPipe stand-in. Everything the tracker does is the ported code of `web/body`, `web/face`, `web/hands`.

## Run
- Page: serve `web/` over http(s) (Cache API and workers need a secure context; `localhost` counts) and open it. `?ort=<dir url>` overrides ONNX Runtime Web (default jsDelivr 1.31.1), `?hf=<base url>` the InstantHMR files, `?rfdetr=<url>` the detector (tests use these).
- Tests (setup as `web/body/NOTES.md`; the e2e needs ffmpeg, network for three.js from jsDelivr, the community RF-DETR file in `%TEMP%\rfdetr-community\model.onnx`):
  - `node --no-warnings web/app/test/e2e.mjs`: three fake-webcam runs (photo_claws, photo_fists, blank), asserts, second visit uses the cache; `SHOTS=<dir>` for screenshots.
  - `REAL_HF=1 node --no-warnings web/app/test/e2e.mjs`: one run on the real Hugging Face URLs (CORS, download time).
  - `node --no-warnings web/app/test/rig.mjs`: the three-vrm rig of `avatar.vrm` equals `web-fixtures/rig.json`.

## How the parts connect
- `loader.js` downloads the 8 model files with progress, stores each URL in the Cache API (`vt-models-v1`), returns blob URLs; workers fetch those, so model bytes are never copied through postMessage.
- Workers (module): `body-worker.js` (BodyTracker; RF-DETR runs in its own nested worker), `face-worker.js` (FaceTracker), `hands-worker.js` (HandStage). Each is one thread of WASM.
- `main.js` captures a 640x480 frame per video frame, runs `AutoBrightness` once, and hands the frame to every idle stage (a busy stage skips the frame, like the app's detector). Hands get the latest body pose (an earlier frame's, as the app), then `HandSmoother` on the page.
- Render loop (rAF): `toAvatarSpace` (mirror, confidences) -> `solve` -> `advanceBody` -> bone local rotations (`avatar.js applyBody`); head from the face result with the head's parent motion removed (head.rs); expressions, brow/lip morphs (brows.rs) and bone-type gaze (gaze.rs). `humanoid.autoUpdateHumanBones` is off: the raw bones are written directly.
- `avatar.js rigFromVrm` builds the `rig.json`-shaped rig from the loaded VRM (glTF node order via the parser's associations; global rotation and position from the scene, so a 0.x model after `rotateVRM0` is in +z-facing space).
- `blendshape_map.json` is a copy of `data/blendshape_map.json`.

## Measured (headless Chrome, fake webcam, 3 workers + render loop, this machine)
photo_claws: body 345 ms compute / 3.4 updates per s, face 42 ms / 16 per s, hands 213 ms / 5.6 per s, render 60 fps. First tracked frames 7.9 s after Start with local files; real Hugging Face: 163 s from Start to tracking (about 215 MB).

## Remains / not verified
- Real webcam, other browsers, GPU-less laptops; VRM 0.x avatars (code path written, no 0.x file tested); the brow/lip morph path (the Twist sample has none, so it is untested on a model); bone-type gaze (the Twist sample reports no look-at applier, so only the expression route ran).
- The first visit is a 215 MB download; the Cache API can be evicted by the browser.
