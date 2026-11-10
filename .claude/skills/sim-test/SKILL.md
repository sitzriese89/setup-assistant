---
name: sim-test
description: Plan and log driving tests of Setup Assistant in RaceRoom. Use when the user is about to drive ("going on track", "what should I test?"), comes back from driving ("did a run", "finished my session"), or asks what has or hasn't been tested yet. Keeps docs/sim-testing.md up to date so the next Claude session knows the state.
---

# Sim test tracking

`docs/sim-testing.md` is the single source of truth for driving tests. It has three parts:

- **To test**: open checks, each with where it came from (PR/issue), what to do on track and
  what counts as a pass.
- **Verified**: checks that passed, with the date and the evidence (recording, numbers).
- **Session log**: one entry per driving session, newest first.

Always read that file first. Never keep test state only in the conversation.

## Pick the mode

- Argument `plan`, or the user is about to drive → **Before driving**.
- Argument `report`, or the user just drove → **After driving**.
- No argument: list the recordings folder. If it has a recording newer than the last session
  log entry, do **After driving**; otherwise **Before driving**.

Recordings: `%APPDATA%\dev.simracing.setupassistant\recordings\session-<id>-run<n>.csv`
(Git Bash: `"$APPDATA/dev.simracing.setupassistant/recordings"`). Track maps with track
names: `.../tracks/r3e-<track>-<layout>.json` (`info.name`).

## Before driving

1. Check for work merged since the newest session entry:
   `git log --merges --oneline <last logged commit>..origin/main` (fetch first). For each merged
   PR that changes what the app shows or detects, add **To test** items. Each item needs a
   pass criterion the user can judge from the car or Claude can judge from a recording.
2. Make sure the local checkout is on `main` and up to date, then give the user:
   - the build command (`cargo run -p setup-assistant`) and the reminders below;
   - the open **To test** items, grouped by what the user must do themselves (look at the
     screen, feel the car, drive a certain car or track) versus what Claude checks afterwards
     from the recording. Keep it to what fits one session; say which car/track helps.
3. Write any new items to the file. Don't commit unless asked.

Reminders for every session:
- Close any older app window; start with **New session** unless continuing a test (next to
  **Next run**; click twice).
- Drive 3–4 laps before changing the setup.
- While driving only the overlay is visible: check anything in the Live tab while in the pits.
- After each garage visit: tick **Tried** on every change made, then press **Next run** before
  driving again. One run = one setup.

## After driving

1. Ask (one AskUserQuestion round, skip what the user already said): car, track, what they
   changed and when, what they felt, and anything odd on screen. Their impressions are evidence
   too; record them in their words.
2. Find the new recordings (newer than the last logged session). For each run, replay it:
   `cargo run -q -p setup-core --example replay -- "<file>"`
   Note samples/minutes, wheel range, steering lock and ratio, the balance table and the
   detections with places. If a session's files don't start at run1, the app continued an
   older session (no New session pressed): the run number came from the saved UI state, so
   map the driver's run numbers to the files before talking about runs.
3. If a run is long or the user changed things mid-run, split it into slices and replay each
   (header + equal row chunks into the scratchpad) to see what changed over time.
4. Look up flagged places in the track map: bin index = distance / 10 m, with speed (m/s) and
   lat_g. Name corners only when sure; otherwise give distance and speed.
5. Check each **To test** item this session could answer. Move passes to **Verified** with
   evidence. Leave failures in **To test** with what was seen, and suggest an issue for real
   bugs (ask before filing). Add new items for anything surprising.
6. Add a **Session log** entry (template in the file). Then tell the user, outcome first: what
   passed, what failed, what's still open, and what to test next time.
7. Don't commit or post to GitHub unless asked.

## Rules

- Evidence over impressions: a pass needs a recording number or a clear on-screen
  observation, and says which.
- Don't change detection thresholds because of one session (AGENTS.md). Record the numbers;
  threshold changes need their own issue.
- Keep entries short. The file is read at the start of every test conversation.
