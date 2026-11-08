# Improvement ideas for Setup Assistant

Repository review: 2026-11-07. This is an analysis backlog, not an implementation plan or a set of accepted design decisions.

> **Status:** the decided plan built from this review is [roadmap.md](roadmap.md), with one
> GitHub issue per item. The quick fixes from #1, #2, #5, #11, #13 and #14 landed in PR #2
> (ADR-18). Kept here as background for the "why".

## Overall assessment

The project already has a useful end-to-end workflow: describe or detect a problem, get explained suggestions, record applied changes, and drive another run. Its strongest foundations are the game-agnostic Rust core, data-driven rule book, reusable synthetic demo, explicit architecture decisions, and browser preview backed by the real engine.

The biggest opportunity is to make the feedback loop more trustworthy: **know which car and run the evidence belongs to, explain how reliable it is, and verify whether a change helped.** More detectors and more games become substantially more valuable once those foundations are stronger.

### Scope and interpretation

- Reviewed architecture and contributor documentation, the rules and telemetry core, recordings, track learning, the RaceRoom adapter, Tauri commands, frontend, preview server, tests, and CI.
- Observations below come from source inspection. Runtime consequences are candidates for focused reproduction; detector accuracy and performance were not measured against real driving during this review.
- Keep the core game/UI agnostic, advice in data, and RaceRoom integration on the official telemetry API. Preserve the plain frontend's low setup cost.
- Threshold changes need recordings or a stated experimental reason. Changes to accepted design choices need a new ADR, particularly session ownership (ADR-10) and the meaning of a fixed problem (ADR-9).
- Effort estimates are relative: **S** = localized work, **M** = several components, **L** = substantial data/model/UI work. They are not delivery estimates.

## Priorities at a glance

| # | Idea | Priority | Effort |
|---|---|---|---|
| 1 | Make recording imports trustworthy | P1 | S–M |
| 2 | Handle missing channels, time resets, and stale data explicitly | P1 | M |
| 3 | Scope runs and history to their driving context | P1 | M |
| 4 | Verify fixes and record the changes actually made | P1 | M |
| 5 | Make track-persistence claims reflect the available evidence | P1 | M |
| 6 | Make persistence failures visible and recoverable | P1 | M |
| 7 | Build a real-recording validation workflow | P1 | M–L |
| 8 | Explain calibration, confidence, and detection evidence | P2 | M |
| 9 | Compare laps and runs fairly | P2 | L |
| 10 | Add car capabilities and parameter-aware advice | P2 | L |
| 11 | Validate rule coverage and explain ranking | P2 | M |
| 12 | Make tyre analysis context-aware | P2 | M–L |
| 13 | Stabilize learned track features and corner names | P2 | M |
| 14 | Make frontend state transitions resilient | P1 | M |
| 15 | Improve keyboard access and small-screen usability | P2 | S–M |
| 16 | Measure and reduce repeated live-report work | P2 | M |
| 17 | Add cross-layer tests and command-contract checks | P1 | M |
| 18 | Make recordings and replays usable from the app | P2 | M–L |
| 19 | Package reproducible releases for drivers | P2 | M |
| 20 | Expand adapters through explicit capabilities | P3 | L |

P1 protects the reliability of existing workflows. P2 improves usefulness and usability. P3 expands reach after the foundations are established.

## Reliability and evidence

### 1. Make recording imports trustworthy

**Evidence:** `crates/setup-core/src/recording.rs::read` skips the header and uses `filter_map` when parsing the first 36 columns. A malformed numeric field can disappear while later values shift left; with enough remaining values, the row is still accepted. The existing round-trip test in `src/lib.rs` covers a valid current-format recording.

**Idea:** Preserve column positions, validate required fields, and return import diagnostics with row/column locations. Read columns by header name, or explicitly recognize supported positional versions. Reject non-finite required values and distinguish an absent optional value from a malformed required one. Preserve compatibility with older recordings.

**Success criterion:** A bad speed, wheel-speed, or temperature field cannot silently become a different measurement. Tests cover old headers, truncated rows, malformed fields, extra columns, and non-finite values; imports report accepted and rejected row counts.

### 2. Handle missing channels, time resets, and stale data explicitly

**Evidence:** `Sample` in `crates/setup-core/src/telemetry.rs` derives zero-valued defaults, although negative values mean unknown for ride height and lap distance. Zero ride height can satisfy the bottoming condition. `Analyzer::push` rejects all timestamps at or below the last timestamp, so a simulation-clock reset can suspend analysis until time catches up. `LiveSession` changes connection state without clearing the analyzer's last frame, which both frontend windows still render.

**Idea:** Define channel availability and validity at the adapter boundary, with explicit unknown defaults or optional measurements. Separate a duplicate frame from a new simulation epoch; clear appropriate transient state on a restart. Expose sample age and suppress or visibly age stale gauges. Give unsupported API versions a distinct diagnostic instead of the same state as a game that is not running.

**Success criterion:** Missing ride heights do not produce bottoming; a reconnect with time reset resumes promptly; NaN/invalid samples cannot poison running averages; paused or disconnected telemetry is visibly stale. Also correct the `lat_velocity` field comment from m/s² to m/s so future adapters use the right units.

### 3. Scope runs and history to their driving context

**Evidence:** `CarContext` in `crates/setup-core/src/model.rs` contains only drivetrain and aero. `Source` exposes track information but no vehicle/session identity. `LiveSession::set_track` switches maps without starting a new analyzer or recording. `app/ui/main.js` stores one global session, and CSV files contain samples without car, track, or rule/threshold metadata.

**Idea:** Introduce game-neutral context identifiers for source, car, track/layout, and session. Associate recordings, applied changes, and comparisons with that context. Define transitions for car switches, track switches, game restarts, and demo mode. Add a versioned recording sidecar containing context, app/rule version, and threshold settings. Keep UI session ownership consistent with ADR-10 unless explicitly superseded.

**Success criterion:** Switching cars or tracks cannot combine incompatible balance data or apply another car's setup history. Demo exploration has its own history. A recording can be understood and reproduced without asking its owner which session it came from.

### 4. Verify fixes and record the changes actually made

**Evidence:** `app/ui/main.js::nextRun` marks all current and detected problems fixed immediately, including when the driver starts another run without ticking any changes. A Tried checkbox records the suggestion's full click count. The rules engine then uses these presumed fixes to penalize future recommendations.

**Idea:** Distinguish attempted, awaiting verification, improved, resolved, and still present. Let drivers enter the actual click count, log a manual change, and amend a mistaken entry. Treat absence of detection as evidence only after enough comparable cornering; retain driver confirmation for subjective problems. Offer a compact experiment note: what changed, what it targeted, and what happened.

**Success criterion:** Starting a run alone does not certify a fix. Applying one click of a three-click recommendation records one click. History can express partial improvement without either forgetting the original problem or claiming it is solved. This would revise ADR-9's current fixed-problem semantics.

### 5. Make track-persistence claims reflect the available evidence

**Evidence:** `crates/setup-core/src/track.rs::RunSummary` records only behavior and distance. `add_run` skips runs with no located detections. `annotate` compares recent stored summaries, while `app/ui/main.js::whereText` says the problem persisted "whatever you changed" and sometimes "that's the track there." The summary does not contain applied changes, car identity, or evidence that the driver revisited the spot in a clean run.

**Idea:** Preserve clean runs and their coverage, context, and relevant setup changes. Distinguish recurring location from demonstrated track influence. Use wording such as "recurs at this location" until stronger evidence exists; a persistent setup or driving issue can also recur at one corner. Compare only relevant runs and include phase/speed context where useful.

**Success criterion:** A clean comparable run breaks the consecutive-recurrence claim. Unchanged setups and unrelated cars do not support a claim that changes failed. A spot that was not driven is treated as unknown, not clean.

### 6. Make persistence failures visible and recoverable

**Evidence:** In `crates/setup-core/src/live.rs`, recording writes and several flushes discard errors; map errors go to stderr. The telemetry worker has no explicit stop/join path. `TrackMap::save` writes directly to the destination, and `load_or_new` silently falls back after a read/parse failure. Frontend `save` and `load` catch storage failures without feedback. The Windows release build hides the console.

**Idea:** Add structured recording/map/session health to status, with actionable UI feedback and retry behavior. Save maps through a recoverable replacement strategy and validate loaded structure, not just track length. Add session schema migrations and export/restore. Give the worker an explicit lifecycle that flushes recordings and saves the latest map/run on normal shutdown. Use collision-resistant recording IDs and avoid truncating an existing recording when run numbers are reused.

**Success criterion:** Disk-full, unwritable-directory, corrupted-map, and storage-quota scenarios produce useful feedback. A failed save preserves the last usable map, and normal app shutdown retains the final buffered data.

### 7. Build a real-recording validation workflow

**Evidence:** `crates/setup-core/src/lib.rs` has valuable exact-detection synthetic tests. `demo.rs` generates idealized 100 Hz data; ADR-15 explicitly notes that this proves logic rather than threshold accuracy. `examples/replay.rs` accepts one recording and one optional understeer threshold, then prints aggregate results. No real-recording fixture corpus is present in the inspected source tree.

**Idea:** Curate small, permissioned recording excerpts with car/track context and driver annotations, including ordinary driving with no expected problems. Extend replay analysis to batch evaluation, machine-readable output, configurable threshold profiles, and event-time/location comparison. Measure both missed detections and false positives; reserve some cars/tracks for evaluation rather than tuning.

**Success criterion:** Each proposed detector adjustment reports before/after results on named recordings, including negative cases. Synthetic edge cases cover sample-rate changes, gaps, noise, sign conventions, and clock resets. Keep the required demo case and exact-detection test for every new detector.

## Better setup decisions

### 8. Explain calibration, confidence, and detection evidence

**Evidence:** `LiveReport` exposes calibration as a boolean. Severity in `Analyzer::report` is based on events per driving minute, with a one-minute denominator floor. `PhaseBalance` contains observation seconds, but the UI primarily shows a mean. Events retain counts and locations rather than a full evidence record. Wheelspin uses the fastest of all wheels, without driven-wheel context.

**Idea:** Expose calibration progress and why samples are rejected, distinguish weak evidence from a neutral car, and show measurement coverage per phase. Retain event duration, peak measurement, channel availability, and relevant wheel/axle details. Evaluate severity by eligible corner/braking opportunities or time exposed, as well as raw frequency. Show this as supporting evidence, not an unvalidated probability score.

**Success criterion:** Drivers can answer "why did this appear?" and "what should I drive to verify it?" A long straight does not make repeated trouble at every hairpin look solved. The underlying severity model is evaluated against recordings before replacement.

### 9. Compare laps and runs fairly

**Evidence:** `LiveStatus::previous_balance` retains only the previous run's phase averages in memory. Track summaries store locations, and the recording schema has no explicit lap validity or conditions. README already identifies lap/run comparison as a next step.

**Idea:** Persist run summaries and compare the same corners at similar speeds, with observation duration, event frequency, balance, and lap/sector time where available. Identify warm-up, out-laps, traffic, and invalid laps where telemetry or driver annotations permit. Include tyre/fuel/condition context when available and say when runs are not comparable.

**Success criterion:** The app can say "entry balance improved at these corners, but exit wheelspin increased" with supporting observations. It avoids attributing a faster lap to a setup change when there is too little comparable evidence. Depends on context metadata and verified change history (#3–4).

### 10. Add car capabilities and parameter-aware advice

**Evidence:** `CarContext` cannot indicate whether a car has adjustable TC, ABS, dampers, or differential settings. `gt.json` contains those recommendations, and `ParamInfo` describes directions but not units, limits, step sizes, or current values.

**Idea:** Add data-driven car/class profiles for available adjustments, garage labels, direction conventions, click increments, and bounds. Allow manual current-value entry for RaceRoom and hide unavailable adjustments. Use telemetry vehicle identity to suggest a profile while allowing correction. Link wheelspin evidence to driven wheels and distinguish inside-wheel spin from both driven wheels losing traction before choosing differential advice.

**Success criterion:** A car without TC never receives a TC adjustment; a parameter at its limit gets an appropriate explanation. Drivers see the actual garage control and realistic step size. RaceRoom setup files remain outside this workflow, consistent with ADR-1.

### 11. Validate rule coverage and explain ranking

**Evidence:** `RuleBook::validate` checks references, speed values, and click bounds, but not duplicate IDs, incompatible phases, or the full supported symptom/context matrix. The fast mid-corner understeer and oversteer rules require aero; a specifically fast mid-corner problem for a no-aero car has no matching fallback. `merge` chooses Increase on equal opposing weights, producing a zero-score conflicted suggestion. General and specific rules can both contribute votes.

**Idea:** Add coverage checks across behavior, phase, speed, drivetrain, and capabilities; explicitly document unsupported combinations. Detect duplicate IDs, empty explanations/changes, incompatible phases, and unintended overlap. Define a deliberate response to ties and low-evidence conflicts. Include rule IDs and ranking contributions in a diagnostic explanation so contributors can understand why advice moved up or down.

**Success criterion:** Every selectable problem yields applicable advice or a clear explanation of the gap. Tied contradictory requests do not look like a confident top recommendation. Rule-data edits have focused behavioral examples as well as schema validation.

### 12. Make tyre analysis context-aware

**Evidence:** `Analyzer::tyre_report` uses fixed spread thresholds and a 30-second driving gate. Temperature and pressure averaging are coupled to all tyre temperatures being available. `app/ui/main.js::tempColor` uses a universal temperature scale. The adapter's inner/outer interpretation is documented as unconfirmed on track.

**Idea:** Validate channel semantics with real data first. Track tyre warm-up/stability and valid data independently per tyre/channel; show hot-pressure trends and per-lap temperature spreads. Add compound/class-specific targets only when supported by evidence. Move configurable advice and explanations into rule/profile data in the spirit of ADR-4, with a path to add an actionable tyre problem to the normal suggestion workflow.

**Success criterion:** One unavailable temperature sensor does not hide all pressure information. A short cold run does not produce confident camber advice. Any "too hot" or "too high pressure" label identifies the profile and observation window behind it.

### 13. Stabilize learned track features and corner names

**Evidence:** `telemetry.rs::cluster` and persistence matching use linear distance, so nearby events on opposite sides of the start line are separated. `TrackMap::corners` regenerates numbered segments from learned lateral force, while user names are keyed by number. Track bins combine elevation with speed and cornering observations across runs and cars.

**Idea:** Use circular track distance for clustering and boundary features. Anchor named corners to stable location identities so newly discovered segments do not reassign names. Separate reusable geometry from car/run-dependent speed and load estimates. Account for teleports, incomplete laps, unusual driving lines, and feature confidence; support corrections and exportable named-corner notes.

**Success criterion:** An event just before and after the start line is recognized as the same place. Learning an extra corner does not move existing names to the wrong corners. Returning in a much faster car does not silently reinterpret old speed-dependent features as universal geometry.

## Application and development experience

### 14. Make frontend state transitions resilient

**Evidence:** `app/ui/main.js` owns persistence, command calls, run transitions, polling, and rendering in one file. `nextRun` mutates local state before the backend call succeeds; repeated clicks are not guarded. `refresh` has no request sequencing, so older suggestions can render after newer ones. Run sequencing exists for some live polling, providing a useful starting pattern. Render cache keys omit some visible inputs: previous balance values and track-profile contents, for example.

**Idea:** Split the plain JavaScript into small session, command-client, and view modules. Make run/session actions single-flight, commit or recover state consistently after command results, and ignore stale suggestion responses. Validate persisted session structure and show recoverable command errors. Use explicit data revisions or complete render dependencies to prevent stale views, and clear cached state on context transitions.

**Success criterion:** Rapid changes render advice for the latest selection; a failed Next run preserves a recoverable session; double-clicking advances once. Updating previous-run values or switching tracks refreshes the relevant view. This can retain the no-bundler/no-framework approach of ADR-2.

### 15. Improve keyboard access and small-screen usability

**Evidence:** `app/ui/index.html` uses clickable SVG groups for corner phases, with no equivalent keyboard controls. Segmented selections and tabs primarily indicate state through CSS classes. The Tauri window minimum is 1000×660, and `styles.css` has no responsive media queries or reduced-motion preference handling.

**Idea:** Add keyboard-operable phase selection, semantic selected states for tabs/segmented controls, visible focus, and polite announcements for important action results. Reflow to a single column at narrow widths and test OS scaling. Respect reduced-motion preferences. Persist overlay placement and make click-through/connection state easy to understand from the main window.

**Success criterion:** A driver can complete describe → suggest → log change → next run entirely by keyboard. At 150–200% scaling, controls remain reachable without horizontal clipping. Important states remain understandable without relying only on color.

### 16. Measure and reduce repeated live-report work

**Evidence:** `LiveSession` holds one mutex during polling, analysis, CSV writes, periodic map saves, and report generation. `status` annotates reports and builds a track view; annotation repeatedly derives corners/features. The main Live tab requests status about every 150 ms, and the overlay requests the same full status about every 100 ms. `event_places` retains every event position for the entire run.

**Idea:** Measure polling jitter, lock hold time, report-generation cost, payload size, and memory growth with both windows open during a long run. If warranted, separate a lightweight live snapshot from slower map/report updates, cache derived track data by revision, and move disk work outside the analysis lock. Bound or incrementally aggregate event locations while retaining detailed evidence in recordings.

**Success criterion:** Establish and meet measured latency/memory targets on a representative machine. A long session or slow map save does not interrupt telemetry acquisition. Avoid introducing a more complex concurrency model until profiling justifies it.

### 17. Add cross-layer tests and command-contract checks

**Evidence:** `.github/workflows/ci.yml` already runs Rust formatting, Clippy, and tests across Linux core/Windows app jobs. Existing tests mostly cover algorithms and adapter layout. Tauri command handling and the preview dispatch in `examples/ui_preview.rs` are maintained separately, with different argument-validation behavior in some branches. There is no browser workflow test in CI.

**Idea:** Add a small browser smoke suite through the existing preview: select a symptom, observe ranked advice, tick a change, start a run, reload history, and exercise demo mode. Add targeted integration tests for recording failure, reconnect, track/car changes, and persistence recovery. Define shared request/response contracts or parity checks for commands supported in both environments, keeping app-specific transport code outside the domain core.

**Success criterion:** A frontend/backend payload mismatch or broken session transition fails a focused test. Preview rejects invalid arguments consistently with desktop commands. A small desktop smoke check covers overlay behavior that preview cannot validate.

### 18. Make recordings and replays usable from the app

**Evidence:** Recordings are automatic, `recordings_dir` is exposed by the backend but unused by `main.js`, and replay is currently a command-line example. The documentation estimates approximately 1.5 MB of storage per driving minute. The UI has no recording library or retention controls.

**Idea:** Add recording status, an open-folder action, and a local library grouped by car/track/session with duration and size. Offer configurable retention, selective deletion, export with context metadata, and in-app replay with a timeline and event markers. Reuse a recording-backed `Source` where practical, while ensuring replay cannot modify the current live run's history or learned map unintentionally.

**Success criterion:** A driver can find, replay, and share a specific run without terminal commands. They can see and manage storage use. A contributor can reproduce the same analysis using the exported recording and metadata.

### 19. Package reproducible releases for drivers

**Evidence:** README requires a source build and lists installers as future work. Tauri bundling is enabled, but the only workflow is CI. The workspace declares no `rust-version`, while CONTRIBUTING advertises Rust 1.77 or newer. CI tests current stable rather than that minimum. App version appears in both the Cargo workspace and Tauri configuration.

**Idea:** Produce versioned Windows release bundles with changelogs and a fresh-install smoke check. Verify the actual minimum supported Rust version against the dependency lockfile, declare it, and either test it or document the current supported toolchain. Check release-version consistency and document how local session/map data survives upgrades.

**Success criterion:** A driver can install and try the demo without Rust or Visual Studio. A release can be traced to its source, dependency lockfile, rule version, and detector configuration. Build instructions reflect a toolchain that CI actually verifies.

### 20. Expand adapters through explicit capabilities

**Evidence:** `setup_core::live::Source` is a useful game-neutral seam, but it currently reports only polling results and track information. `R3eSource`, UI connection text, and the game badge are wired specifically for RaceRoom. Non-Windows telemetry is a stub.

**Idea:** After context/availability work (#2–3), expose source identity, supported channels, vehicle context, and useful connection errors through a game-neutral interface. Pilot one additional adapter with recorded fixtures and a clear support matrix. Treat a Proton bridge as a separate feasibility experiment, and evaluate setup-file integration for other games only through their supported formats and an explicit design decision.

**Success criterion:** A second source can use the existing analysis and UI without game-specific conditionals in the core. Missing channels disable only dependent features and are explained to the driver. Adapter conversion tests establish units and capabilities before live deployment.

## Suggested order of exploration

1. **Protect current results:** recording parsing, missing-channel/time-reset handling, persistence diagnostics, and frontend transition reliability (#1–2, #6, #14).
2. **Make history meaningful:** context identity, accurate change logging, verified outcomes, and honest recurrence claims (#3–5).
3. **Establish evidence:** real-recording evaluation and cross-layer tests (#7, #17), then calibration/evidence displays and rule coverage (#8, #11).
4. **Improve the tuning loop:** fair comparisons, car profiles, tyre context, and stable track learning (#9–10, #12–13).
5. **Broaden usability and reach:** accessibility, measured performance work, recording tools, installers, and a second adapter (#15–16, #18–20).

Useful first investigations are a malformed-CSV reproduction, a game-clock-reset sequence, a no-change Next run, a clean run between two problematic runs, and a fast mid-corner symptom with aero disabled. Each is small enough to clarify an existing behavior before committing to a larger redesign.
