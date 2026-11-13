# Roadmap

Where Setup Assistant is going, in order. Each item is a GitHub issue; milestones group them.
Background and the original analysis are in [ideas.md](ideas.md). Design decisions land in
[ADR.md](../ADR.md) as they're made. The review of 2026-11-11 ([FINDINGS.MD](../FINDINGS.MD))
reordered the plan: see "Order of work" below.

**Audience:** for now the main developer and a few contributors building from source. A public
alpha (installer for other RaceRoom drivers) follows once history and fix checking are
trustworthy (see the alpha gate below).

## Status

The **Status** column of every table below says what is on `main`: **Done (#PR)**, **Partly
done (#PR: what)**, **In progress** (driver tasks only) or **Open**. Work in review isn't listed
here: `gh pr list` shows it, and each PR names its issue with `Fixes #n`.

- The PR that finishes an item or bug sets its row to Done in the same PR, so the status is
  right the moment it merges. A PR that finishes part of an item says which part.
- A PR opened without that update: set the row when merging it.
- **Now** in "Order of work" marks the current step; move it when a step is complete.
- New scope gets a row (and an issue) before work starts.

**Done before the roadmap** ([PR #2](https://github.com/sitzriese89/setup-assistant/pull/2)):
game clock resets, unknown ride height, strict CSV import, tied votes, places across the start
line, Next run / suggestion guards, softer recurrence wording.

**Bugs from the 2026-11-11 review** (not roadmap items, one PR each):

| Bug | Issue | Status |
|---|---|---|
| ABS pulsing counts chassis and braking understeer for the same moment | [#45][45] | Done (#53) |
| Toggling the demo truncates the run's recording | [#46][46] | Done (#56) |
| Source line is empty or stale while not driving | [#47][47] | Done (#57) |
| Removed problems leave their ticked changes in Next run | [#48][48] | Done (#58) |
| Power oversteer after the corner is filed as mid-corner | [#49][49] | Done (#54) |
| Negative ride height on hard strikes isn't counted as bottoming (after M3.10) | [#50][50] | Done (#55) |
| Hardening: saved track maps, telemetry-thread panics, vertical median | [#51][51] | Done (#59) |

## Decisions behind this plan

| Topic | Decision |
|---|---|
| Who owns the session | The backend: versioned JSON per session in the app data folder. The UI is a view. Supersedes ADR-10 ([M2.1][m2.1]). |
| History scope | Per **car + track/layout**. Each context has its own sessions; returning resumes the latest; New session starts fresh. The demo is its own context. |
| Validate before verifying | Fix checking (M2.4) judges "resolved" from detector events, so the detector is validated first: geometry fit with road-wheel units, phase hysteresis, the episode record and a minimal `evaluate` with replay parity come before M2.4. A missed detection would otherwise read as a fix. |
| When a problem is fixed | States: open → change tried → awaiting check → improved / resolved / still there. Telemetry judges after **3 clean passes** per place (same phase, similar speed): no events = resolved, ≥ 50% fewer per pass = improved. Problems added by hand wait for the driver. The driver can override any status. Revises ADR-9. |
| Logging changes | Click stepper on "Tried" (actual clicks), "I changed something else", editable history. |
| Setup values | Asked only for settings the app suggests, then tracked from logged clicks. |
| Car knowledge | Class profiles in JSON (GT3, GT4, TCR …) from RaceRoom's class ID, per-car overrides over time. |
| Track maps | Geometry and corner names shared per track; speed/load per car class; names anchored to positions. |
| Recording context | A JSON sidecar next to every recording (identity, channels, version, thresholds, resets), written from M1 on so recordings collected now stay usable. |
| Real recordings | Gzipped excerpts (1–3 MB) committed under `fixtures/real/`, with notes, clean laps and a holdout set. |
| Driver feedback | Note and quick tags per run, saved next to the recording. |
| Evidence UI | Expandable evidence line per detection, calibration progress bar. |
| UI tests | Playwright in `tests/ui` only; the app stays bundler-free. Not before the alpha: #8 gives most of the safety without a JS toolchain. |
| Recordings retention | Keep everything; warn above 2 GB. |
| Run comparison | Per-corner before/after table. |
| Accessibility | Keyboard operation, narrow layout, reduced motion. |
| Releases | Unsigned NSIS installer on GitHub Releases with checksum; revisit signing before a wider release. |
| Second game | Le Mans Ultimate, after M1 context work. Writing its setup files needs its own ADR. |
| Deferred | Linux/Proton bridge, in-app replay timeline. |

## Order of work

1. **M1**, except the items marked *after the alpha*. **Now** (the review bugs above are
   all done).
2. **Validation slice of M3:** M3.7 + M3.8 as one change with one ADR (both change what the
   understeer threshold means, so it is re-checked once), M3.9, M3.3 (the episode record), and
   a minimal M3.2: `evaluate` over local recordings with replay parity, before the CI baseline.
3. **M2**, with M2.4 on the validated detector.
4. **The rest of M3**, then the alpha gate.

Items marked *after the alpha* stay open but don't block it.

## M1 · Data you can trust

Missing, stale or broken data is visible and never becomes a measurement.

| Item | Issue | Status |
|---|---|---|
| M1.1 Source reports game, API version, car, track and available channels | [#3][3] | Done (#34) |
| M1.2 Show stale telemetry and unsupported game versions | [#4][4] | Open |
| M1.3 Crash-safe saving and visible save/recording health | [#5][5] | Open |
| M1.4 Clean worker shutdown and unique recording IDs | [#6][6] | Open |
| M1.5 Measure live polling cost with both windows open · *after the alpha* (recordings show a steady 10 ms interval) | [#7][7] | Open |
| M1.6 Command parity test between desktop app and ui_preview | [#8][8] | Open |
| M1.7 Playwright smoke tests for the main workflow · *after the alpha* | [#9][9] | Open |
| M1.8 Metadata sidecar with every recording | [#43][43] | Open |
| M1.9 Ask for a new session when the car or track changes (until M2.2) | [#44][44] | Open |

## M2 · Sessions and context

History belongs to a car and track, records what was actually changed, and only calls a
problem fixed with evidence.

| Item | Issue | Depends on | Status |
|---|---|---|---|
| M2.1 ADR: backend owns sessions | [#10][10] | | Open |
| M2.2 Backend session store per car + track | [#11][11] | M1.1, M1.3, M2.1 | Open |
| M2.3 Actual clicks, manual changes, editable history | [#12][12] | M2.2 | Open |
| M2.4 Problem lifecycle with per-place verification (ADR) | [#13][13] | M2.2, validation slice (M3.2 minimal, M3.3, M3.7, M3.8, M3.9) | Open |
| M2.5 Recurring spots need comparable driven runs · *after the alpha* | [#14][14] | M2.2, M2.4 | Open |
| M2.6 Track maps: shared geometry, per-class data, anchored names · *only anchored names before the alpha* | [#15][15] | M1.1 | Open |
| M2.7 Run notes and quick tags | [#16][16] | M2.2 | Open |

## M3 · Evidence → alpha gate

Detectors are checked against real laps, and drivers can see why something was flagged.

| Item | Issue | Depends on | Status |
|---|---|---|---|
| M3.1 `excerpt` tool and real-recording fixtures | [#17][17] | M1.8 | Open |
| M3.2 `evaluate` tool with baseline check in CI (a minimal version with replay parity first) | [#18][18] | M3.1 for the CI baseline | Open |
| M3.3 Detection evidence and calibration progress (episode record) | [#19][19] | | Open |
| M3.4 Severity per pass vs per minute | [#20][20] | M2.4, M3.2, M3.9 | Open |
| M3.5 Rule coverage, no-aero fallback, ranking diagnostics | [#21][21] | | Open |
| M3.6 Collect recordings on 3 tracks, incl. known-answer runs (driver task) | [#22][22] | | In progress: Hockenheim and Red Bull Ring recorded ([sim-testing.md](sim-testing.md)) |
| M3.7 Understeer in road-wheel degrees, not steering-wheel degrees | [#35][35] | M1.1; together with M3.8 | Partly done (#37: steering lock recorded, road-wheel balance in `replay`) |
| M3.8 Learn the steering geometry from the zero-speed intercept | [#40][40] | together with M3.7 | Open |
| M3.9 Phase hysteresis: one episode per phase | [#41][41] | | Open |
| M3.10 Tell kerb strikes apart from bottoming | [#42][42] | | Done (#61) |

**Alpha gate:** M1–M3 done (except items marked *after the alpha*), and on real recordings
from at least 3 tracks:

- `evaluate` shows no unexpected false detections. Report the rule-of-three bound with it
  (no false alert in *T* hours still allows about 3/*T* per hour).
- One known-answer run per detector moves its detection the right way: a deliberate setup
  change with a known direction, driven A/B/A (e.g. front anti-roll bar to maximum for
  understeer). No false detections alone doesn't show the detector can see a problem.
- The fitted steering geometry matches the car: speed → 0 gives lock / wheelbase (about ±10%).

## M4 · Public alpha

| Item | Issue | Status |
|---|---|---|
| M4.1 Minimum Rust version and version consistency check | [#23][23] | Open |
| M4.2 Release workflow: unsigned NSIS installer, checksum, changelog | [#24][24] | Open |
| M4.3 Keyboard access and narrow layout | [#25][25] | Open |
| M4.4 Recording list: size, open folder, delete/export, 2 GB warning | [#26][26] | Open |

## M5 · Better tuning

| Item | Issue | Depends on | Status |
|---|---|---|---|
| M5.1 Car class profiles and value-aware advice | [#27][27] | M1.1, M2.3 | Open |
| M5.2 Driven-wheel wheelspin analysis | [#28][28] | | Open |
| M5.3 Per-corner run comparison table | [#29][29] | M2.2, M2.4 | Open |
| M5.4 Confirm RaceRoom tyre channel layout on a real stint | [#30][30] | | Open |
| M5.5 Context-aware tyre analysis | [#31][31] | M5.1, M5.4 | Open |

## M6 · Le Mans Ultimate

| Item | Issue | Depends on | Status |
|---|---|---|---|
| M6.1 Le Mans Ultimate telemetry adapter | [#32][32] | M1.1 | Open |

## How to work through it

- One issue per PR; each PR follows [CONTRIBUTING.md](../CONTRIBUTING.md) and the ground rules in
  [AGENTS.md](../AGENTS.md).
- Items without dependencies inside a milestone can run in parallel.
- Threshold changes only with `evaluate` output once M3.2 exists; until then, with `replay`
  output before and after on the local recordings.
- Detector changes get a demo case that fails without the change. The demo is idealized (its
  gentle curves had no understeer, its throttle never hovers at a cut-off), so make the case as
  messy as the real data that showed the problem.
- Keep driving between PRs ([sim-testing.md](sim-testing.md)): driving found more real
  problems than code review. Attach replay output to each session entry.
- When a decision here changes, update this file and add or supersede an ADR.

[m2.1]: https://github.com/sitzriese89/setup-assistant/issues/10
[3]: https://github.com/sitzriese89/setup-assistant/issues/3
[4]: https://github.com/sitzriese89/setup-assistant/issues/4
[5]: https://github.com/sitzriese89/setup-assistant/issues/5
[6]: https://github.com/sitzriese89/setup-assistant/issues/6
[7]: https://github.com/sitzriese89/setup-assistant/issues/7
[8]: https://github.com/sitzriese89/setup-assistant/issues/8
[9]: https://github.com/sitzriese89/setup-assistant/issues/9
[10]: https://github.com/sitzriese89/setup-assistant/issues/10
[11]: https://github.com/sitzriese89/setup-assistant/issues/11
[12]: https://github.com/sitzriese89/setup-assistant/issues/12
[13]: https://github.com/sitzriese89/setup-assistant/issues/13
[14]: https://github.com/sitzriese89/setup-assistant/issues/14
[15]: https://github.com/sitzriese89/setup-assistant/issues/15
[16]: https://github.com/sitzriese89/setup-assistant/issues/16
[17]: https://github.com/sitzriese89/setup-assistant/issues/17
[18]: https://github.com/sitzriese89/setup-assistant/issues/18
[19]: https://github.com/sitzriese89/setup-assistant/issues/19
[20]: https://github.com/sitzriese89/setup-assistant/issues/20
[21]: https://github.com/sitzriese89/setup-assistant/issues/21
[22]: https://github.com/sitzriese89/setup-assistant/issues/22
[23]: https://github.com/sitzriese89/setup-assistant/issues/23
[24]: https://github.com/sitzriese89/setup-assistant/issues/24
[25]: https://github.com/sitzriese89/setup-assistant/issues/25
[26]: https://github.com/sitzriese89/setup-assistant/issues/26
[27]: https://github.com/sitzriese89/setup-assistant/issues/27
[28]: https://github.com/sitzriese89/setup-assistant/issues/28
[29]: https://github.com/sitzriese89/setup-assistant/issues/29
[30]: https://github.com/sitzriese89/setup-assistant/issues/30
[31]: https://github.com/sitzriese89/setup-assistant/issues/31
[32]: https://github.com/sitzriese89/setup-assistant/issues/32
[35]: https://github.com/sitzriese89/setup-assistant/issues/35
[40]: https://github.com/sitzriese89/setup-assistant/issues/40
[41]: https://github.com/sitzriese89/setup-assistant/issues/41
[42]: https://github.com/sitzriese89/setup-assistant/issues/42
[43]: https://github.com/sitzriese89/setup-assistant/issues/43
[44]: https://github.com/sitzriese89/setup-assistant/issues/44
[45]: https://github.com/sitzriese89/setup-assistant/issues/45
[46]: https://github.com/sitzriese89/setup-assistant/issues/46
[47]: https://github.com/sitzriese89/setup-assistant/issues/47
[48]: https://github.com/sitzriese89/setup-assistant/issues/48
[49]: https://github.com/sitzriese89/setup-assistant/issues/49
[50]: https://github.com/sitzriese89/setup-assistant/issues/50
[51]: https://github.com/sitzriese89/setup-assistant/issues/51
