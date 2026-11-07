# How it works

Technical details behind detection, the track map, runs and recordings. For the big picture
see the [README](../README.md); for why things are the way they are, see [ADR.md](../ADR.md).

## Rules file

Each rule matches a behaviour (and optionally phases, a corner speed, aero-only, drivetrains)
and lists changes in the order to try them. `clicks` is for a slight problem and is multiplied
by severity (1/2/3) up to `max_clicks`. When two problems pull a parameter in opposite
directions, the stronger one wins and the card shows the conflict. Unknown fields are
rejected, so typos fail the `builtin_rules_are_valid` test.

## RaceRoom notes

- Setups live in `Documents\My Games\SimBin\RaceRoom Racing Experience\UserData\CarSetups\<track>\<layout>\*.svm`.
- They are encrypted (16-byte header: magic `0F B1 FF FF`, version 4, plaintext length,
  then an 8-byte block cipher in ECB mode with one fixed key). The app does not read or
  write them; you apply the changes in the in-game garage.
- Live data comes from RaceRoom's official shared memory (`$R3E`, Sector3's `r3e-api` repo).
  The layout in `crates/r3e-telemetry/src/layout.rs` is checked field by field against
  offsets computed from `r3e.h`. If RaceRoom bumps the major version (currently 3), the
  reader waits instead of reading garbage.
- Tyre temps come as left/centre/right. The reader assumes "left" is the car's left, so on
  left tyres the left edge is the outside. Worth confirming on track, since camber hints depend on it.

## How detection works

- **Understeer:** in gentle curves (0.08–0.3 g) every car turns exactly as its steering
  geometry says, whatever the setup. The analyzer learns that geometry there, then measures
  in real corners how much *extra* steering you need, in steering-wheel degrees per g. Above
  30°/g for 0.35 s counts as understeer. This number doesn't depend on corner radius, and a
  car that understeers everywhere still reads high. (The first version compared corners
  against the car's own medium-speed cornering, so an all-round understeering car looked
  "normal". That was the bug behind missed understeer.)
- **Understeer under braking:** when the front wheels turn more than 4% slower than the car
  while braking, the front tyres are busy braking and some understeer is physics, not the
  chassis. Those moments are reported as their own problem (brake bias, ABS, brake release)
  and shown as a separate "Under braking" balance row. On real laps this moved most "entry
  understeer" into this category: chassis entry balance read +5 to +8°/g, fronts-slipping
  moments +22 to +26°/g.
- **Crests:** the analyzer learns the vertical-acceleration reading on level straights (with
  or without gravity, either sign). When the load drops below 75% of that, the car counts as
  light for 0.3 s and its balance isn't judged.
- **Places:** every event stores its lap distance. Detections list where they happened, and
  a problem with at least 3 events, 70% of them within 80 m, is flagged as "one place only":
  maybe the track there (crest, bump, kerb) rather than the setup. Distances wrap at the start
  line, so events just before and just after it count as one place.
- **Oversteer:** countersteering while the body slides (slip > 2.5°), or slip > 6° on its own.
- **Lockups / wheelspin:** wheel surface speed vs. car speed under braking / throttle.
- **Bottoming:** ride height under 8 mm at speed (not verified against real scraping yet).
  With a track map, bottoming that mostly happens in compressions becomes its own problem
  ("Bottoming in compressions") with compression-specific advice.
- Each problem counts once per episode and is reported after it happened twice. Severity
  comes from how often it happens per minute of driving.
- The signs of steering and yaw don't matter; the analyzer learns how they relate.

## Track map

The app learns each track from your own laps and saves it per track and layout
(`%APPDATA%\dev.simracing.setupassistant\tracks`). Every 10 m it keeps elevation, speed
and cornering force. From that it finds:

- **corners** (T1, T2, …: cornering above 0.4 g), which you can name in the Live tab;
- **crests and compressions**, where the road's curvature at your speed adds or removes at
  least 0.3 g of load.

Once a lap is mapped, detections say where they happen ("most at T7 The Dipper · compression").
The map also remembers where problems happened in your last runs on that track. A problem at
the same spot in each recent run is flagged as recurring there. That's a hint, not proof that
it's the track: clean runs and what you changed aren't compared yet. This needs RaceRoom's car
position, which older recordings don't contain.

## Runs and history

A session is a series of runs. Tick "Tried" on the changes you make, then press **Next run**:
the ticked changes are saved with the problems they were for, the current problems count as
fixed, and live detection starts fresh for the changed car. From then on suggestions
(`RuleBook::suggest_with_history`):

- rank down anything that reverses an earlier change, cap it at half of what you changed,
  and say which change it undoes;
- warn when a change could bring back a fixed problem;
- note when you already went some clicks in the same direction.

If a "fixed" problem shows up again (detected or added by hand), it stops counting as fixed.
The session is kept across app restarts; **New session** clears it.

## Overlay

"Show overlay" in the Live tab opens a small always-on-top window with the balance needle,
corner phase and the latest detected event. Drag it into place, then "Lock" makes it
click-through. RaceRoom has to run in borderless or windowed mode; exclusive fullscreen
draws over every other window.

## Recordings and tuning

Real driving is recorded per run as CSV to the app data folder
(`%APPDATA%\dev.simracing.setupassistant\recordings`), about 1.5 MB per minute. Replay one to
see what the analyzer makes of it, optionally with a different understeer threshold:

```bash
cargo run -p setup-core --example replay -- path/to/session-…-run1.csv 25
```

All thresholds live in `Thresholds` in `telemetry.rs`. Most are still first guesses; the braking
split was checked against two real recordings (31 + 7 minutes, Mount Panorama, GT3).
