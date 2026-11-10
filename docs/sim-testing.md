# Sim testing log

What still needs checking on track, what has been checked, and how each driving session went.
Kept up to date by the `sim-test` skill (`.claude/skills/sim-test/SKILL.md`). Recordings stay in
`%APPDATA%\dev.simracing.setupassistant\recordings` and are not committed.

## To test

| # | Check | From | How | Pass when |
|---|---|---|---|---|
| T1 | Source line under the connection status | #34 | Look at the Live tab in the pits or between runs (only the overlay is visible while driving) | Shows "RaceRoom 3.6 · Car … (class …) · <track> · <session>" with the right track and session type |
| T2 | "Not available from this game" line | #34 | Look at the Live tab in the pits, in menus and right after leaving the pits | Absent with full data, or steady (no flicker) when shown |
| T3 | Game clock reset | #2 (ADR-18) | Restart a session or return to pits mid-run, then keep driving | Detection carries on afterwards; no burst of false problems |
| T4 | Places near the start line | #2 | Look at places listed for problems near the line | Events just before and after the line show as one place |
| T5 | Bottoming detection | – | Drive Bathurst (the Esses, Forrest's Elbow compression) with a low car | Bottoming only where you feel/hear the car scrape; 8 mm threshold is still unverified. Note: Red Bull Ring 2026-11-10 run 3 flagged bottoming 2x at 3640 m from a kerb strike (front-left ride height −12 mm, vertical jolt 7–18 m/s²). Driver: a driving mistake, not a setup problem; no issue filed. Watch whether kerbs keep showing up as bottoming |
| T6 | Steering ratio of the 2026 Lamborghini | #35 | Drive the Lambo; Claude replays the recording | Ratio between about 10:1 and 20:1. With the AMG at 19.6:1, this shows how much the steering-wheel threshold differs between cars |
| T7 | Slow-corner balance reads strong oversteer | 2026-11-08 session | Claude replays; compare with how slow corners feel | Explained: either the car really rotates there, or a measurement problem in tight corners (then file an issue). Hockenheim showed −27 to −40°/g in every slice with few oversteer events; Red Bull Ring −16 to −33°/g in all 3 runs. Driver (Red Bull Ring): "somewhat snappy on throttle", then more of an understeer issue after raising TC. Exit numbers fit the snap; slow mid-corner still read −16°/g (oversteer) after the TC change, which doesn't fit the understeer felt. Likely a measurement problem in tight corners: candidate issue |
| T8 | Change log matches the changes made | 2026-11-08 session | Tick Tried, then Next run after every garage visit; tell Claude what changed | Change log in the app lists the same changes per run as the driver made |

## Verified

| Check | Date | Evidence |
|---|---|---|
| Steering lock is recorded (38-column recording) | 2026-11-08 | Hockenheim run 1: lock 22°, wheel range 862°, ratio 19.6:1 (Mercedes-AMG GT3) |
| Tyre temperatures and pressures recorded throughout | 2026-11-08 | Hockenheim run 1: no gaps or zero readings in 38 min |
| One recording per run with Next run per garage visit (T8, first half) | 2026-11-10 | Red Bull Ring: 3 recordings, game-clock gaps of 105 s and 69 s between them (garage visits) |
| Tyre left/right and inside/outside orientation looks right | 2026-11-08 | Left tyres hotter on a clockwise track; inside 1–3 °C hotter than outside on all four. Strong hint, not proof |

## Session log

Template:

```
### YYYY-MM-DD · <track> · <car>
- Build: <commit> · Runs: <n> (<minutes> min) · Recordings: <file names>
- Changes: <what changed, between which runs>
- Driver: <what the user felt, in their words>
- App: <main detections, balance, places>
- Checks: <T# passed / failed / not covered>
- Next: <what to test next time>
```

### 2026-11-10 · Red Bull Ring GP · Mercedes-AMG GT3
- Build: 88dae46 · Runs: 3 (7 + 11 + 8 min) · Recordings: `session-1794337107-run2/3/4.csv`
- Numbering: no New session, so the app continued the Hockenheim session (stored at run 2):
  the driver's runs 1–3 are files run2–4, and Hockenheim's change history still applied.
  Sessions per car + track are planned in #11.
- Changes: in the app's change log; TC was raised during the session.
- Driver: slow corners "somewhat snappy on throttle"; with more TC it became more of an
  understeer issue. Run 3 "bottoming" at 3640 m was a kerb (driving mistake).
- UI: the driver couldn't find **New session**: it sits in the "Earlier runs" box on the
  Describe tab, not on the Live tab.
- App: problems per minute fell 3.5 → 1.9 → 0.6. Braking instability and front lockups only in
  run 1. Balance moved towards understeer: braking (fast) +10 → +21 → +18°/g, slow mid-corner
  −21 → −22 → −16°/g; run 3 flagged slight understeer under braking. Ratio 19.6:1 again.
- Checks: T8 first half verified; T5 kerb flagged as bottoming (driving, not counted as a
  failure); T7 mismatch between feel and slow mid-corner numbers. T1/T2 not seen (Live tab hidden while driving). T3, T4, T6 not covered.
- Next: press New session first; check the Live tab in the pits (T1/T2); drive the Lambo (T6).

### 2026-11-08 · Hockenheimring GP · Mercedes-AMG GT3
- Build: 88dae46 · Runs: 1 (38 min) · Recordings: `session-1794156129-run1.csv`
- Changes: several, all ticked Tried during the run; Next run pressed only at the end, so
  they are one change-log entry and the recording covers all setups.
- Driver: fixed a bit of oversteer; some of it came from overdriving the car.
- App: braking instability, front lockups and entry oversteer only in the first quarter. Slight
  exit oversteer all run (around 880 m, 1000 m, 4.1–4.3 km), wheelspin near 2140 m (hairpin
  exit). Front left tyre warmed from 78 to 84 °C over the run; the others stayed flat.
- Checks: steering lock and tyre data verified. T7 and T8 added.
- Next: press Next run per garage visit (T8); drive the Lambo for T6.

### 2026-11-07 · Mount Panorama · Lamborghini (2026 GT3)
- Build: before #37 (no steering lock column) · Runs: 3 recordings
  (`session-1794075950-run1.csv`, `session-1794077292-run1.csv`, `-run2.csv`)
- App: wheel range 680° in all three; the braking/chassis understeer split was checked on
  these (see how-it-works.md).
