---
name: implement
description: Coordinate implementing roadmap items and GitHub issues through background workers, one per issue, each in its own git worktree and PR. Use when the user asks to implement or fix an issue ("implement #43"), to work on the next roadmap items, or to merge the resulting PRs.
---

# Implementing issues: coordinator and workers

This session is the **coordinator**. It picks issues, dispatches one **worker** per issue,
reviews each worker's PR and merges with the user's approval. Workers do the coding; the
coordinator keeps the overview and stays out of the files workers are editing.

## 1. Pick

1. Read the Status section and the **Now** marker in `docs/roadmap.md`, and run `gh pr list`.
2. `gh issue view <n>` for each candidate. Note its "Done when" and the files it will touch.
3. Group into **waves**. Issues in one wave touch disjoint files and don't depend on each other.
   An issue that shares a file with another (e.g. `live.rs`, `telemetry.rs` + `lib.rs` tests)
   waits for the earlier one to merge.

Done when: every chosen issue is in a wave, and the user knows the waves.

## 2. Dispatch

Per issue in the current wave, start an `Agent` with `isolation: "worktree"` and
`run_in_background: true`, using the brief in [WORKER.md](WORKER.md) with its placeholders
filled in. Give each worker its own `CARGO_TARGET_DIR` (`C:\t\w<issue>`) so parallel builds
don't lock each other.

While workers run, the coordinator does work outside their files: docs, `sim-testing.md`,
answering the user. It never predicts a worker's result before its notification arrives.

## 3. Review

When a worker reports:

1. Read the PR diff (`gh pr diff <n>`) against the issue's "Done when" and the ground rules in
   AGENTS.md. Check: tests that would fail without the change, roadmap Status row set, ADR for a
   design decision, docs for user-facing changes, Tauri command ↔ `ui_preview` parity.
2. Detector changes: run `replay` on the local recordings before and after and compare.
3. CI green (`gh pr checks <n>`). A red check caused by the toolchain, not the PR, gets its own
   small fix PR first.
4. Problems go back to the same worker via `SendMessage`, so it keeps its context.

Done when: each PR is either ready or sent back with concrete fixes.

## 4. Merge

Merging is the user's call: present the ready PRs, the order, and ask.

1. When several PRs touch the same files, merge them all into a scratch branch from
   `origin/main` first and run fmt, clippy and tests on the combination. Text-clean merges can
   still break tests that assert exact detections.
2. Merge in order with `gh pr merge <n> --merge`, checking `mergeStateStatus` is `CLEAN`
   before each.
3. Watch main's CI after the last merge.
4. Then: start the next wave, move the **Now** marker when a roadmap step is complete, and add
   a row to the To test list in `docs/sim-testing.md` for anything only driving can confirm.

Done when: main is green, roadmap Status matches what merged, and the user has a short summary
(PRs, what each does, what to test on track, what's next).

## Gotchas

- Use the Bash tool for `gh`/`git` commands with quotes or `--jq`; PowerShell splits the
  arguments. Commit with `git commit -F <file>` and a message file in the scratchpad.
- Runs stuck "queued" with 0 jobs after a GitHub outage can't be cancelled or deleted; ignore
  them. The next push starts a fresh run.
- The runners use the latest stable Rust. Run `rustup update stable` locally when CI's clippy
  disagrees with yours.
