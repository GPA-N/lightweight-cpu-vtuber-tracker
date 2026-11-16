# Notes for resuming, stage 3 (hands) of ticket 001, branch ticket/001-js-port

State: stage 3 done and committed (message starts "Stage 3 hands:"). Stage 4 (page) is next: `specs/tickets/004-integration/` in the main checkout.
Earlier stages: `web/body/NOTES.md`, `web/face/NOTES.md`. Results: `specs/tickets/003-hand-port/results.md` (main checkout).

## Last parity run (headless Chrome vs Rust, the stage fed the fixture `poseIn`)
palm box 4.0e-5 px, keypoints 8.1e-5 px | ROI 8.8e-5 px, 2.5e-6 rad | hand crop 2.4e-4 px, 8.4e-6 rad | landmarks raw 9.2e-4 px, z 4.1e-4 | smoothed 8.6e-4 px, z 2.3e-4 |
sides, counts and sources identical on all 67 | hands through the stage 1 solver: all bones 2.2e-3 deg, advance_body 2.2e-3 deg.
Timing: init 0.7 s; palm 36 ms; hand 52 ms for 2 crops; 89 ms per step.

## Harness
Same pattern as body and face. The hand clock of the dump is k * 33_333_333 ns; the smoother is fed that in seconds.

## For stage 4
- Run the hand stage in its own worker: the app overlaps it with the body model (`overlap` in hmr_body.rs), so it should run in parallel with the body worker, not behind it (body 165 ms + hands 89 ms would serialise to 254 ms). Not measured in parallel.
- Per frame: pose = previous good frame's `BodyTracker` pose (null on the very first frame only if you accept palm-only hands, else run body first); smooth with `HandSmoother.apply(hands, nowSeconds)`; `BodyTracker.attachHands(hands)` for the next `process`.
