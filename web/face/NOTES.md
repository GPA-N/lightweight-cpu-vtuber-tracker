# Notes for resuming, stage 2 (face) of ticket 001, branch ticket/001-js-port

State: stage 2 done and committed (see `git log`, message starts "Stage 2 face:"). Stage 3 hands and 4 page are next (specs in the main checkout `specs/tickets/00{3,4}-*/`).
Stage 1 notes: `web/body/NOTES.md` (harness, conventions). Results: `specs/tickets/002-face-port/results.md` (main checkout).

## Last parity run (headless Chrome vs Rust; 7 stills, 60 sequence steps, 90 face-lost blank steps)
a boxes 5.7e-5 px, keypoints 1.8e-4 px | b landmarks x,y 2.6e-3 px (limit 0.05), presence 6.5e-7 | c weights 4.7e-5 | d rotation 1.8e-3 deg |
e expressions 3.3e-5, brows 3.2e-5, gaze 3.6e-5 | f ran/found flags identical, face-lost replay weights 2.3e-6, rotation 2.2e-5 deg.
Timing: init 1.0 s; landmarks 18 ms, blendshapes 3 ms, detector 5 ms (every 5th step), process() 23 ms (27 ms with detector).

## Harness
Same pattern as body: `test/page.mjs` leaves JSON in `window.__results`, `test/parity.mjs` serves `/ort/ /models/ /fix/ /web/ /data/` and compares in node
using `../../body/test/lib.mjs`. The server block is duplicated from body (web/body must not be edited from here).

## For stage 4
- Order per camera frame: AutoBrightness (in place) -> FaceTracker.process -> BodyTracker.process (body ignores `face`).
- Face costs about 24 ms per step, body about 165 ms: put body (InstantHMR + MHR) in a worker; face may stay on the main thread or get its own worker. Running face and body in one worker serialises them (a face step would wait behind a body step); not measured in parallel.
