# Architecture Decision Records

The important decisions behind Setup Assistant, in the order they were made, with the reasons
and what they cost. Read this before changing direction on any of them. If you change one,
don't edit history: add a new record that supersedes the old one and update the old one's status.

Format: **Context** (the situation), **Decision** (what we do), **Consequences** (what follows,
good and bad). Statuses: *Accepted*, *Superseded by ADR-n*, *Proposed*.

| # | Decision | Status |
|---|---|---|
| 1 | [Suggest changes, don't write RaceRoom setup files](#adr-1-suggest-changes-dont-write-raceroom-setup-files) | Accepted |
| 2 | [Rust + Tauri 2 with a plain HTML/CSS/JS frontend](#adr-2-rust--tauri-2-with-a-plain-htmlcssjs-frontend) | Accepted |
| 3 | [Game-agnostic core, games behind adapters](#adr-3-game-agnostic-core-games-behind-adapters) | Accepted |
| 4 | [Setup knowledge lives in a JSON rule book](#adr-4-setup-knowledge-lives-in-a-json-rule-book) | Accepted |
| 5 | [RaceRoom telemetry through the official shared memory](#adr-5-raceroom-telemetry-through-the-official-shared-memory) | Accepted |
| 6 | [Don't trust sign conventions; learn them](#adr-6-dont-trust-sign-conventions-learn-them) | Accepted |
| 7 | [Balance relative to the car's own medium-speed cornering](#adr-7-balance-relative-to-the-cars-own-medium-speed-cornering) | Superseded by ADR-8 |
| 8 | [Balance as extra steering per g beyond the learned geometry](#adr-8-balance-as-extra-steering-per-g-beyond-the-learned-geometry) | Accepted |
| 9 | [Work in runs; suggestions know the history](#adr-9-work-in-runs-suggestions-know-the-history) | Accepted |
| 10 | [The UI owns the session; the backend is told](#adr-10-the-ui-owns-the-session-the-backend-is-told) | Accepted |
| 11 | [Understeer under braking is its own problem](#adr-11-understeer-under-braking-is-its-own-problem) | Accepted |
| 12 | [Ignore crests; remember where problems happen](#adr-12-ignore-crests-remember-where-problems-happen) | Accepted |
| 13 | [Learn a track map from the driver's laps](#adr-13-learn-a-track-map-from-the-drivers-laps) | Accepted |
| 14 | [Record every run; tune against real laps](#adr-14-record-every-run-tune-against-real-laps) | Accepted |
| 15 | [A synthetic demo lap is the test fixture](#adr-15-a-synthetic-demo-lap-is-the-test-fixture) | Accepted |
| 16 | [In-game overlay as a second always-on-top window](#adr-16-in-game-overlay-as-a-second-always-on-top-window) | Accepted |
| 17 | [Develop the UI in a normal browser](#adr-17-develop-the-ui-in-a-normal-browser) | Accepted |
| 18 | [Treat bad or missing data as unknown, never as a reading](#adr-18-treat-bad-or-missing-data-as-unknown-never-as-a-reading) | Accepted |
| 19 | [Sources describe themselves; missing channels are masked](#adr-19-sources-describe-themselves-missing-channels-are-masked) | Accepted |

---

## ADR-1: Suggest changes, don't write RaceRoom setup files

**Context.** The original idea was to adjust a setup in the app and write it straight into the
game's folder (`Documents\My Games\SimBin\RaceRoom Racing Experience\UserData\CarSetups\<track>\<layout>\*.svm`).
Comparing two saved setups that differ by one wing click showed the files are encrypted: a
16-byte header (magic `0F B1 FF FF`, version 4, plaintext length, 4 zero bytes), then an 8-byte
block cipher in ECB mode with one fixed key for all files (identical 8-byte blocks repeat, only
on 8-byte boundaries). Reading them would mean extracting the key from the game executable,
which ships with an anti-tamper layer (`protect.exe` / `protect.dll`).

**Decision.** For RaceRoom the app only *suggests* changes ("front ARB, 2 clicks softer"); the
driver applies them in the garage. We don't extract keys or work around the protection.

**Consequences.**
- The core value (detect problem → explain fix) doesn't depend on file access.
- No risk to the user's game account, no terms-of-service problems, no key in a public repo.
- Changes are in "clicks", not real values (see roadmap: per-car knowledge).
- Writing setups stays possible for games with plain-text setups (rFactor 2 / LMU `.svm`, ACC
  JSON, AC `.ini`), behind the same adapter boundary (ADR-3). Asking KW Studios (RaceRoom) for an
  official way is the other route.

## ADR-2: Rust + Tauri 2 with a plain HTML/CSS/JS frontend

**Context.** The owner wanted Rust for the backend, a Rust-based shell with an HTML/CSS UI that
can look good, and a path to Linux.

**Decision.** Tauri 2. The frontend is plain HTML/CSS/JS in `app/ui/`: no framework, no
bundler, no npm. The Rust side exposes a few commands (`catalog`, `suggest`, `live_status`, ...).

**Consequences.**
- Small binary, native WebView2 on Windows, Linux possible later.
- Anyone can edit the UI without a JS toolchain. If the UI grows a lot, a framework
  (Svelte/Solid) can be introduced later; nothing in the core depends on the UI.
- `cargo run -p setup-assistant` is the whole build; `tauri-cli` is optional.

## ADR-3: Game-agnostic core, games behind adapters

**Context.** RaceRoom first, other games later.

**Decision.** Three layers:
- `crates/setup-core`: models, rule book, telemetry analysis, track map, live session. Knows
  no game and no UI.
- Game adapters, e.g. `crates/r3e-telemetry`: turn game data into `setup_core::telemetry::Sample`
  and implement `setup_core::live::Source` (`poll()`, `track()`).
- `app/`: the Tauri shell and UI.

**Consequences.** A new game is a new crate implementing `Source`. All detection, rules and
history work unchanged. Game-specific quirks (units, sign conventions, tyre temperature order)
are handled in the adapter.

## ADR-4: Setup knowledge lives in a JSON rule book

**Context.** Setup advice should be tunable by people who know cars, not only by Rust developers.

**Decision.** `crates/setup-core/rules/gt.json` holds parameters, behaviours (problems) and
rules: problem + phase + corner speed (+ aero-only, drivetrain) → ordered changes with clicks,
a cap and a plain-language reason. The engine scales clicks by severity, merges changes to the
same parameter, and reports conflicts when two problems pull a parameter in opposite directions.
Unknown JSON fields are rejected and a test validates the whole book.

**Consequences.**
- Advice changes are data changes with a test as a safety net.
- The current book is generic GT knowledge, not per car. Per-car books are a later step.

## ADR-5: RaceRoom telemetry through the official shared memory

**Context.** Automatic detection needs live data. RaceRoom publishes `$R3E` shared memory,
documented in Sector3's `r3e-api` repo (`r3e.h`, version 3.6 at the time).

**Decision.** `crates/r3e-telemetry/src/layout.rs` mirrors `r3e.h` as `#[repr(C, packed)]` up to
`num_cars` (the per-driver array after it isn't needed). A test checks 22 field offsets against
offsets computed from the header with `pack(1)`. The reader refuses to read when the major
version isn't 3, and reconnects by itself when the game starts.

**Consequences.**
- No undocumented memory reading, and a layout mismatch fails a test instead of producing garbage.
- If Sector3 changes the struct, the mirror and the offset test have to be updated together.
- Tyre temperatures come as left/centre/right; we assume "left" is the car's left (outside edge
  on left tyres). Not yet confirmed on track.

## ADR-6: Don't trust sign conventions; learn them

**Context.** The header documents some axes (+X left, +Y up, +Z back for some vectors) but not
the sign of steering input vs. yaw rate, or whether vertical acceleration includes gravity.

**Decision.** The analyzer learns relationships from driving instead of hard-coding them: the
sign between steering and yaw from normal cornering, and the "level road" vertical reading
from straights (with or without gravity, either sign).

**Consequences.** Adapters stay thin and other games work without sign fixes. A test drives the
demo lap with flipped signs and expects the same detections. The cost is a short calibration at
the start of each run (see ADR-8).

## ADR-7: Balance relative to the car's own medium-speed cornering

*Superseded by ADR-8.*

**Context.** Detecting understeer needs a reference for how much the car should turn.

**Decision (first version).** Learn the car's yaw-per-steering in medium-g corners (0.2 to 0.9 g)
and flag understeer when a corner needed much more steering than that (ratio < 0.7).

**Why it was replaced.** In the first real session the driver fixed oversteer, then the car
understeered everywhere, and the app didn't notice. The reference was learned from the same
understeering corners, so an all-round understeering car looked "normal". Only understeer that
was *worse at the limit than at medium g* showed up.

## ADR-8: Balance as extra steering per g beyond the learned geometry

**Context.** ADR-7's reference absorbed the problem it was meant to detect.

**Decision.** In gentle curves (0.08 to 0.3 g) every car follows its steering geometry
regardless of setup, so the analyzer learns the geometric yaw-per-steering there. In real
corners it measures the *extra* steering needed beyond geometry, in steering-wheel degrees per
g of cornering (an understeer gradient). Positive = understeer, negative = oversteer. Above
30°/g for 0.35 s counts as understeer. Oversteer is detected from countersteer plus body slip.

**Consequences.**
- Absolute and independent of corner radius, so slow and fast corners compare, and so do runs.
- The geometry learned on two real runs agreed within 5% (0.090 vs 0.094).
- Needs some gentle curves before it can judge; lap 1 is partly calibration.
- 30°/g is a first guess. It needs tuning per car class (ADR-14).

**Note (2026-11-11).** The learned geometry is not a constant: on real recordings the gain
falls by half between 50 and 250 km/h, because in fast gentle curves the understeer term is
larger than the geometric one. The median is dominated by fast curves, so slow corners read
20–55°/g towards oversteer. The "agreed within 5%" above was agreement between two equally
biased medians. Fix planned in #40 (geometry from the zero-speed intercept), which will
supersede the geometry part of this ADR. See FINDINGS.MD, B1.

## ADR-9: Work in runs; suggestions know the history

**Context.** Fixing one problem often creates the opposite one. Naive suggestions then undo
the earlier fix (e.g. stiffen the rear bar that was just softened to cure oversteer).

**Decision.** A session is a series of runs. The driver ticks the changes they made and starts
the next run; those become `AppliedChange`s and the run's problems become "fixed".
`RuleBook::suggest_with_history`:
- ranks a change that reverses earlier changes far down, caps it at half the clicks, and says
  what it undoes;
- warns when a change is argued against by a fixed problem's rules ("could bring back ...");
- notes when the same direction was already changed.

A fixed problem that is detected or added again stops counting as fixed. When *every*
suggestion risks a fix, the UI says the car is probably near its balance point.

**Consequences.** The app guides towards changes that don't trade one problem for another. It
depends on the driver ticking what they actually changed, so the UI asks when nothing is ticked.

## ADR-10: The UI owns the session; the backend is told

**Context.** "Next run" seemed to do nothing in testing: the run counter lived in two places and
got out of sync after an app restart, and nothing was carried over when nothing was ticked.

**Decision.** The UI holds the session (problems, ticked changes, history, run number) in
`localStorage`, so it survives restarts, and tells the backend (`live_set_run`,
`live_next_run { run }`). Next run gives visible feedback, asks when nothing is ticked, and also
carries over problems that telemetry detected but the driver didn't add.

**Consequences.** One source of truth. History is per machine and not shared. Moving it to a
file in the app data folder is easy if needed.

## ADR-11: Understeer under braking is its own problem

**Context.** Real recordings showed "entry understeer" growing with brake pressure: about
+1°/g off the brakes, +7 at 5-20 %, +10 at 20-50 %, +28 above 50 %. With the front wheels
rolling it was +1°/g; with them slipping (ABS working) +17°/g. That's mostly the front tyres
spending grip on braking, not a chassis problem, and it pointed drivers at dampers and bars.

**Decision.** When the front wheels turn more than 4 % slower than the car under braking,
understeer counts as **Understeer under braking**, with its own rules (brake bias, ABS, coast
diff, brake pressure, plus a note about releasing the brake). It also gets its own row in the
balance table. Normal entry understeer only counts with the fronts rolling.

**Consequences.** On the real recording, chassis entry understeer went from 38 detections to 2,
and 33 became braking understeer. Its main fix (bias rearward) often conflicts with fixes for
unstable braking, which the history logic (ADR-9) makes visible.

## ADR-12: Ignore crests; remember where problems happen

**Context.** Over crests the tyres lose load whatever the setup, and some problems only happen
at one spot on a track. Neither is a setup problem.

**Decision.**
- When vertical load drops below 75 % of level road (ADR-6), the car counts as light for 0.3 s
  and balance isn't judged.
- Every event stores its lap distance. Detections list their places; a problem with at least
  3 events, 70 % of them within 80 m, is flagged "one place only".

**Consequences.** Fewer false alarms on hilly tracks; the driver sees *where*. Needs lap
distance and vertical acceleration from the game (recordings before this change don't have them).

## ADR-13: Learn a track map from the driver's laps

**Context.** On Mount Panorama the bottoming detector fired 47 times in 31 minutes. Hilly tracks
need context: bottoming in one big compression calls for different advice than bottoming everywhere.

**Decision.** No external track database. The app learns each track (per game, track and layout)
from the driver's own laps: every 10 m the average elevation (car position height), speed and
cornering force. From that: corners T1, T2, ... (above 0.4 g; the driver can name them), crests
and compressions (road curvature at the driver's speed adds or removes at least 0.3 g). Maps
are saved in the app data folder and keep the places of problems from the last 12 runs.

Detections are then described by place ("T7 The Dipper · compression"). Bottoming that mostly
happens in compressions becomes **Bottoming in compressions** with its own advice. A problem at
the same spot in each recent run, whatever was changed, is flagged as likely the track.

**Consequences.**
- Works for any track the driver drives, including mods; maps improve with every lap.
- Needs about one full lap before places have names.
- Thresholds (0.4 g corner, 0.3 g features, 8 mm bottoming) are guesses to tune on real tracks.
- General track advice ("low drag for Conrod Straight") can't be learned; that would need
  hand-written track notes.

## ADR-14: Record every run; tune against real laps

**Context.** Every detector threshold started as a guess, and the only way to know is real data.

**Decision.** Real driving is recorded per run as CSV in
`%APPDATA%\dev.simracing.setupassistant\recordings` (~1.5 MB per minute). The `replay` example
runs a recording through the analyzer, optionally with another threshold. New columns are
appended at the end so old recordings still load.

**Consequences.** Threshold changes can be justified with data (ADR-11 came from this). The
recordings are personal and stay local. Ask drivers for them when tuning.

## ADR-15: A synthetic demo lap is the test fixture

**Context.** Detection needs tests that don't need the game, and users need a way to try the
app without driving.

**Decision.** `setup_core::demo` generates laps with built-in, known problems and a known track
(5 corners, a crest, a compression). Variants: `Mixed` (one of each kind, used by most tests),
`Loose` (oversteer) and `Pushing` (understeer everywhere: the regression case for ADR-7). The
same generator powers "Try demo lap" in the app, switching Loose → Pushing on "Next run".

**Consequences.** Tests assert exact detections ("these 6 and nothing else"), which catches
false positives too. The demo is idealised physics; it proves the logic, not the thresholds.

## ADR-16: In-game overlay as a second always-on-top window

**Context.** With one screen the driver can't see the app while driving.

**Decision.** A small transparent, undecorated, always-on-top Tauri window (`overlay.html`) with
the balance needle, corner phase and the latest event. It can be locked to click-through.

**Consequences.** Simple and game-independent, but it only shows over the game in borderless or
windowed mode. Exclusive fullscreen would need a real overlay hook, which we avoid.

## ADR-17: Develop the UI in a normal browser

**Context.** Iterating on the UI inside the desktop app is slow, and automated checks of the app
window are hard.

**Decision.** The `ui_preview` example serves `app/ui` over HTTP with the real engine behind
`POST /invoke/<command>`; `main.js` falls back to that when Tauri isn't there. Live telemetry
runs in demo mode there. For checks inside the real app window, WebView2's remote debugging
port can be enabled with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`.

**Consequences.** Fast UI work and browser-based checks (also for AI agents). Every new Tauri
command needs a matching branch in `ui_preview.rs`.

## ADR-18: Treat bad or missing data as unknown, never as a reading

**Context.** A review of the code (2026-11-07) found places where missing or broken data
quietly turned into measurements or stopped analysis: a malformed CSV value shifted the
following columns; a missing ride height defaulted to 0 m, which counts as bottoming;
RaceRoom's simulation clock restarts at 0 with a new session, and the analyzer then dropped
every frame until the clock passed its old value; equally strong opposite votes for a
parameter produced an arbitrary "increase"; the same spot just before and after the start line
counted as two places; and the app called a recurring spot "likely the track, whatever you
changed" although it never compares clean runs or changes.

**Decision.**
- `Sample::default()` marks ride height and lap distance as unknown (< 0), not 0.
- A clock that goes backwards starts a new epoch: in-progress events are dropped, everything
  learned about the car is kept. Equal timestamps are still skipped as duplicates.
- Recordings are read column by column (`recording::read_checked`). A row with a missing,
  malformed or non-finite value is rejected and counted, never shifted. Supported layouts:
  34, 36 and 37 columns (38 since #37, with steering lock); extra columns are ignored;
  elevation may be empty.
- A parameter with equally strong opposite votes is left out of the suggestions: there is no
  direction to recommend.
- With the lap length known, places wrap at the start line, in the analyzer and in run
  comparison.
- The UI says a problem "came back at this spot" instead of claiming it's the track. This
  softens the wording of ADR-12 and ADR-13; the detection logic is unchanged.

**Consequences.** No detection thresholds changed. Old recordings still load. A recurring spot
stays a hint until clean runs, car identity and changes are compared.

## ADR-19: Sources describe themselves; missing channels are masked

**Context.** History is going to be scoped to car + track (roadmap M2), and a second game
(LMU) may not provide every channel RaceRoom does. The core only knew the track.

**Decision.** `Source::identity()` returns a `SourceIdentity`: game, telemetry API version,
car (stable game-prefixed IDs, names when the game gives them), track, session type and
`Channels`, the optional channels the source fills in. Required channels (speed, pedals,
steering, yaw, lateral acceleration and velocity, wheel speeds) aren't listed. The analyzer and
track map see missing channels as their "unknown" value (`Channels::mask`), so detectors that
need them stay quiet without per-detector checks. `LiveStatus` carries the identity and a
plain-language list of what is unavailable. RaceRoom reports car and class IDs only; names
come later with the class profiles (roadmap M5.1).

**Consequences.** Adapters must declare their channels honestly; per-frame values RaceRoom
marks as -1 turn the channel off. Recordings keep the raw values.

## ADR-20: Bottoming needs both wheels of an axle; kerbs aren't reported

**Context.** Kerb strikes were counted as bottoming (#42): Red Bull Ring 2026-11-10 run 3
flagged it twice at 3640 m from the front-left wheel at −12 mm. Bottoming advice (ride height,
springs) is wrong for a kerb. In the four local recordings (AMG GT3, Hockenheim and Red Bull
Ring) every reading below 8 mm is on one wheel or one side of the car; no sample has both wheels
of an axle low at once. Their vertical jolts range from about 3 to 42 m/s², while road noise reaches
4 m/s² at the 99th percentile and 8 m/s² at the 99.9th, so a jolt threshold can't separate
kerbs from other low readings.

**Decision.** Bottoming counts only when both wheels of the front or the rear axle are below
the threshold at the same time (still at least 0.05 s, above 25 m/s). One wheel low, or one
side of the car, is a kerb strike: not recorded and not reported, since kerbs aren't a setup
problem by default. No new threshold; the 8 mm threshold is unchanged.

**Consequences.** All low readings in the recordings so far stop counting, including the hard
negative ones #50 starts counting. A splitter scraping on one corner only (heavy roll) is no
longer reported; if real recordings show that, it needs its own rule. Whether real axle-wide
scraping is caught is still to be checked on track (sim-testing T5).
