# Notes for AI coding agents

Setup Assistant: a Rust + Tauri 2 desktop app that suggests sim racing setup changes from
described problems and live telemetry (RaceRoom first). Read [ADR.md](ADR.md) before changing
direction; [CONTRIBUTING.md](CONTRIBUTING.md) has the full developer guide.

Scope, order and status are in [docs/roadmap.md](docs/roadmap.md): one GitHub issue per item,
worked one issue per PR. Before picking or starting work, read its Status section and the
**Now** marker, and run `gh pr list` for what is already in review.

Issues are implemented by background workers, one per issue in its own worktree and PR, with
this session as coordinator: use the `implement` skill.

## Ground rules

- `crates/setup-core` stays game- and UI-agnostic. Game specifics go in adapter crates
  implementing `setup_core::live::Source`.
- Setup advice is data: edit `crates/setup-core/rules/gt.json`, not code.
- Don't read, decrypt or write RaceRoom setup files (ADR-1).
- Detection thresholds are guesses being tuned on real data. Don't change them without a
  recording or a stated reason.
- Significant design decisions get a new ADR in `ADR.md` (append; supersede, don't rewrite).

## Before you finish

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

New detectors need a demo case in `crates/setup-core/src/demo.rs` and a test that asserts
exact detections. New Tauri commands need a branch in `crates/setup-core/examples/ui_preview.rs`.
A PR that finishes a roadmap item or listed bug sets its Status row in `docs/roadmap.md`
(rules in its Status section).

## Driving tests

What still needs testing in RaceRoom, and how past sessions went, is in
[docs/sim-testing.md](docs/sim-testing.md). Use the `sim-test` skill to plan a session and to
log its results there.

## Checking UI changes

- Browser: `cargo run -p setup-core --example ui_preview`, open http://127.0.0.1:1420, then
  Live telemetry → Try demo lap.
- Real app window: start it with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`
  and drive the page over the Chrome DevTools Protocol.
- On Windows, if linking fails with `LNK1104`, set `CARGO_TARGET_DIR` to a short path.
