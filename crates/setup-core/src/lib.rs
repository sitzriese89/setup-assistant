//! Sim-agnostic core: what the driver feels -> what to change in the setup.

pub mod demo;
pub mod live;
pub mod model;
pub mod recording;
pub mod rules;
pub mod telemetry;
pub mod track;

pub use model::*;
pub use rules::{BehaviorInfo, ParamInfo, RuleBook};

#[cfg(test)]
mod telemetry_tests {
    use super::*;
    use telemetry::{Analyzer, LiveReport, Sample};

    fn run(laps: u32, tweak: impl Fn(&mut Sample)) -> LiveReport {
        run_variant(laps, demo::Variant::Mixed, tweak)
    }

    fn run_variant(laps: u32, variant: demo::Variant, tweak: impl Fn(&mut Sample)) -> LiveReport {
        let mut a = Analyzer::default();
        let mut t0 = 0.0;
        for _ in 0..laps {
            let lap = demo::lap_variant(t0, variant);
            t0 = lap.last().unwrap().time + 0.01;
            for mut s in lap {
                tweak(&mut s);
                a.push(&s);
            }
        }
        a.report()
    }

    fn found(r: &LiveReport, behavior: &str, phase: Option<Phase>, speed: CornerSpeed) -> bool {
        r.detected
            .iter()
            .any(|d| d.symptom.behavior == behavior && d.symptom.phase == phase && d.symptom.speed == speed)
    }

    #[test]
    fn demo_lap_problems_are_detected() {
        let r = run(3, |_| {});
        let list: Vec<_> =
            r.detected.iter().map(|d| (&d.symptom.behavior, d.symptom.phase, d.symptom.speed, d.count)).collect();
        assert!(r.calibrated);
        assert!(found(&r, "understeer", Some(Phase::Mid), CornerSpeed::Slow), "{list:?}");
        assert!(found(&r, "oversteer", Some(Phase::Exit), CornerSpeed::Slow), "{list:?}");
        assert!(found(&r, "front_lockup", None, CornerSpeed::Both), "{list:?}");
        assert!(found(&r, "wheelspin", Some(Phase::Exit), CornerSpeed::Both), "{list:?}");
        assert!(found(&r, "braking_understeer", None, CornerSpeed::Both), "{list:?}");
        assert!(found(&r, "bottoming", None, CornerSpeed::Both), "{list:?}");
        // Nothing that isn't in the demo lap: no entry understeer (that was braking),
        // no fast mid-corner understeer (that was a crest).
        assert_eq!(r.detected.len(), 6, "unexpected detections: {list:?}");
        for d in &r.detected {
            // The hairpin comes before enough gentle curves were seen on lap 1.
            let expected = if d.symptom.behavior.contains("understeer") { 2 } else { 3 };
            assert_eq!(d.count, expected, "one event per lap expected: {list:?}");
        }
    }

    /// The reported bug: after fixing oversteer the car pushes everywhere.
    /// A car that understeers in every corner must still be detected.
    #[test]
    fn understeer_everywhere_is_detected() {
        let r = run_variant(3, demo::Variant::Pushing, |_| {});
        let list: Vec<_> =
            r.detected.iter().map(|d| (&d.symptom.behavior, d.symptom.phase, d.symptom.speed, d.count)).collect();
        for (phase, speed) in
            [(Phase::Mid, CornerSpeed::Slow), (Phase::Mid, CornerSpeed::Fast), (Phase::Exit, CornerSpeed::Slow)]
        {
            assert!(found(&r, "understeer", Some(phase), speed), "{phase:?} {speed:?}: {list:?}");
        }
        assert!(r.detected.iter().all(|d| d.symptom.behavior == "understeer"), "{list:?}");
        let mid = r.balance_by_phase.iter().find(|b| b.phase == Phase::Mid && b.speed == CornerSpeed::Slow).unwrap();
        assert!((mid.deg_per_g - 45.0).abs() < 3.0, "measured {} deg/g, built in 45", mid.deg_per_g);
    }

    #[test]
    fn crests_are_not_counted_as_understeer() {
        let r = run(3, |_| {});
        assert!(!found(&r, "understeer", Some(Phase::Mid), CornerSpeed::Fast));
        // The same pushing without the crest signal would be flagged.
        let r = run(3, |s| s.vert_accel = 9.81);
        assert!(found(&r, "understeer", Some(Phase::Mid), CornerSpeed::Fast));
    }

    #[test]
    fn braking_understeer_needs_slipping_fronts() {
        // Same steering, but the front wheels roll freely: then it's chassis understeer on entry.
        let r2 = run(3, |s| {
            s.wheel_speed[0] = s.speed;
            s.wheel_speed[1] = s.speed;
        });
        assert!(found(&r2, "understeer", Some(Phase::Entry), CornerSpeed::Slow));
        assert!(!found(&r2, "braking_understeer", None, CornerSpeed::Both));
    }

    #[test]
    fn one_place_problems_are_flagged() {
        // Mixed: exit oversteer only ever happens at corner 4.
        let r = run(3, |_| {});
        let os = r.detected.iter().find(|d| d.symptom.behavior == "oversteer").unwrap();
        assert!(os.one_place, "{:?}", os.places);
        // Pushing car: understeer in every slow corner, so not one place.
        let r = run_variant(3, demo::Variant::Pushing, |_| {});
        let us = r
            .detected
            .iter()
            .find(|d| {
                d.symptom.behavior == "understeer"
                    && d.symptom.phase == Some(Phase::Mid)
                    && d.symptom.speed == CornerSpeed::Slow
            })
            .unwrap();
        assert!(!us.one_place, "{:?}", us.places);
        assert!(us.places.len() >= 3);
    }

    fn demo_map(laps: u32) -> track::TrackMap {
        let mut map = track::TrackMap::new(demo::track_info());
        let mut t0 = 0.0;
        for _ in 0..laps {
            let lap = demo::lap(t0);
            t0 = lap.last().unwrap().time + 0.01;
            for s in &lap {
                map.add(s);
            }
        }
        map
    }

    #[test]
    fn track_map_finds_corners_crest_and_compression() {
        let map = demo_map(2);
        assert!(map.ready(), "coverage {}", map.coverage());
        let corners = map.corners();
        assert_eq!(corners.len(), 5, "{corners:?}");
        let features = map.features();
        let kinds: Vec<_> = features.iter().map(|f| f.kind).collect();
        assert_eq!(kinds.iter().filter(|k| **k == track::FeatureKind::Crest).count(), 1, "{features:?}");
        assert_eq!(kinds.iter().filter(|k| **k == track::FeatureKind::Compression).count(), 1, "{features:?}");
        // The compression is in the braking zone before T4.
        let dip = features.iter().find(|f| f.kind == track::FeatureKind::Compression).unwrap();
        assert_eq!(map.describe(dip.at_m), "before T4 · compression");
    }

    #[test]
    fn bottoming_in_compressions_gets_its_own_advice() {
        let map = demo_map(2);
        let mut r = run(3, |_| {});
        map.annotate(&mut r);
        let b = r.detected.iter().find(|d| d.symptom.behavior.starts_with("bottoming")).unwrap();
        assert_eq!(b.symptom.behavior, "bottoming_compression");
        assert!(b.places[0].label.contains("compression"), "{}", b.places[0].label);
    }

    #[test]
    fn same_spot_every_run_is_flagged_as_track() {
        let mut map = demo_map(2);
        let r = run(3, |_| {});
        map.add_run(&r);
        map.add_run(&r);
        let mut now = run(3, |_| {});
        map.annotate(&mut now);
        let lock = now.detected.iter().find(|d| d.symptom.behavior == "front_lockup").unwrap();
        assert!(lock.persistent, "{:?}", lock.places);
        // A fresh map without history can't say that.
        let mut fresh = run(3, |_| {});
        demo_map(2).annotate(&mut fresh);
        assert!(fresh.detected.iter().all(|d| !d.persistent));
    }

    #[test]
    fn loose_car_shows_oversteer_not_understeer() {
        let r = run_variant(3, demo::Variant::Loose, |_| {});
        let list: Vec<_> =
            r.detected.iter().map(|d| (&d.symptom.behavior, d.symptom.phase, d.symptom.speed, d.count)).collect();
        assert!(found(&r, "oversteer", Some(Phase::Exit), CornerSpeed::Slow), "{list:?}");
        assert!(found(&r, "unstable_braking", None, CornerSpeed::Both), "{list:?}");
        assert!(!r.detected.iter().any(|d| d.symptom.behavior == "understeer"), "{list:?}");
        let mid = r.balance_by_phase.iter().find(|b| b.phase == Phase::Mid && b.speed == CornerSpeed::Slow).unwrap();
        assert!(mid.deg_per_g < 0.0, "loose car should read below zero, got {}", mid.deg_per_g);
    }

    #[test]
    fn recordings_replay_the_same() {
        let dir = std::env::temp_dir().join(format!("setup-core-test-{}", std::process::id()));
        let path = dir.join("lap.csv");
        let mut rec = recording::Recorder::create(&path).unwrap();
        let lap = demo::lap(0.0);
        for s in &lap {
            rec.write(s).unwrap();
        }
        rec.flush().unwrap();
        let back = recording::read(&path).unwrap();
        assert_eq!(back.len(), lap.len());
        assert!((back[500].steer - lap[500].steer).abs() < 1e-3);
        assert_eq!(back[500].tyre_temp, lap[500].tyre_temp);
        assert!((back[500].lap_distance - lap[500].lap_distance).abs() < 0.01);
        assert!((back[500].vert_accel - lap[500].vert_accel).abs() < 1e-3);
        assert!((back[500].elevation.unwrap() - lap[500].elevation.unwrap()).abs() < 1e-2);
        assert!((back[500].steer_lock_deg - lap[500].steer_lock_deg).abs() < 1e-3);
        assert!(back[500].steering_ratio().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sign_conventions_dont_matter() {
        // A game that reports steering with the opposite sign and mirrored lateral values.
        let r = run(3, |s| {
            s.steer = -s.steer;
            s.lat_accel = -s.lat_accel;
            s.lat_velocity = -s.lat_velocity;
        });
        assert!(found(&r, "understeer", Some(Phase::Mid), CornerSpeed::Slow));
        assert!(found(&r, "oversteer", Some(Phase::Exit), CornerSpeed::Slow));
        assert_eq!(r.detected.len(), 6);
    }

    #[test]
    fn tyre_hints_point_at_the_right_tyres() {
        let r = run(2, |_| {});
        let hints = |name: &str| r.tyres.iter().find(|t| t.name == name).unwrap().hints.join(" ");
        assert!(hints("FL").contains("camber"), "FL: {}", hints("FL"));
        assert!(hints("RL").contains("pressure may be too high"), "RL: {}", hints("RL"));
        assert!(hints("FR").is_empty(), "FR: {}", hints("FR"));
    }

    #[test]
    fn detections_feed_the_rules_engine() {
        let r = run(3, |_| {});
        let book = RuleBook::builtin();
        let car = CarContext { drivetrain: Drivetrain::Rwd, has_aero: true };
        let symptoms: Vec<_> = r.detected.iter().map(|d| d.symptom.clone()).collect();
        let out = book.suggest(&car, &symptoms);
        assert!(out.iter().any(|s| s.param == "tc"), "traction control should come up for exit oversteer/wheelspin");
    }

    #[test]
    fn game_clock_reset_resumes_analysis() {
        // Three laps, then the game restarts the session and its clock starts at 0 again.
        let mut a = Analyzer::default();
        for session in 0..2 {
            let mut t0 = 0.0;
            for _ in 0..3 {
                let lap = demo::lap(t0);
                t0 = lap.last().unwrap().time + 0.01;
                lap.iter().for_each(|s| a.push(s));
            }
            let lockups = a.report().detected.iter().find(|d| d.symptom.behavior == "front_lockup").map(|d| d.count);
            assert_eq!(lockups, Some(3 * (session + 1)), "session {session}");
        }
    }

    #[test]
    fn missing_ride_height_is_not_bottoming() {
        assert!(Sample::default().ride_height.iter().all(|h| *h < 0.0));
        assert!(Sample::default().lap_distance < 0.0);
        let r = run(3, |s| s.ride_height = Sample::default().ride_height);
        assert!(!found(&r, "bottoming", None, CornerSpeed::Both));
        assert_eq!(r.detected.len(), 5);
    }

    #[test]
    fn bad_recording_rows_are_rejected_not_shifted() {
        let dir = std::env::temp_dir().join(format!("setup-core-csv-{}", std::process::id()));
        let path = dir.join("bad.csv");
        let mut rec = recording::Recorder::create(&path).unwrap();
        rec.write(&demo::lap(0.0)[500]).unwrap();
        rec.flush().unwrap();
        drop(rec);
        let text = std::fs::read_to_string(&path).unwrap();
        let (header, good) = text.trim_end().split_once('\n').unwrap();
        let cols: Vec<&str> = good.split(',').collect();
        let with = |i: usize, v: &str| {
            let mut c = cols.clone();
            c[i] = v;
            c.join(",")
        };
        let rows = [
            good.to_string(),
            with(2, "x"),            // malformed throttle
            with(1, "NaN"),          // non-finite speed
            cols[..20].join(","),    // truncated
            cols[..34].join(","),    // oldest layout: still fine
            with(36, ""),            // no elevation: fine
            format!("{good},extra"), // extra column: ignored
            cols[..37].join(","),    // before the steering lock column: fine
            with(37, "y"),           // malformed steering lock
        ];
        std::fs::write(&path, format!("{header}\n{}\n", rows.join("\n"))).unwrap();
        let r = recording::read_checked(&path).unwrap();
        assert_eq!(r.samples.len(), 5);
        assert_eq!(r.rejected, 4);
        assert!(r.first_problem.as_deref().unwrap().contains("throttle"), "{:?}", r.first_problem);
        assert!(r.samples[1].lap_distance < 0.0, "oldest layout has no lap distance");
        assert!(r.samples[2].elevation.is_none());
        assert!((r.samples[3].speed - r.samples[0].speed).abs() < 1e-6);
        assert!(r.samples[0].steer_lock_deg > 0.0);
        assert_eq!(r.samples[4].steer_lock_deg, 0.0, "37-column layout has no steering lock");
        assert!(r.samples[4].steering_ratio().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn places_across_the_start_line_are_one_place() {
        let events = [5.0, 10.0, 1995.0, 1990.0, 800.0];
        let places = telemetry::cluster(&events, 80.0, Some(2000.0));
        assert_eq!(places.len(), 2);
        assert_eq!(places[0].count, 4);
        let d = places[0].distance_m;
        assert!(!(1.0..1999.0).contains(&d), "should sit on the line, got {d}");
        assert_eq!(telemetry::cluster(&events, 80.0, None).len(), 3, "without the lap length");
    }

    #[test]
    fn missing_channels_are_ignored_even_if_filled() {
        // The demo fills ride heights that touch the ground; a source that says it has no
        // ride height must not get bottoming from them.
        let no_ride = telemetry::Channels { ride_height: false, ..telemetry::Channels::ALL };
        let mut a = Analyzer::default();
        a.set_channels(no_ride);
        let mut t0 = 0.0;
        for _ in 0..3 {
            let lap = demo::lap(t0);
            t0 = lap.last().unwrap().time + 0.01;
            lap.iter().for_each(|s| a.push(s));
        }
        let r = a.report();
        assert!(!found(&r, "bottoming", None, CornerSpeed::Both));
        assert_eq!(r.detected.len(), 5);
        assert_eq!(no_ride.unavailable().len(), 1);
        assert!(no_ride.unavailable()[0].starts_with("Bottoming"));
        assert!(telemetry::Channels::ALL.unavailable().is_empty());
    }

    #[test]
    fn steering_ratio_needs_range_and_lock() {
        let s = &demo::lap(0.0)[500];
        // Demo: 540° wheel, 0.3 rad (17.2°) road-wheel lock.
        let ratio = s.steering_ratio().unwrap();
        assert!((ratio - 270.0 / 0.3f32.to_degrees()).abs() < 1e-3, "{ratio}");
        let no_lock = telemetry::Channels { steer_lock: false, ..telemetry::Channels::ALL };
        assert_eq!(no_lock.mask(s).steering_ratio(), None, "a masked lock is ignored");
        assert!(no_lock.unavailable()[0].starts_with("Steering ratio"));
        let no_range = Sample { steer_range_deg: 0.0, ..s.clone() };
        assert_eq!(no_range.steering_ratio(), None, "unknown wheel range: no ratio");

        let mut a = Analyzer::default();
        demo::lap(0.0).iter().for_each(|x| a.push(x));
        assert_eq!(a.steering_ratio(), Some(ratio));
        assert!(!a.wheel_range_assumed());
    }

    /// Plays three demo laps quickly, without ride height, then goes quiet.
    struct NoRideHeight {
        samples: Vec<Sample>,
        next: usize,
    }

    impl live::Source for NoRideHeight {
        fn poll(&mut self) -> live::Poll {
            let end = (self.next + 3000).min(self.samples.len());
            let chunk = self.samples[self.next..end].to_vec();
            self.next = end;
            live::Poll::Samples(chunk)
        }
        fn identity(&self) -> Option<live::SourceIdentity> {
            let mut id = demo::identity(demo::track_info());
            id.game = "Test".into();
            id.channels.ride_height = false;
            Some(id)
        }
    }

    #[test]
    fn unusable_track_map_is_backed_up_not_trusted() {
        let dir = std::env::temp_dir().join(format!("setup-core-map-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let info = demo::track_info();
        // Right length, but no bins: used to panic on the first sample.
        let broken = format!(
            r#"{{"info":{{"id":"demo","name":"Demo track","length_m":{}}},"bins":[],"laps":3,"names":{{"1":"Hairpin"}}}}"#,
            info.length_m
        );
        std::fs::write(dir.join("demo.json"), &broken).unwrap();
        let mut map = track::TrackMap::load_or_new(Some(&dir), info);
        assert_eq!(map.laps, 0, "a fresh map, not the broken one");
        for s in demo::lap(0.0).iter().step_by(50) {
            map.add(s);
        }
        assert_eq!(std::fs::read_to_string(dir.join("demo.json.bak")).unwrap(), broken, "old map kept");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Panics on its second poll, then plays a demo lap, 100 samples per poll.
    struct PanicsOnce {
        polls: u32,
        samples: Vec<Sample>,
        next: usize,
    }

    impl live::Source for PanicsOnce {
        fn poll(&mut self) -> live::Poll {
            self.polls += 1;
            if self.polls == 2 {
                panic!("test: source failed once");
            }
            let end = (self.next + 100).min(self.samples.len());
            let chunk = self.samples[self.next..end].to_vec();
            self.next = end;
            live::Poll::Samples(chunk)
        }
    }

    #[test]
    fn telemetry_survives_a_panic() {
        let source = PanicsOnce { polls: 0, samples: demo::lap(0.0), next: 0 };
        let session = live::LiveSession::start(Box::new(source), None);
        let rules = RuleBook::builtin();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        // Commands keep working, and analysis carries on after the panic.
        while session.status(&rules).report.driving_seconds < 5.0 {
            assert!(std::time::Instant::now() < deadline, "analysis stopped after the panic");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        session.reset();
    }

    #[test]
    fn live_session_reports_source_and_unavailable_channels() {
        let mut samples = Vec::new();
        let mut t0 = 0.0;
        for _ in 0..3 {
            let lap = demo::lap(t0);
            t0 = lap.last().unwrap().time + 0.01;
            samples.extend(lap);
        }
        let total = samples.len();
        let session = live::LiveSession::start(Box::new(NoRideHeight { samples, next: 0 }), None);
        let rules = RuleBook::builtin();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let status = loop {
            let st = session.status(&rules);
            // All samples analysed: driving time covers (almost) the three laps.
            if st.report.driving_seconds >= (total as f32 * 0.01) * 0.9 || std::time::Instant::now() > deadline {
                break st;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let source = status.source.as_ref().expect("identity reported");
        assert_eq!(source.game, "Test");
        assert_eq!(source.car.as_ref().unwrap().id, "demo-car");
        assert_eq!(source.track.as_ref().unwrap().id, "demo");
        assert!(status.unavailable.iter().any(|u| u.starts_with("Bottoming")), "{:?}", status.unavailable);
        let behaviors: Vec<_> = status.report.detected.iter().map(|d| d.symptom.behavior.as_str()).collect();
        assert!(!behaviors.contains(&"bottoming"), "{behaviors:?}");
        assert!(behaviors.contains(&"front_lockup"), "analysis ran: {behaviors:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gt3() -> CarContext {
        CarContext { drivetrain: Drivetrain::Rwd, has_aero: true }
    }

    fn symptom(behavior: &str, phase: Option<Phase>, speed: CornerSpeed) -> Symptom {
        Symptom { behavior: behavior.into(), phase, speed, severity: Severity::Slight }
    }

    #[test]
    fn builtin_rules_are_valid() {
        let book = RuleBook::builtin();
        assert_eq!(book.validate(), Vec::<String>::new());
        for b in &book.behaviors {
            assert!(book.rules.iter().any(|r| r.behavior == b.id), "no rules for {}", b.id);
        }
    }

    #[test]
    fn slow_mid_understeer_softens_front_bar_first() {
        let book = RuleBook::builtin();
        let out = book.suggest(&gt3(), &[symptom("understeer", Some(Phase::Mid), CornerSpeed::Slow)]);
        assert_eq!(out[0].param, "front_arb");
        assert_eq!(out[0].direction, Direction::Decrease);
        assert!(out.iter().all(|s| s.param != "rear_wing"), "slow corners should not touch aero");
    }

    #[test]
    fn fast_understeer_uses_aero_only_with_aero() {
        let book = RuleBook::builtin();
        let s = [symptom("understeer", Some(Phase::Mid), CornerSpeed::Fast)];
        let with = book.suggest(&gt3(), &s);
        assert!(with.iter().any(|x| x.param == "rear_wing" && x.direction == Direction::Decrease));
        let no_aero = CarContext { has_aero: false, ..gt3() };
        let without = book.suggest(&no_aero, &s);
        assert!(without.iter().all(|x| x.param != "rear_wing"));
    }

    #[test]
    fn severity_scales_clicks_with_cap() {
        let book = RuleBook::builtin();
        let mut s = symptom("understeer", Some(Phase::Mid), CornerSpeed::Slow);
        s.severity = Severity::Severe;
        let out = book.suggest(&gt3(), &[s]);
        let arb = out.iter().find(|x| x.param == "front_arb").unwrap();
        assert_eq!(arb.clicks, 3);
    }

    /// Oversteer on exit was fixed by softening the rear; now the car understeers on exit.
    /// Suggestions must not just undo the fix.
    #[test]
    fn history_keeps_earlier_fixes() {
        let book = RuleBook::builtin();
        let oversteer = symptom("oversteer", Some(Phase::Exit), CornerSpeed::Slow);
        let history = History {
            applied: vec![
                AppliedChange {
                    param: "rear_arb".into(),
                    direction: Direction::Decrease,
                    clicks: 2,
                    for_symptoms: vec!["Oversteer".into()],
                    run: 1,
                },
                AppliedChange {
                    param: "tc".into(),
                    direction: Direction::Increase,
                    clicks: 1,
                    for_symptoms: vec!["Oversteer".into()],
                    run: 1,
                },
            ],
            fixed: vec![oversteer],
        };
        let now = [symptom("understeer", Some(Phase::Exit), CornerSpeed::Slow)];

        let naive = book.suggest(&gt3(), &now);
        let naive_rear = naive.iter().position(|s| s.param == "rear_arb").unwrap();

        let out = book.suggest_with_history(&gt3(), &now, &history);
        let rear = out.iter().position(|s| s.param == "rear_arb").unwrap();
        assert_eq!(out[rear].direction, Direction::Increase);
        assert_eq!(out[rear].undoes.len(), 1, "should say it undoes the rear bar change");
        assert!(!out[rear].would_worsen.is_empty(), "should warn that oversteer may come back");
        assert!(out[rear].clicks <= 1, "at most half of the 2 clicks back");
        assert!(
            out[rear].score < naive[naive_rear].score * 0.5,
            "undoing should score much lower than without history"
        );
        assert!(
            out[0].undoes.is_empty() && out[0].would_worsen.is_empty(),
            "top pick should be safe: {}",
            out[0].param
        );
    }

    #[test]
    fn opposite_requests_are_flagged() {
        let book = RuleBook::builtin();
        let mut worse = symptom("unstable_braking", None, CornerSpeed::Both);
        worse.severity = Severity::Severe;
        let out = book.suggest(&gt3(), &[symptom("understeer", Some(Phase::Entry), CornerSpeed::Both), worse]);
        let bias = out.iter().find(|x| x.param == "brake_bias").unwrap();
        assert!(!bias.conflicts.is_empty());
    }

    #[test]
    fn equally_strong_opposite_requests_give_no_direction() {
        let book = RuleBook::builtin();
        let out = book.suggest(
            &gt3(),
            &[
                symptom("understeer", Some(Phase::Entry), CornerSpeed::Both),
                symptom("unstable_braking", None, CornerSpeed::Both),
            ],
        );
        assert!(out.iter().all(|x| x.param != "brake_bias"), "a tie shouldn't pick a direction");
    }
}
