# Worker brief

The coordinator fills in `<…>` and passes this as the worker's prompt. Add what the
coordinator knows that the issue doesn't say: context from driving, files other workers are
editing right now, what is deliberately out of scope.

---

You are implementing GitHub issue #<n> (roadmap item <id>, "<title>") in
sitzriese89/setup-assistant, a Rust + Tauri 2 sim-racing setup assistant. You work in your own
git worktree. Do the whole job yourself and open a PR; the coordinator reviews and merges it.

**Read first:** AGENTS.md, CONTRIBUTING.md, the ADRs that bear on the change, the Status
section of docs/roadmap.md, and `gh issue view <n>`. The issue's scope and "Done when" are
authoritative. Then read the code you will change.

**Context from the coordinator:** <driving context, related issues, decisions already made>

**Other workers right now:** <issue: files>. Leave those files alone unless the issue can't
be done without them; if so, say so in your report.

**Out of scope:** <later issues this one must not absorb>

**Finish line**
- Tests that fail without the change and pass with it (detectors: a demo case in `demo.rs`
  asserting exact detections; new Tauri commands: a branch in `examples/ui_preview.rs`).
- Detector changes: `replay` output on the local recordings in
  `%APPDATA%\dev.simracing.setupassistant\recordings`, before and after, in the PR.
- A design decision gets a new ADR appended to ADR.md. User-facing changes update README or
  docs/how-it-works.md.
- In bash, `export CARGO_TARGET_DIR='C:\t\w<n>'`, then all pass:
  `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`.

**Git and PR**
- Branch `<type>/<short-name>`. Use the Bash tool for git and gh. Commit with
  `git commit -F <message file in your temp dir>`; messages end with a blank line and
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- PR body: what and why, how it was tested, `Fixes #<n>`, ending with
  `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
- After `gh pr create`, set the item's row in docs/roadmap.md to `Done (#<PR>)` in a follow-up
  commit and push. Wait for CI (`gh pr checks <PR> --watch`) and fix what fails.

**Report** (short): PR URL, CI result, design choices, how you verified, what is left open or
questionable, and anything only driving in RaceRoom can confirm.
