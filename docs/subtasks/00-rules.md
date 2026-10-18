# Rules for every subtask (read this and your own subtask file, nothing else)

You are a low-level worker. Do exactly your subtask. Do not improve anything else.

1. Process: invoke the Skill tool for `superpowers:verification-before-completion` before you report. The OpenSpec change is `openspec/changes/reduce-live-cpu-and-memory/`; when your subtask is done and measured, tick its line in `tasks.md` (`- [ ]` to `- [x]`) and add one row to `docs/subtasks/results.md`.
2. Builds: the laptop lags if builds stack up. ONE cargo process at a time, never in parallel. Always `$env:CARGO_BUILD_JOBS = '4'` first (PowerShell). Never `cargo clean`. Wait for a build to finish before anything else. Before you start, check nothing else is building: `Get-Process rustc,cargo -ErrorAction SilentlyContinue` must print nothing.
3. Release builds only for measuring: `cargo build --release -p bench -p app --bin live`.
4. Measure twice. Live runs: `.\target\release\live.exe --seconds 15 --out fixtures\private\out\live_<tag>_<n>` (uses the laptop camera, ~25 s each, at most 4 live runs per subtask). Read the numbers with:
   `Select-String -Path fixtures\private\out\live_<tag>_<n>\live_report.txt -Pattern 'average CPU|peak working set|peak private|render FPS|median tracking latency|DXGI'`
5. Accuracy guard (after any change that can touch model input or output): `$env:DUMP_LM=1; .\target\release\bench.exe --check-image fixtures\private\test10.png --models models | Select-String '^RAW (jawOpen|eyeWideLeft|mouthSmileLeft|browInnerUp)'` must print jawOpen 0.4637 (within 0.005), eyeWideLeft 0.0118, mouthSmileLeft 0.8731, browInnerUp about 0.6767.
6. Keep a change only if your subtask file's KEEP rule passes. Otherwise revert your edit exactly (restore the old code) and say so.
7. Never commit to git. Never edit files outside your subtask. Do not touch `data/blendshape_map.json`, `docs/notes/`, `fixtures/private/test*.png`.
8. Tool tips: in the Bash tool never put an apostrophe inside a heredoc; write files with the Write or Edit tool. Use `npx -y ctx7@latest library <name> <query>` then `npx -y ctx7@latest docs <libraryId> <query>` to check an API before you use it.
9. Finish with at most 8 lines: what you changed (file:line), the before/after numbers, KEEP or REVERT, tests result (`cargo test --workspace -q` must pass, `cargo clippy --workspace --all-targets -q` must print nothing).

Baseline for live comparisons: the old `live_base` run (face in view) had median tracking latency 11.3 ms, CPU average 113% of one core, peak working set 466 MB, peak private 858 MB, render 60 fps, DXGI 0.0 MB. The subtask 01 baseline runs had NO face in view (the user was away), so the tracker mostly idled: CPU average 84.2% and 93.4%, peak working set 456.6 and 463.4 MB, peak private 848.7 and 855.4 MB, render 60 fps, and the `median tracking latency` line reads 0.0 ms (with no face the tracker returns at once, so ignore that line in no-face runs). A face in view adds about 20 to 30 points of CPU. RULE: compare live runs only with runs of the same kind. Live subtasks (03, 04, 05, 06) measure the render and capture overhead, so run them with NO face in view and compare with the subtask 01 numbers; subtask 02 (tracker code) is measured on a still image with `bench`, which always has a face. Live CPU varies 10 to 15 percent between runs, so only trust a change that beats the baseline in both of your runs. Note for the project manager: this shows the non-tracker part (Bevy, capture, preview) uses about 85 to 90 points of CPU on its own, which is where the big savings are.

## Debate protocol (workers 02 to 06)

Every subtask is argued, not just run. The record lives in `docs/subtasks/debate/NN.md` (NN = your subtask number).
1. BEFORE you edit anything, the worker writes `## Hypothesis` (what you think causes the cost, why, citing a file:line or a measurement) and `## Prediction` (a number: how much CPU, MB or ms the change will save, and what would prove you wrong).
2. After measuring, the worker writes `## Result` (the real numbers, both runs) and `## Verdict` (KEEP or REVERT per the KEEP rule, and whether the hypothesis was right, wrong or unproven).
3. A second worker, the challenger, then reads that file and the code and must ATTACK: is the noise (10 to 15 percent) bigger than the effect, is there a confounder (camera fps, room light, another process, thermal state, a run order effect), was the right thing measured, is there a cheaper or better explanation, did the change break anything not tested. The challenger writes `## Challenge` with numbered objections and may run ONE extra measurement to test an objection (same build rules).
4. The worker answers each objection under `## Rebuttal` (accept, or refute with evidence). Two rounds at most.
5. Anything still disputed goes under `## Unresolved` and the project manager decides. A hypothesis that is not backed by a measurement stays marked UNPROVEN and the change is not kept on it.
The challenger does not edit code except to run its one extra measurement, and must restore the tree afterwards.
