# Contributing

Thanks for helping! Bug reports, recordings of real laps, setup knowledge and code are all
welcome. Before larger changes, read [ADR.md](ADR.md): it explains why things are the way they
are. Opening an issue to discuss is a good first step.

## Development setup

### Windows (main platform)

1. [Rust](https://rustup.rs) stable (1.77 or newer).
2. [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
   with the "Desktop development with C++" workload.
3. WebView2 runtime (preinstalled on Windows 11).
4. RaceRoom, if you want live telemetry (optional: everything also works with the demo lap).

```bash
git clone https://github.com/sitzriese89/setup-assistant.git
cd setup-assistant
cargo test --workspace
cargo run -p setup-assistant
```

> **`LNK1104: cannot open file ...` while linking?** The path to the build folder is too long
> for the Windows linker. Clone to a short path (e.g. `C:\dev\setup-assistant`) or set
> `CARGO_TARGET_DIR` to a short folder.

### Linux / macOS

The core crates build and test anywhere:

```bash
cargo test -p setup-core -p r3e-telemetry
cargo run -p setup-core --example ui_preview   # UI in your browser, demo mode
```

Building the desktop app needs Tauri's system libraries
([Tauri prerequisites](https://tauri.app/start/prerequisites/)), e.g. on Debian/Ubuntu
`libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev`. RaceRoom telemetry is Windows-only for now.

## Everyday commands

| What | Command |
|---|---|
| Run the app | `cargo run -p setup-assistant` |
| All tests | `cargo test --workspace` |
| Format | `cargo fmt --all` |
| Lint (CI fails on warnings) | `cargo clippy --workspace --all-targets -- -D warnings` |
| UI in a browser, real engine, demo data | `cargo run -p setup-core --example ui_preview` → http://127.0.0.1:1420 |
| Replay a recorded run | `cargo run -p setup-core --example replay -- <file.csv> [understeer_deg_per_g]` |

In the app, **Live telemetry → Try demo lap** plays synthetic laps with known problems.

## Project layout

```
crates/setup-core/           game-agnostic core (no game, no UI code)
  src/model.rs               Symptom, Suggestion, History, ...
  src/rules.rs               rule book engine: matching, merging, conflicts, history
  src/telemetry.rs           Sample + Analyzer: detects problems from telemetry
  src/track.rs               learned track map: corners, crests, compressions
  src/live.rs                background polling of a Source, runs, recording
  src/recording.rs           CSV recordings
  src/demo.rs                synthetic laps and demo track (tests + demo mode)
  rules/gt.json              setup knowledge
  examples/ui_preview.rs     serves app/ui in a browser
  examples/replay.rs         replays recordings through the analyzer
crates/r3e-telemetry/        RaceRoom adapter ($R3E shared memory)
app/src-tauri/               Tauri shell: commands, overlay window, icons
app/ui/                      HTML/CSS/JS frontend (main window + overlay)
docs/                        logo, screenshot, how-it-works
```

## Common contributions

### Improve setup advice

Edit `crates/setup-core/rules/gt.json`. Each rule maps a behaviour (+ optional phases, corner
speed, aero-only, drivetrains) to an ordered list of changes. Write the `why` in plain
language: drivers read it. Run `cargo test -p setup-core`: unknown fields and broken
references fail `builtin_rules_are_valid`.

### Tune detection

Thresholds live in `Thresholds` in `crates/setup-core/src/telemetry.rs`. Please back changes
with recordings: drive, find the CSV in `%APPDATA%\dev.simracing.setupassistant\recordings`,
and compare `replay` output before and after. Say in the PR which car/track you tested and
what you felt in the car.

### Add a game

1. New crate `crates/<game>-telemetry`.
2. Convert the game's data to `setup_core::telemetry::Sample` (SI units; see the field docs).
   Sign conventions don't matter (ADR-6), units do.
3. Implement `setup_core::live::Source`: `poll()`, and `identity()` with the game, API version,
   car, track, session type and which optional `Channels` the game provides. Missing channels
   are treated as unknown, so the detectors that need them stay quiet and the driver is told.
4. Wire it up in `app/src-tauri/src/main.rs`.
5. Add a layout/conversion test like `crates/r3e-telemetry`'s offset test.

### UI

Plain HTML/CSS/JS in `app/ui`, no build step. Use `ui_preview` for fast iteration. A new Tauri
command needs a matching branch in `examples/ui_preview.rs`.

## Tests

- Detection tests use the synthetic laps in `demo.rs` and assert *exact* detections, so false
  positives fail too. When you add a detector, add a demo case and a test.
- Keep `cargo fmt` and `cargo clippy -D warnings` clean; CI checks both on Linux and Windows.

## Pull requests

- One topic per PR, with a short description of what and why.
- A design change (new direction, replacing an approach)? Add or update a record in
  [ADR.md](ADR.md) in the same PR.
- User-facing behaviour changed? Update README or `docs/how-it-works.md`.
- Finishes a roadmap item or a listed bug? Set its Status in
  [docs/roadmap.md](docs/roadmap.md) in the same PR.
- By contributing you agree that your contribution is licensed under the project's
  [MIT license](LICENSE).

## Working with AI coding agents

Agents are welcome contributors too. [AGENTS.md](AGENTS.md) has the short version of the
conventions above; point your agent at it and at ADR.md. With Claude Code, the `implement`
skill (`.claude/skills/implement/`) runs one background worker per issue, each in its own git
worktree and PR, and reviews and merges the PRs with you.
