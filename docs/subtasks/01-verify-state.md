# 01 Verify the tree and record a clean baseline

Read `00-rules.md` first. Goal: prove the tree is sound after the earlier ONNX spinning change, and record a fresh baseline for the later subtasks. You change no code.

Steps:
1. Check nothing is building (rule 2). `$env:CARGO_BUILD_JOBS = '4'`.
2. `cargo build --release -p bench -p app --bin live`, then `cargo test --workspace -q` and `cargo clippy --workspace --all-targets -q`. Tests must all pass, clippy must print nothing. If something fails, stop and report the exact error; do not fix it.
3. Accuracy guard (rule 5).
4. Two live runs, tags `s01_1` and `s01_2` (rule 4).
5. Create `docs/subtasks/results.md` with a table header `| subtask | change | CPU avg % | peak WS MB | peak private MB | render fps | median latency ms | decision |` and write the two baseline rows (subtask `01`, change `none (baseline after spinning off)`).

KEEP rule: not applicable. Tick task 1 in `tasks.md` only if every command above succeeded.
