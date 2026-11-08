# Machine learning evaluation for Setup Assistant

**Reviewed:** 2026-11-08, `main` at `6d509a9` (merge of #34, "Sources report identity and available channels").
**Status:** Research evaluation only. Nothing here changes the roadmap, ADR-1 to ADR-19, thresholds, dependencies, or code. If model-assisted decisions are ever accepted, that takes a new ADR.
**Revision:** Second pass. Every code claim was rechecked against the source. Added: the missing episode log (§2.1), steering-unit normalization (§2.2), replay/live parity gaps (§2.3), report-level metrics, the rule of three, an asymmetric error budget, and known-answer setup perturbation tests (§5). The calibration experiment now comes before the model experiment (§8).

## 1. Verdict

**Yes, machine learning could improve the project later. No, it should not be part of the current plan.**

The best first use is not "AI tunes the car." It is a small, offline-trained, locally executed model that helps the existing analyzer decide whether a telemetry episode is a real, repeatable handling symptom or a transient crest, kerb, lockup, or noisy sample. That preserves the current product: physics detects, rules advise, the driver decides.

Order of work:

1. **Fix measurement before modelling (no ML):** an episode log, steering-unit normalization, and replay that matches live analysis (§2.1-2.3). These are small, help M3 directly, and every later step depends on them.
2. Finish M1-M3 and collect real recordings across at least three tracks, including known-answer setup perturbation runs (§5.4).
3. Offline: compare current thresholds against tuned global and class thresholds (experiment A, §8).
4. Only if tuned thresholds leave a measured gap: a small episode-validity model (experiment B, §8). If it wins on held-out real driving, run it in shadow mode beside the analyzer. It must not change advice.
5. Consider recommendation ranking only after M2 and M5 produce verified, one-change-at-a-time outcomes.

ML must not become a prerequisite for the public alpha. There is no evidence yet that a trained model beats a well-calibrated version of the current analyzer. The bottleneck is trustworthy labels, representative driving data, and consistent units, not model capacity or compute.

## 2. What the repository actually is today

The project is a deterministic expert system with a physics-informed front end:

- `crates/setup-core/src/telemetry.rs` learns steering/yaw sign, geometric steering gain (median of up to 4000 gentle-curve samples, refreshed every 200 frames), understeer gradient in steering-wheel degrees per g (0.2 s smoothing), vertical load baseline, and detects sustained episodes. The understeer gradient is already a strong engineered feature; it is not raw sensor data.
- `crates/setup-core/src/rules.rs` ranks changes from `rules/gt.json` by weighted votes, rank order, severity, conflicts, and history. `Suggestion.score` is a ranking weight, not a probability.
- `crates/setup-core/src/track.rs` learns a per-track map in 10 m bins: elevation, lateral g, speed; then corners, crests, compressions, and recurring places.
- `crates/setup-core/src/live.rs` polls a `Source` about every 10 ms, analyzes, records, and updates the track map under one mutex.
- `crates/r3e-telemetry` mirrors RaceRoom's official shared memory and declares available channels (ADR-19).

This is a good ML baseline, but the repository is missing the parts a supervised model needs.

| Area | Current evidence | ML consequence |
|---|---|---|
| Detection | Hand-tuned `Thresholds`: 30 deg/g understeer for 0.35 s, 120 km/h slow/fast split, 0.35 g cornering, 0.75 load for crests, 0.008 m bottoming, 0.85 lockup ratio, 1.12 wheelspin ratio, 0.04 front slip under braking | Strong prior. Any learned component must beat a tuned version of this, not an untuned one. |
| Features | Understeer gradient, slip angle, lateral g, phase, speed class, braking slip, wheel-speed ratios, load, episode state, place | Enough for a small tabular model, but only computed per frame and then discarded (§2.1). |
| Event counting | `Episode` hysteresis: detector-specific on-time (0.35 s understeer, 0.15 s oversteer/wheelspin, 0.08 s lockup), 0.3 s gap. Counted at **onset**. Understeer/oversteer episodes restart when the corner phase changes. Gaps over 0.5 s reset all episodes | One corner can produce two understeer events (mid, then exit). Detector events are not corner passes; labels need an explicit mapping. |
| Reporting | A problem is shown only after `min_events = 2`; places cluster within 80 m; "one place" at 70% share | A model that removes one event can hide a whole problem. Evaluate what the driver sees, not only events (§5.3). |
| Severity | `count / driving_minutes`, denominator floored at 1 minute, straights included; <1 slight, <2.5 moderate, else severe | Fixed rule, not calibrated confidence. Not normalized by opportunity (corner time per phase/speed). Do not reuse as a label. |
| Advice | 22 parameters, 9 behaviours, 18 rules; votes weighted by rule rank and severity | Explainable baseline and candidate generator. Learned ranking is not justified yet. |
| History | `model.rs` stores applied clicks and fixed symptoms. `nextRun()` in `app/ui/main.js` marks every problem of the finished run fixed at the **start** of the next run, before any driving; `unfixDetected` removes the mark only if the problem is detected again (≥2 events) | "Fixed" means "not seen again yet". It depends on how long the next run was. It is not a verified outcome. |
| Recordings | 37-column CSV: time, speed, pedals, steering, yaw, lateral acceleration/velocity, wheel speeds, tyre temps/pressures, ride heights, gear, steering range, vertical acceleration, lap distance, elevation | Raw replay works, but context, channel mask, resets, setup state, conditions, and labels are absent. |
| Recording import | `read_checked` supports 34/36/37 columns and rejects bad rows; older layouts load | Good reliability base. Still no schema version or provenance in the file. |
| Channel masking | `Channels::mask` turns unavailable channels into their unknown value before analysis and before the track map | Necessary for honest missing-data behavior. Recordings store the unmasked sample and not the mask. |
| Track map | 10 m running means; corners above 0.4 g; features above 3 m/s² curvature effect; last 12 run summaries | Useful place features. Summaries omit clean runs and do not encode what changed. |
| Tyres | Temperatures average only when all 12 are present; pressures average only when all 4 pressures **and** all temperatures are present; fixed spread thresholds after 30 s | One missing sensor hides every tyre reading. No uncertainty. |
| Tests | Synthetic demo laps assert exact detections; `replay` example prints one recording and only overrides the understeer threshold | Good software regressions, not independent real-world validation. |
| Corpus | No `fixtures/` directory, no real recordings, no trained artifacts, no model dependencies | Achievable accuracy is unknown. |

### 2.1 There is no episode to score yet

`Analyzer::record` stores a count and a lap distance per `(behavior, phase, speed)` key, at the moment an episode passes its minimum on-time. Nothing records when the episode ended, how long it lasted, its peak or mean gradient, slip, load, brake, the calibration state at the time, or whether a crest suppressed part of it.

So "score the episodes the detector emits" has nothing to score. The prerequisite is an **episode log**: one record per episode with start/end time and distance, phase(s), speed class, peak/mean of the detector's own inputs, calibration age, and active suppressions (light load, braking slip, countersteer). It is not ML. It also covers M3.3 (detection evidence), gives M3.2 `evaluate` something to match against labels, and supplies the evidence the driver should see. Build it first.

### 2.2 The understeer threshold is in the wrong unit for comparing cars

The gradient is `extra_steer × half_range / lateral_g`, in **steering-wheel** degrees. Two consequences:

- **Steering ratio.** A car with a 16:1 rack needs a third more wheel angle than a 12:1 car for the same front-axle behavior. A single 30 deg/g threshold then means different handling on different cars. Much of the "per-class threshold" variation predicted by ADR-8 may be this unit effect, not a class difference.
- **Unknown wheel range.** When `steer_range_deg` is missing, the analyzer assumes 540°. On a car with a 720° wheel the gradient reads 25% low; on a 360° wheel it reads 50% high. RaceRoom reports `steer_wheel_range_degrees` only sometimes (the adapter test covers it being zero).

Normalize before calibrating or learning: express the gradient in road-wheel degrees per g where the ratio is known (RaceRoom's `steer_lock_degrees`, if a real-car check confirms it is road-wheel lock), and treat episodes measured with an assumed range as lower-quality evidence. Otherwise any calibration or model will learn per-car unit corrections and call them handling.

### 2.3 Replay is not the analysis the driver saw

The `replay` example creates an `Analyzer` with default channels and no track length, and never builds a track map. In live analysis:

- missing channels are masked (recordings store the raw sample, including placeholders);
- the lap length merges places either side of the start line;
- the track map adds crest/compression labels and `persistent` across runs;
- `LiveSession::reset` can clear the analyzer mid-recording, and identity (and therefore the mask) can change mid-run. Neither event is in the CSV.

An evaluator built on today's replay measures a different system. M3.2 must replay with the recorded identity, mask, lap length, resets, and a track map built only from data available at that time. Add one golden test: live session over the demo source vs replay of the same samples must produce identical reports.

### Data the game already exposes but the core drops

The RaceRoom layout (`crates/r3e-telemetry/src/layout.rs`) includes signals that `Sample` does not carry: per-tyre `tire_load`, `tire_grip`, `tire_wear`, `tire_flatspot`, `tire_dirt`, `tire_type`, `brake_temp`, `brake_pressure`, `suspension_deflection`, `suspension_velocity`, `fuel_left`, `fuel_capacity`, `fuel_per_lap`, `traction_control_setting`, `traction_control_percent`, `abs_setting`, `brake_bias`, `steer_lock_degrees`, `local_g_force`, and `steering_force`.

These are valuable candidates for features, context, and weak labels. `steer_lock_degrees` matters most for §2.2. Fuel and tyre wear matter as confounders in run comparison. They are not free: the core is game-agnostic (ADR-3), so each must be exposed through a capability-gated channel, validated on real cars, and handled as missing when a second adapter cannot provide it. Do not train on a RaceRoom-only field and pretend it generalizes to LMU.

### The two data traps

1. **Detector output is not ground truth.** The demo tests and most labels would come from the same analyzer being evaluated. A model trained on those labels learns to reproduce the current detector, including its false positives. Labels need independent review or an external outcome (§4.2, §5.4).
2. **The UI's "fixed" flag is not an outcome.** It is set before the next run is driven and cleared only by re-detection. The roadmap's M2.4 lifecycle fixes this. Until then, `history.fixed` cannot supervise a recommendation model.

## 3. Where ML could add value, ranked

| Rank | Opportunity | Value | Readiness | Smallest worthwhile approach | Verdict |
|---|---|---|---|---|---|
| 0 | Episode log, unit normalization, replay parity | Enables everything below | Code exists to extend | Plain Rust; no model | Prerequisite. Do first, with M3. |
| 1 | Class-aware threshold calibration | High | Needs M3 corpus and §2.2 | Offline search over a small, physically bounded parameter set; partially pooled per class | Experiment A. It may remove the need for a model. |
| 2 | Episode validity / false-positive reduction | High | Needs episode log and independent labels | L2 logistic regression over existing derived features | Experiment B, only if A leaves a gap. |
| 3 | Run comparison and improvement estimate | High product value | Needs M2, M5.3 | Matched per-corner statistics first; model only for residual variation | Statistics first. ML only if it beats them. |
| 4 | Recommendation re-ranking | Potentially high | Needs verified interventions | Regularized outcome model over candidates already produced by `RuleBook` | Defer. Data does not exist yet. |
| 5 | Tyre warm-up / hot-pressure forecast | Medium | Needs M5.4 tyre semantics | Per-compound regression or a small state model | After channel semantics are confirmed. |
| 6 | Track map refinement | Medium | Data exists | Probabilistic segmentation, per-bin confidence, robust smoothing | Geometry and statistics first. Low urgency. |
| 7 | Finding recordings worth labelling | Medium | Needs a corpus | Disagreement, feature diversity, outlier scoring | Offline contributor tool only. |
| 8 | Natural-language symptom entry | Convenience | Taxonomy exists | Catalog/synonym matcher first; constrained LLM only if needed | Optional, independent of telemetry ML. |
| 9 | Inverse setup estimation or RL | Unproven | Nothing needed exists | None justified | Do not pursue. |

### 3.1 Calibration first, because it is cheaper and may be enough

The current thresholds are global guesses. ADR-8 says 30 deg/g is a first guess that needs tuning per car class; ADR-14 says tune against real laps. After the unit fix in §2.2, check whether class differences remain at all before fitting class-specific values.

Search a limited set of parameters around the current defaults: `understeer_deg_per_g`, `understeer_min_s`, `slow_corner_kmh`, `light_load`, `front_slip_braking`, `lockup_ratio`. Compare global, class-specific, and partially pooled settings (shrink each class toward the global value in proportion to its data). With little data, use the global value rather than inventing an extreme class parameter.

Few tracks make overfitting easy: with three tracks, a fine grid over six parameters will find a combination that looks perfect by chance. Keep the grid coarse and physically motivated, select on validation recordings only, and report the held-out track separately.

This is not necessarily machine learning. If a robust search over a handful of physically meaningful thresholds solves the observed errors, ship that and stop.

**Hard constraint:** never redefine a car's persistent understeer as its normal baseline. ADR-7 documents exactly that failure: the first analyzer learned from the same understeering corners and called an all-round understeering car normal. Any per-driver normalization, anomaly score, or personal model must be tested against that case (§5.4 gives a test).

### 3.2 Episode validity is the strongest first supervised use

The product already separates "detect a symptom" from "recommend a change." A model can sit between them and answer: is this episode a repeatable handling symptom or an artefact?

Useful inputs, all derivable from the episode log (§2.1) plus a short trailing window:

- Understeer gradient (normalized per §2.2), slip angle, yaw/steering consistency, calibration age and quality, measured vs assumed steering range.
- Episode duration, peak and variation of lateral g, speed, steering, brake, throttle.
- Front/rear wheel-slip ratios and driven-wheel context.
- Vertical load, ride-height behavior, crest/compression context, kerb proximity if available.
- Channel availability, data gaps, stale frames.
- Car class and corner phase, with an explicit unseen-context fallback.

Start with L2 logistic regression. It gives roughly calibrated scores with little data and can be shipped as coefficients. Try a shallow tree model only if it leaves a documented gap. Predict separate labels such as `real_understeer`, `crest_effect`, `braking_effect`, `insufficient_evidence`, because symptoms overlap.

Score the episodes the current detector already emits. This keeps integration small, but it cannot recover missed events. Evaluate recall over whole recordings, not only candidate windows. Expanding candidate generation is a separate experiment.

Show the measured evidence the driver already understands: duration, slip, phase, place, load, channel quality. A model score or feature attribution is not a mechanical explanation.

### 3.3 Run comparison before learned recommendations

M5.3's per-corner before/after table is a better investment than a model that promises lap-time gains. Match passes by corner, phase, entry speed, and data quality. Compare event frequency, balance, exit behavior, and driver assessment against repeat-run variation.

The first number to measure is the **minimum detectable change**: how much per-corner balance and event rate vary between repeated runs with no setup change. A setup change whose effect is smaller than that cannot be shown to work, by statistics or by a model.

A later model could adjust for measurable confounders such as tyre state, fuel mass, or entry speed. It cannot adjust for what is not recorded: line changes, traffic, damage, weather, grip evolution, or the driver learning the track. Reduced detections are not proof of a better setup; a smoother driver can simply stop triggering the detector.

### 3.4 Recommendation ranking needs intervention data that does not exist

The rule book produces an ordered candidate list. A learned ranker could reorder those candidates for a context, but only after sessions record:

- the exact setup values before and after, not just "two clicks softer";
- other simultaneous changes;
- the suggestions shown and their order;
- the chosen action and any override reason;
- follow-up passes with comparable coverage;
- an independently checked outcome.

Even then, the data is biased: the top suggestion is the one most often tried, so its outcome is over-represented. An untried candidate has an unknown outcome, not a negative one. Multiple simultaneous changes cannot identify which one helped. A post-change improvement can be regression to the mean or driver adaptation.

If this is ever attempted, keep capability filtering, parameter limits, conflicts, and history guards explicit. A learned score may reorder candidates; it must not invent a parameter, bypass ADR-1, or write setup files.

### 3.5 Tyres, track, and language are later and smaller

- **Tyres:** after M5.4 confirms inner/middle/outer semantics, predict stabilization and hot pressure per compound and stint stage. Predicting pressure is better defined than claiming an optimal cold pressure or camber from telemetry alone. Fix the all-or-nothing averaging first: one missing sensor currently hides every temperature and pressure.
- **Track:** the 10 m bins already provide place features. A model could add per-bin confidence or better segmentation, but a robust geometric method is easier to explain and validate. ADR-13's thresholds are still guesses; tune them on real tracks before adding a model.
- **Language:** map "it pushes wide when I trail-brake" to the existing symptom taxonomy. Try a synonym/catalog matcher first. If an LLM is used, validate structured output against known behaviour, phase, and speed values, show the interpretation for correction, and keep high-frequency telemetry out of the request.

## 4. Data needed to make any of this credible

### 4.1 Add a sidecar, not a new database

The recording CSV should stay simple. Put context beside it in a versioned JSON sidecar (or a session-level manifest):

| Field | Minimum content | Why |
|---|---|---|
| Provenance | recording/session/run ID, app version/commit, adapter/API version, schema version, rule and threshold version | Reproduce a result and detect behavior drift. |
| Context | game, car/model/class, track/layout, session kind, demo/real flag, optional pseudonymous driver ID | Split evaluation and avoid mixing incompatible contexts. |
| Availability | channel mask with change times, measured or assumed steering range, stale/invalid intervals | Replay must mask exactly as live analysis did (§2.3). |
| Events | analyzer resets, run starts, identity changes, with sample times | Replay must reset where live did. |
| Conditions | tyre compound/age if known, fuel, weather/grip/damage, or explicit unknowns | Separate controllable from missing confounders. |
| Intervention | actual change, old/new value where known, other changes, shown suggestions, chosen action | Support any later effectiveness analysis. |
| Annotation | time or corner-pass interval, symptom tags, uncertain vs clean, reviewer, evidence source | Labels cannot be reconstructed from raw frames. |

Keep identity and capability changes visible. `SourceIdentity` exists in `live.rs` and ADR-19, but it is not attached to recordings. Recording file names (`session-<epoch seconds>-run<n>.csv`) can collide; fix that (M1.4) before collecting at scale.

### 4.2 Labels

- Label intervals or corner passes, not frames. Include symptom presence, phase, speed class, and `uncertain` as a separate state from `no symptom`. Map detector events to passes explicitly (one pass can hold a mid and an exit event).
- Use multiple tags where symptoms overlap: braking understeer and a lockup can describe the same moment.
- Keep ordinary clean driving in the set. A dataset of only detections teaches the model what a detection looks like, not what normal looks like.
- **Review blind.** Let the analyzer propose candidate windows, mix them with randomly sampled control windows, shuffle, and hide the detector's verdict from the reviewer. Otherwise the reviewer confirms the detector.
- Review a subset twice (by a second person, or by the same person a week later) and record disagreement. Driver impressions are useful but not sensor truth.
- Include long continuous recordings and missed-event searches. A clip centred on a known detection hides false negatives.
- Keep synthetic laps for invariants and failure injection. Report their accuracy separately; idealized physics makes any model look better.

### 4.3 Sampling and collection budget

At 100 Hz, one hour is about 360,000 frames but only a few hundred corners, far fewer complete runs, and very few controlled setup changes. Frame count is not coverage.

A feasible pilot is hundreds of reviewed candidate and control windows across tens of runs, several cars or classes, multiple drivers where possible, and the roadmap's three-track collection. That is a collection suggestion, not a sample-size guarantee. Expand until learning curves, subgroup coverage, and label disagreement stabilize.

Recommendation research needs independent intervention experiments, not more frames from the same stint: repeated no-change runs and, where practical, A/B/A comparisons with one adjustment at a time. The known-answer runs in §5.4 are the cheapest start.

## 5. Evaluation protocol

### 5.1 Prevent leakage

1. Split whole sessions and related excerpts before generating overlapping windows. Near-duplicate clips stay in one split.
2. Keep separate evaluations for a known car, an unseen driver, an unseen track, and an unseen car class. Do not claim cross-game generalization from RaceRoom-only results.
3. Fit normalization, threshold selection, feature selection, and probability calibration on training/validation only. The final holdout is used once.
4. Replay in time order with live parity (§2.3). A live prediction cannot use a centred window, end-of-run statistics, or a track map learned from the future portion of that recording.
5. Preserve calibration resets, missing-channel behavior, clock resets, and stale data during replay.
6. Treat lap distance, car ID, and track ID as potential memorization shortcuts. They may be valid for a deliberately track-specific model, but an unseen-track claim must expose the limitation.

### 5.2 Compare against the strongest deterministic baseline

For the same held-out recordings:

1. Frozen current analyzer and rule book.
2. Same algorithms after the unit fix (§2.2), with default thresholds. This isolates what normalization alone buys.
3. Same algorithms with tuned global and class thresholds.
4. A simple learned model on the same evidence.
5. A more complex model only if the simple one leaves a documented gap.

If tuned thresholds match the model, ship the thresholds. If the model only compensates for wrong sensor semantics, wrong units, or a missing capability check, fix the data interpretation instead.

### 5.3 Metrics

Measure at three levels, because the driver only sees the last two:

| Level | Primary | Also check |
|---|---|---|
| Episode | Per-symptom precision and recall with one-to-one time/place matching | False alerts per clean driving hour and per eligible pass; latency; fragmentation; missed episodes |
| Run report | Agreement of the reported problem set (behavior, phase, speed) and severity with the reviewed run | Problems that appear or vanish only because of `min_events = 2`; wrong "one place"/"persistent" flags |
| Advice | Change in the top-3 suggestions vs those from reviewed symptoms | Suggestions that would reverse a correct earlier change |
| Confidence | Reliability plot and Brier score on independent labels | Calibration by class; error rate vs abstention coverage |
| Run comparison | Agreement with reviewed improved/unchanged/worse outcomes | False claims of improvement; minimum detectable change; comparable-pass count |
| Tyre prediction | Absolute error in pressure/temperature units | Error by compound/stint stage; interval coverage; missing-channel behavior |
| Recommendation ranking | Prospectively verified targeted improvement | Worsened symptoms; driver preference; action-selection bias; pace/stability trade-off |
| Runtime | p95/p99 inference and end-to-end latency | Dropped samples, memory, CPU, artifact size; both windows open |

**Errors are not symmetric.** A false detection produces advice that can make a good car worse, and the driver may trust it because it came from telemetry. A missed detection costs less: the driver can still describe the problem manually. Choose the operating point for precision first. State the tolerated recall loss explicitly; do not optimize F1 by default.

Do not report overall frame accuracy. A model that predicts "normal" almost everywhere scores well and is useless. Compute uncertainty by resampling sessions and drivers, not correlated frames. Show per-class, per-track, and per-driver slices. With three tracks, an "unseen track" result rests on three data points: report each track, not a pooled average.

**Zero is not zero.** If no false alerts occur in *T* hours of reviewed clean driving, the 95% upper bound on the rate is about 3/*T* per hour (rule of three). 90 minutes of clean driving with no false alert still allows up to about 2 per hour. Report that bound next to the alpha gate result.

### 5.4 Known-answer tests: setup perturbation

Human labels are expensive and biased toward the detector. Physics provides labels for free when the setup is changed on purpose:

- Drive the same car and track with a baseline setup and with an exaggerated, known-direction change, e.g. front anti-roll bar to maximum and rear to minimum, then back to baseline (A/B/A).
- The physics answer is known: mid-corner understeer gradient must rise in B, and fall back in the second A. The detector must report more understeer in B, at more places, not at one crest.
- Repeat with a known-oversteer change and a brake-bias change (braking understeer/lockups).
- For ADR-7: build a deliberately understeering car and check it is reported everywhere, not learned as normal.

These runs test direction and sensitivity, not exact counts. They are independent of the analyzer, need no reviewer, and are the cheapest data for the later run-comparison and ranking work. Add them to the M3.6 collection.

### 5.5 Promotion and rejection

Before fitting the final candidate, declare the primary metric, tolerated recall loss, minimum improvement, and runtime budget. Base them on measured baseline errors, not on generic ML targets.

Promote only when:

- improvement over tuned thresholds holds on held-out real recordings, at the run-report level and not only per episode;
- important subgroups do not regress;
- known-answer perturbation tests still pass;
- probabilities and abstention behave as advertised;
- the model runs locally within the measured budget;
- fallback to the deterministic analyzer is predictable.

Otherwise keep the deterministic baseline and keep the improved dataset and evaluator. Inconclusive evidence means shadow mode or stop, not "ship and see."

The roadmap's alpha gate (no unexpected false detections on at least three tracks) is useful regression coverage, not a generalization guarantee (see the rule of three above).

## 6. Smallest architecture that fits

- **Train offline.** Keep training dependencies in an optional contributor workflow. Reuse recordings and the planned `evaluate` tool. Do not add a Python or GPU dependency to the desktop app.
- **Infer locally.** A linear model ships as coefficients plus normalization metadata. A small tree ensemble can also be serialized compactly. Add ONNX or a learned runtime only if a justified model cannot be represented otherwise.
- **Share feature definitions.** Compute features through the Rust path used in live and replay analysis (the episode log). If training uses another language, add golden feature/output comparisons to prevent unit, window, missing-value, and precision drift.
- **Score episodes, not frames, and not under the mutex.** `LiveSession` holds one mutex while polling, analyzing, writing CSV, and updating the track map. Do not add 100 Hz inference there. Score completed episodes or a bounded update stream outside the lock.
- **Keep the game-neutral boundary.** Adapters supply normalized measurements and capability masks. Any feature processing or inference belongs in the game-neutral core or a dedicated crate, not in `r3e-telemetry`.
- **Fallback by default.** Validate telemetry, channel availability, and model context before inference. On a failed load, unsupported class, missing required channel, assumed steering range, or changed game version, run the current analyzer and say so.
- **Version the artifact.** Record model version/hash, feature schema and units, training-data manifest, supported game/class/channel combinations, calibration method, and evaluation results. A model is a reviewed release, not silently changing behavior.
- **Keep explanations measured.** Distinguish "physics detector," "model-assisted," and "insufficient evidence" where trust depends on it. Do not present a ranking score as confidence or feature importance as a mechanical cause.

An accepted model-assisted decision should get an ADR covering its role, fallback, data policy, and evaluation gates. The unit normalization in §2.2 changes what the 30 deg/g threshold means and also needs an ADR (superseding part of ADR-8).

## 7. Fit with the roadmap

| Roadmap item | ML relevance | Suggested handling |
|---|---|---|
| M1.1 source identity (#3, done) | Provides car/class/track/channels | Persist identity, mask changes, and steering range with recordings, not only live status. |
| M1.3-M1.4 persistence, IDs (#5-6) | Protects the corpus and stable grouping | Keep provenance and rejected rows visible; fix the session-epoch + run-number filename collision before large-scale collection. |
| M1.5 polling cost (#7) | Establishes runtime headroom | Decide later whether inference is affordable; do not assume. |
| M2.2-M2.4 sessions, changes, lifecycle (#11-13) | Supplies real interventions and verified outcomes | Preserve actual values, comparable coverage, driver feedback, and status provenance. Without this, no ranking research. |
| M2.6 track maps (#15) | Place and feature context | Distinguish map data available at prediction time from future observations. |
| M2.7 notes and tags (#16) | Driver labels and reasons | Allow interval/pass links and uncertain labels. |
| M3.1-M3.2 fixtures and evaluation (#17-18) | Core research infrastructure | Replay with live parity (§2.3); match against labels at episode and report level; add split manifests and baseline/model reports. The threshold profiles from a file that #18 plans are all experiment A needs. |
| M3.3 detection evidence (#19) | Makes model inputs inspectable | Implement as the episode log (§2.1). |
| M3.4 severity per pass (#20) | Opportunity-normalized severity | Evaluate before adding any confidence score. |
| M3.6 three-track recordings (#22) | Starts the real corpus | Include clean driving, repeats, known-answer A/B/A runs, varied drivers/cars, and a held-out track. |
| M4 public alpha | Ships the deterministic product | Do not block on ML. Offer optional anonymized contributions only with clear consent. |
| M5.1/M5.3 profiles and comparison (#27/#29) | Makes ranking interpretable | Statistics, minimum detectable change, and per-corner tables first. Class profiles should store the steering ratio. |
| M5.4-M5.5 tyres (#30-31) | Confirms sensor semantics and targets | Consider forecasting only afterward. |
| M6.1 LMU (#32) | Adds a second domain | Evaluate separately; no mixed-game training without a domain-shift study. |

Steering-unit normalization (§2.2) is now roadmap item M3.7 (#35).

If adopted, each experiment should be its own issue and PR, following the existing one-issue-per-PR process. ML should not become a parallel platform project.

## 8. Proposed experiments

### Experiment A: does calibration fix the false alerts? (no ML)

**Question:** After unit normalization, do tuned global or class thresholds reduce false understeer and braking-understeer reports without missing genuine persistent understeer?

**Prerequisites:** episode log, replay parity, M3.1-M3.2, reviewed recordings from three tracks, and at least one A/B/A perturbation set.

**Method:**

1. Freeze the current analyzer as baseline 1. Add the unit fix as baseline 2.
2. Run `evaluate` with a coarse grid of threshold profiles (§3.1) on training/validation tracks.
3. Pick one global profile and, where data allows, partially pooled class profiles.
4. Report on the held-out track: episode, run-report, and advice metrics, the rule-of-three bound, and the perturbation tests.

**Deliverable:** A threshold profile PR with before/after `evaluate` output (as CONTRIBUTING requires for threshold changes), or a report that no profile helps.

### Experiment B: does a small model beat the tuned thresholds?

Run only if experiment A leaves a documented, specific error class (for example crest-adjacent false understeer at fast corners).

**Question:** Can L2 logistic regression over episode-log features remove that error class without hiding genuine persistent understeer?

**Method:**

1. Use A's best profile as the candidate generator and baseline. Collect positive, clean, crest-related, braking-related, and uncertain intervals with blind review, including calibration lead-in.
2. Use the episode, not the frame, as the unit.
3. Fit L2 logistic regression. Add a shallow-tree comparison only if the linear model leaves a clear gap.
4. Score the episodes the detector emits, and separately measure whole-recording recall so candidate-stage misses stay visible.
5. Evaluate on held-out sessions and an unseen track at all three levels (§5.3). Keep synthetic tests for understeer everywhere, sign changes, missing channels, crests, and clock resets as separate regressions. Restrict the first claim to one class, such as GT3.

**Deliverable:** A reproducible comparison report. A model artifact only if it clears the predeclared gates. No live behavior change is needed to answer the question.

**Stop conditions (both experiments):** weak label agreement, no robust gain over the stronger baseline, missed persistent understeer, failed perturbation tests, benefit confined to memorized tracks, or maintenance/runtime cost above demonstrated value. Keep the better deterministic baseline and the improved dataset.

## 9. Approaches to defer

- **Deep sequence models or telemetry transformers.** They need broad labeled sequences and are harder to validate, explain, and ship at this data scale. Revisit only after simple models hit a measured limit.
- **Reinforcement learning or autonomous setup search.** There is no controllable simulator loop, verified setup state, or cheap repeatable reward. Driver time and confounded lap times make online exploration unsuitable.
- **Bayesian optimization.** More plausible than RL for a few bounded parameters, but still needs actual setup values, comparable runs, and driver-approved experiments. Revisit after M5.
- **Anomaly detection as diagnosis.** An unusual lap can be faster, use a different line, or contain a bad sensor. Use it to select clips for review, not to recommend a setup change.
- **Per-driver personal models from a few laps.** Easy to learn a faulty baseline or overfit one track. Keep explicit profiles and robust calibration until independent data supports personalization.
- **Cloud inference, federated learning, or an ML service.** No demonstrated need. Local artifacts and offline experiments are a much smaller path.
- **End-to-end telemetry to optimal setup.** Missing controllable setup state, counterfactual outcomes, and a validated simulator interface. Do not pursue.

## 10. Bottom line

Setup Assistant is a good candidate for **small, physics-informed, locally executed models backed by real recording evidence**. The existing architecture already has the right seams: a game-neutral core, a data-driven rule book, explicit capability masks, a replayable recording format, and an explainable baseline.

The most valuable next steps are not ML: log episodes instead of only counting them, compare cars in road-wheel rather than steering-wheel units, and make replay reproduce live analysis. Then calibrate thresholds against blind-reviewed laps and known-answer setup changes. A model earns a place only for errors that remain after that.

The attractive long-term win is ranking changes from verified outcomes, but that data does not exist yet and cannot be manufactured from the current "Next run marks it fixed" behavior.

Neither win is established by this source review. Both must beat a tuned, explainable baseline on held-out real driving before they earn a place in the product.

## Review limits

This evaluation is based on source inspection of `main`: core analysis, rules, model, recording, live session, track map, demo, replay example, RaceRoom adapter and layout, frontend run handling, tests, roadmap, and ADRs. It did not run the app, benchmark inference, measure detector accuracy, or inspect any real recording. Threshold values and RaceRoom channel semantics (including whether `steer_lock_degrees` is road-wheel lock) remain unvalidated here. The steering-ratio and wheel-range percentages in §2.2 are arithmetic from the formula, not measurements. No model was trained, and no code, threshold, dependency, ADR, or roadmap item was changed.
