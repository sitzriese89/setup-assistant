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
        let out = book.suggest(
            &gt3(),
            &[
                symptom("understeer", Some(Phase::Entry), CornerSpeed::Both),
                symptom("unstable_braking", None, CornerSpeed::Both),
            ],
        );
        let bias = out.iter().find(|x| x.param == "brake_bias").unwrap();
        assert!(!bias.conflicts.is_empty());
    }
}
