# Findings

**Reviewed:** 2026-11-11, `main` at `30f4f70`.
**Scope:** all Rust crates, the UI, the rule book, ADRs, roadmap, sim-testing log and
`MACHINE_LEARNING_EVAL.MD`.
**How:** source reading, `cargo test --workspace` (27 tests pass), and a throwaway probe run over
the four real recordings in `%APPDATA%\dev.simracing.setupassistant\recordings` (Hockenheim run 1,
Red Bull Ring runs 2–4, all Mercedes-AMG GT3). No code, threshold, ADR or roadmap item was changed.
The app itself was not started.

Each finding says whether it is **measured** (on the recordings), **from code** (read, not
reproduced) or a **hypothesis**.

## 1. Summary

1. **The balance measurement has a speed-dependent offset** (measured). The "steering geometry"
   the analyzer learns is not a constant: it falls by half between 50 and 250 km/h. Using one
   median value makes slow corners read 20–55°/g towards oversteer. This explains open check T7
   ("slow-corner balance reads strong oversteer") and means slow-corner understeer is close to
   undetectable on a real car. It should be fixed before any threshold tuning, #35, or M2.4.
2. **One corner can count several events** (measured as likely): the understeer/oversteer
   episode restarts on every phase change, and a third of phase changes follow the previous one
   within 0.35 s. Counts and severity are inflated in some corners and suppressed in others.
3. **A real recording can be overwritten** (from code) by toggling the demo on and off in the
   same run.
4. **The plan is sound in order but builds fix-verification (M2.4) on a detector that isn't
   validated yet.** A thin slice of M3 should move ahead of M2.4, and recordings need a metadata
   sidecar now, while they are being collected.
5. **The ML evaluation's verdict holds** (not now; fix measurement first). It is partly out of
   date since #37, and it rates the understeer gradient as "a strong engineered feature", which
   finding 1 contradicts. Details in section 5.

## 2. Bugs

### B1. Learned steering geometry absorbs low-g understeer · high · measured

`telemetry.rs:615-630, 658-662, 673-680`. ADR-8 assumes that at 0.08–0.3 g every car follows its
steering geometry. It doesn't at speed: steering needed is `L/R + K·a_y`, and in a gentle curve
at 200 km/h the understeer term `K·a_y` is larger than the geometric term `L/R`. So the learned
gain `yaw / (speed · steer)` depends on speed:

| Speed (km/h) | Hockenheim run 1 | Red Bull Ring run 2 |
|---|---|---|
| 40–60 | 0.139 | 0.136 |
| 80–100 | 0.145 | 0.141 |
| 120–140 | 0.106 | 0.105 |
| 180–200 | 0.086 | 0.093 |
| 220–240 | 0.067 | 0.071 |
| **Analyzer's single value** | **0.085** | **0.093** |
| Kinematic value, lock 22° / wheelbase 2.6–2.7 m | 0.142–0.148 | 0.142–0.148 |

A straight-line fit of `1/gain` against `speed²` gives an intercept of 0.147–0.148 for these
two runs, which is the kinematic value. (Red Bull Ring run 4 gives 0.137; it has almost no
gentle samples below 100 km/h, so its fit is weaker. Run 3 was not probed.) Two things follow:

- The true geometry is the zero-speed intercept, not the median. The median is dominated by
  fast gentle curves (most samples are at 160–220 km/h).
- With the single value, predicted steering in a slow corner is too large, so "extra steering"
  comes out negative. Expected offset from the fit: about −43 to −56°/g at 65 km/h / 30 m
  radius, −19 to −25°/g at 90 km/h / 60 m, near 0 at 180 km/h. T7 reported −16 to −40°/g in
  slow corners. Fast corners are roughly right.

Consequences: the 30°/g threshold effectively needs 50–85°/g of real understeer in slow
corners; the balance table and needle show oversteer there; the driver's "more of an understeer
issue" at Red Bull Ring could not show up. ADR-8's "geometry agreed within 5% (0.090 vs 0.094)"
was agreement between two equally biased numbers. It is the ADR-7 failure in a milder form: the
reference absorbs part of the understeer.

The demo cannot catch this: its gentle kinks have zero extra steering even for the `Pushing`
car (`demo.rs:135-139`), so tests encode the assumption.

Suggested fix (needs an ADR superseding part of ADR-8, and before/after `replay` output):
fit `1/gain = a + b·v²` over the gentle samples (per-speed-bin medians, then a robust line) and
use `1/a` as the geometry. `b` is the car's low-g understeer gradient (0.27–0.38 road-wheel
°/g here), a useful number to show. Make the demo's kinks carry the same gradient as its
corners so a test fails without the fix. The meaning of 30°/g changes with this, so do it
together with #35 and re-check the threshold once, not twice.

Side result for #35/T6: the intercept implies a wheelbase of 2.60–2.61 m for those two runs
(2.81 m for the weaker run 4 fit; real car: roughly 2.65 m). That supports reading `steer_lock_degrees` as road-wheel lock.

### B2. Episodes restart on every phase change · medium · measured (flicker), from code (effect)

`telemetry.rs:724-727, 733-736`. When the phase changes during understeer or oversteer, the
episode is reset, including `counted`. Phase depends on hard cut-offs (brake 0.05, throttle
0.6), so a driver modulating the throttle around 60% flips mid ↔ exit. On Hockenheim run 1,
339 of 1056 phase changes while cornering came within 0.35 s of the previous change.

- Flips slower than the minimum time: one corner counts 3–4 understeer events, which raises
  severity and clicks.
- Flips faster than 0.35 s: `on_for` never reaches the minimum, so real understeer is missed.

Fix: hysteresis on phase (e.g. exit above 0.6, back to mid below 0.4, with a minimum dwell), and
keep one episode per phase instead of resetting a shared one. Needs a demo case.

### B3. Chassis and braking understeer can both count for the same moment · medium · from code

`telemetry.rs:706, 717-730`. `braking_slip` is a per-frame test on front wheel speed. With ABS
pulsing it toggles, and because episodes tolerate 0.3 s gaps, the `understeer` and
`braking_understeer` episodes both accumulate on-time and can both fire in one corner. The
balance sample also alternates between the "braking" and "entry" rows. Fix: latch the braking
state for a short time (like `light_for`).

### B4. Toggling the demo overwrites the run's recording · medium (data loss) · from code

`live.rs:145-162, 400-410`. `set_demo` closes the recorder. After the demo is switched off,
`recorder()` reopens `session-<id>-run<n>.csv` with `File::create`, which truncates what was
recorded before. Same for any path that closes and reopens within one run number. Fix: never
truncate (`create_new`, and add a suffix on collision). Belongs with #6.

### B5. Source line is empty or stale while not driving · medium · from code

`r3e-telemetry/src/lib.rs:165-173`. The adapter returns `Poll::Idle` before refreshing the
identity, so in the garage or menus the app shows the identity of the last driven frame, or
nothing before the first drive. Checks T1/T2 in `docs/sim-testing.md` ask the driver to look
"in the pits": they will see nothing on a fresh start and the old car after a car change. Fix:
compute the identity before the `driving` check.

### B6. Unticked-but-remembered changes go into history · low-medium · from code

`app/ui/main.js:170-174, 442-456`. Removing a single problem doesn't prune `state.tried`. A
ticked card that disappears stays in the map, is counted in "Next run · n changes made", and is
saved as an applied change that the driver can no longer see or untick. (`Clear` does prune.)

### B7. Power oversteer after the corner is filed as mid-corner · low-medium · from code

`telemetry.rs:698-702, 732`. Oversteer doesn't require cornering g, and a "straight" phase maps
to mid. A slide on corner exit, once lateral g drops below 0.35 g on full throttle, is recorded
as mid-corner oversteer and gets mid-corner advice. Use throttle to pick exit there.

### B8. Negative ride height means both "unknown" and "hit the ground hard" · low-medium · measured

`telemetry.rs:758`. RaceRoom reports negative ride heights on hard strikes (min −11.6 mm,
Hockenheim; −2.5 mm, Red Bull Ring run 4). The detector only counts 0–8 mm, so the hardest
contacts don't count while the approach to them does. Kerb strikes (T5) are also counted as
bottoming with no roadmap item to separate them; a short spike on one wheel with a vertical
jolt is a kerb, a sustained low reading on an axle is bottoming.

### B9. Hardening · low · from code

- `track.rs:143-155, 176`: a saved map is only checked for track length. An empty or short
  `bins` array panics in `add`. A panic in the telemetry thread poisons the mutex, and every
  command then panics (`lock().unwrap()`), with no console in release builds. Validate
  `bins.len()` on load; recover from poisoning or surface it.
- `track.rs:147-154`: a length mismatch or parse error silently replaces the saved map (corner
  names and run history) on the next save. Keep the old file as `.bak`. Part of #5.
- `telemetry.rs:640`: until geometry is learned, `pushes_since_geometry` never resets, so the
  vertical-level median (sort of up to 4000 values) runs every frame. Harmless, wasteful.
- `telemetry.rs:645`: without gravity in the signal (RaceRoom: median 0.0), the up/down sign
  can't be learned and is assumed. ADR-6 says "either sign"; that only holds with gravity
  included. Document it or let adapters state the convention.
- `ui_preview.rs`: accepts cross-origin POSTs and unbounded `Content-Length`. Dev-only, local,
  no disk writes; low risk.

### Worth checking, not confirmed

- **Body slip in hairpins** (hypothesis). `slip_deg` uses lateral velocity at the car's
  reference point. At low speed that includes a geometric part of roughly `b/R` (about 3.7° at
  20 m radius) with no sliding. Thresholds of 2.5°, 3.5° and 6° could be reached in tight
  hairpins. A replay comparing slip against corner radius would settle it.
- **Wheelspin/lockup use any wheel** vs car speed; inner/outer wheel speed differences in tight
  corners eat part of the margin. Already covered by #28.

## 3. What could be done better

- **Recordings have no metadata.** Car, track, app version, thresholds, channel mask, resets and
  run boundaries live only in `docs/sim-testing.md`, by hand. Write a small JSON sidecar when a
  recording is created. This is cheap, and every recording made before it exists is harder to
  use later. It is in the ML evaluation (§4.1) and ideas.md (#3) but in no roadmap item.
- **Replay is not the live analysis** (no mask, no lap length, no track map, no resets). Make
  the analysis a function both paths call, and add one test: live session over the demo vs
  replay of the same samples gives the same report.
- **Detections keep only a count and a distance.** An episode record (start, end, peak,
  phase, suppressions) is the base for #19, #18 and #20. Build it once.
- **The demo is too ideal to protect the detectors.** B1 and B2 pass all tests. Add low-g
  understeer, throttle modulation around the phase cut-offs, ABS pulsing and a kerb strike.
  Commit one or two short real excerpts as soon as `excerpt` (#17) exists.
- **Session contamination until #11 lands.** Red Bull Ring continued Hockenheim's history
  because New session wasn't pressed. The backend already knows car and track (#3): prompt
  "new car/track: start a new session?" when the identity changes. Small, and it protects data
  being collected now.
- **Severity per minute** dilutes on long tracks and counts B2's duplicates. #20 covers it;
  B2 should be fixed first or the comparison is skewed.
- **Tyre hints**: "inside isn't hotter than outside" fires below 2 °C spread; the real car
  showed 1–3 °C on all four tyres, so the hint would show most of the time. Hold these hints
  back until #30/#31.
- **Status polling does repeated work under the telemetry lock** (corners and features are
  recomputed per place, per poll, for two windows). Recordings show a steady 10 ms sample
  interval (p99 15–20 ms), so this is not urgent; cache by map revision when touching it.
- **Docs drift**: the ADR index table stops at ADR-17 (18 and 19 exist); ADR-18 lists 34/36/37
  columns (now 38); ADR-8's "agreed within 5%" should get a note once B1 is decided; the UI text
  hard-codes "RaceRoom".
- **`main.js` is 880 lines** holding session, commands and rendering. It works; split it when
  #10/#11 move the session to the backend, not before.

## 4. The plan (docs/roadmap.md)

What is good: the order (trustworthy data → context → evidence → alpha), one issue per PR, ADRs
for direction changes, an explicit alpha gate, and the driving log. Decisions are recorded with
reasons. Keep all of that.

What I would change:

1. **Validate the detector before building on it.** M2.4 decides "resolved" from "no events in
   3 clean passes". With B1, slow-corner understeer produces no events whether or not it is
   fixed. Move a thin slice ahead of M2.4: B1 + #35 as one ADR'd change, B2/B3, the episode
   record (#19), and a minimal `evaluate` (#18) with replay parity. The rest of M3 can stay
   where it is.
2. **Add the missing items**: geometry fit (B1), phase hysteresis (B2), recording sidecar,
   no-truncate recordings (into #6), identity while idle (B5, a bug against closed #3), kerb vs
   bottoming (T5), identity-change prompt.
3. **Loosen two dependencies.** #17 (`excerpt`) doesn't need #16 (notes and tags); a sidecar is
   enough. #22 (collect recordings) is already happening and shouldn't wait for #17.
4. **Strengthen the alpha gate.** "No unexpected false detections on 3 tracks" tests precision
   only. T7 was a miss, not a false alarm. Add one known-answer run per detector (a deliberate
   setup change with a known direction, A/B/A) and the physics check from B1: the fitted
   geometry must match lock / wheelbase.
5. **Size.** 30 of 31 issues are open; 20 stand before the alpha. Since the plan was written,
   merged work was #34, #37, a UI fix and the test skill: useful, but driven by what driving
   revealed, not by the M1 list. That is the right instinct. Candidates to move after the
   alpha: #7 (polling cost; measured fine above), #9 (Playwright; brings a JS toolchain into a
   no-npm project, #8 gives most of the safety cheaply), #15 beyond anchored names, #14.
6. **Keep doing driving tests between PRs.** The sim-testing log found more real problems (T5,
   T7, New session) than code review did. Add "replay output attached" to the session template.

## 5. Re-evaluation of MACHINE_LEARNING_EVAL.MD

**Verdict: still right.** ML could help later; it should not be in the current plan; fix
measurement first; calibration (experiment A) before a model (experiment B); no ranking model
without verified outcomes. Nothing found here argues for starting earlier. B1 argues for more
caution.

**Out of date since #37 (`88dae46`):**

- §2 table: recordings have 38 columns, not 37; `read_checked` handles 34/36/37/38.
- "Data the game exposes but the core drops": `steer_lock_degrees` is now in `Sample`, the
  recording and `replay`.
- §2.2: no longer only arithmetic. Measured: AMG GT3 wheel range 862°, lock 22°, ratio 19.6:1.
  With the 540° fallback that car would read 37% low. The lock-is-road-wheel question now has
  supporting evidence (B1 side result).
- "No real recordings": still true for the repo (no `fixtures/`), but four local recordings
  (64 min, one car, two tracks) exist and were enough to find B1.

**What it gets wrong or misses:**

- §2 calls the understeer gradient "a strong engineered feature". It has a speed-dependent
  offset of up to −55°/g (B1). §2.2 identifies a unit problem (a scale factor per car); the
  geometry bias is larger and changes sign of the reading in slow corners. It should be
  prerequisite number one in §1 and §3 rank 0.
- This is exactly the trap §5.2 warns about, with a concrete case: any model given speed as a
  feature would learn the offset and beat fixed thresholds. So would tuning `slow_corner_kmh`
  or per-class thresholds in experiment A. Neither result would mean anything. Experiment A
  must not start before B1 is fixed.
- §2 event counting says one corner "can produce two understeer events". It can produce more,
  or none (B2). Any label-to-event mapping in §4.2 has to handle that, or B2 gets fixed first.
- §5.4 known-answer tests should add the free one: gain vs speed must extrapolate to
  lock / wheelbase. It needs no labels, no second setup and no reviewer, and it would have
  caught B1.
- §4.1 puts the sidecar in the ML data section. It is needed now for ordinary replay and
  belongs in M1.

**What holds up well:** the two data traps (detector output is not ground truth; "fixed" is not
an outcome), blind review with control windows, precision-first error budget, the rule of three
(checked: 0 alerts in 1.5 h still allows about 2/h), report-level metrics, shadow mode, and the
list of approaches to defer.

**On the document itself:** it is 365 lines for a decision of "not now". Suggest cutting it to
a one-page decision record (verdict, prerequisites, the two experiments, stop conditions) and
moving the prerequisites into issues, where three of them already are or should be (#19, #35,
replay parity in #18). The long evaluation protocol can stay as an appendix; it will only
matter if experiment A ever leaves a gap.

## 6. Limits of this review

- B1, B2 (flicker rate), B8 and the sample interval are measured on four recordings of one car
  on two tracks. The gain-vs-speed shape should be confirmed on the Lamborghini (T6) and one
  low-downforce car; aero changes the `v²` term but not the intercept argument.
- B3–B7 and B9 come from reading the code and were not reproduced in the running app.
- The probe used for the measurements was a scratch program outside the repo. It would make a
  reasonable `examples/geometry.rs` if B1 is taken up.
- No thresholds were evaluated; the body-slip item is a hypothesis.
- UI layout, accessibility, the rule book's advice quality and `layout.rs` offsets were not
  reviewed in depth.

## 7. Follow-up (2026-11-11)

| Finding | Where it went |
|---|---|
| B1 geometry | #40 (roadmap M3.8, with #35); ADR-8 note |
| B2 phase flicker | #41 (M3.9) |
| B3 ABS pulsing | #45, PR #53 |
| B4 recording truncated | #46, PR #56 |
| B5 identity while idle | #47, PR #57 |
| B6 ticked changes of removed problems | #48, PR #58 |
| B7 exit slides as mid-corner | #49, PR #54 |
| B8 negative ride height | #50, draft PR #55 (lands with #42: every new event in the recordings is a one-wheel kerb strike) |
| Kerb vs bottoming (T5) | #42 (M3.10) |
| B9 hardening | #51, PR #59 (ui_preview item not done) |
| Recording sidecar | #43 (M1.8) |
| New-session prompt on car/track change | #44 (M1.9) |
| Plan, alpha gate, dependencies | docs/roadmap.md ("Order of work", alpha gate) |
| ML evaluation | Decision record on top of MACHINE_LEARNING_EVAL.MD, corrections inline |
| Docs drift | ADR index (18, 19), ADR-18 columns, ADR-8 note; replay line in the sim-testing template |

Not taken up: hard-coded "RaceRoom" in the UI text (fine until a second game, #32); tyre
hints held back until #30/#31 (noted on those issues); status polling cache (noted on #7);
splitting `main.js` (with #10/#11).
