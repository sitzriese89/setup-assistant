//! Replays a recorded run through the analyzer, for tuning the detectors.
//!
//!   cargo run -p setup-core --example replay -- <recording.csv> [understeer_deg_per_g]
//!
//! Reads `<recording>.json` next to it when present: missing channels are masked and places
//! wrap at the start line, as they were live. Older recordings have none.

use std::path::PathBuf;

use setup_core::recording::{self, SidecarEvent};
use setup_core::telemetry::{Analyzer, Thresholds};
use setup_core::{Phase, RuleBook};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: replay <recording.csv> [understeer_deg_per_g]");
        std::process::exit(2);
    };
    let mut thresholds = Thresholds::default();
    if let Some(u) = args.next().and_then(|a| a.parse().ok()) {
        thresholds.understeer_deg_per_g = u;
    }

    let imported = recording::read_checked(&path).expect("can't read recording");
    if imported.rejected > 0 {
        eprintln!(
            "left out {} bad rows; first: {}",
            imported.rejected,
            imported.first_problem.as_deref().unwrap_or("?")
        );
    }
    let samples = imported.samples;
    let mut a = Analyzer::new(thresholds.clone());

    // The sidecar (since M1.8) says what the CSV can't: car, track, channels, events.
    match recording::read_sidecar(&path) {
        Ok(Some(side)) => {
            println!(
                "recorded with app {} ({}), session {} run {}{}",
                side.app_version,
                side.commit.as_deref().unwrap_or("commit unknown"),
                side.session_id,
                side.run,
                if side.demo { ", demo" } else { "" }
            );
            if let Some(src) = side.last_source() {
                let car =
                    src.car.as_ref().map_or("car unknown".to_string(), |c| c.name.clone().unwrap_or(c.id.clone()));
                let track = src
                    .track
                    .as_ref()
                    .map_or("track unknown".to_string(), |t| format!("{} ({:.0} m)", t.name, t.length_m));
                println!("{} {} · {car} · {track} · {:?}", src.game, src.api_version, src.session);
                a.set_channels(src.channels);
                a.set_track_length(src.track.as_ref().map(|t| t.length_m));
                for u in src.channels.unavailable() {
                    println!("  unavailable: {u}");
                }
            }
            if side.thresholds != Thresholds::default() {
                println!("  recorded with other thresholds than this build's; the replay uses this build's");
            }
            for e in &side.events {
                let (what, at) = match e {
                    SidecarEvent::Reset { at } => ("reset by the driver", at),
                    SidecarEvent::AnalysisRestarted { at } => ("analysis restarted after an error", at),
                    SidecarEvent::Identity { at, .. } => ("source changed (car, track, session or channels)", at),
                    SidecarEvent::End { at } => ("end of recording", at),
                };
                let time = at.time.map_or("–".to_string(), |t| format!("{t:.1} s"));
                println!("  row {:>7} ({time}): {what}", at.rows);
            }
            if !matches!(side.events.last(), Some(SidecarEvent::End { .. })) {
                println!("  no end event: the app stopped while recording");
            }
        }
        Ok(None) => println!("no sidecar (older recording): all channels assumed, lap length unknown"),
        Err(e) => println!("sidecar unreadable ({e}): all channels assumed, lap length unknown"),
    }

    for s in &samples {
        a.push(s);
    }
    let r = a.report();
    let rules = RuleBook::builtin();

    println!("{} samples, {:.0} s driving", samples.len(), r.driving_seconds);
    match a.geometry_gain() {
        Some(k) => println!("steering geometry learned: {k:.4} (rad/s per m/s per unit steer)"),
        None => println!("steering geometry NOT learned: no gentle curves found"),
    }
    // Steering units (#35): the analyzer works in steering-wheel degrees.
    let distinct = |f: &dyn Fn(&setup_core::telemetry::Sample) -> f32| {
        let mut v: Vec<i64> = samples.iter().map(|s| f(s).round() as i64).collect();
        v.sort_unstable();
        v.dedup();
        v.iter().map(|x| if *x <= 0 { "unknown".to_string() } else { format!("{x}°") }).collect::<Vec<_>>().join(", ")
    };
    println!("steering wheel range: {}", distinct(&|s| s.steer_range_deg));
    if a.wheel_range_assumed() {
        println!("  (range unknown at the end: understeer assumed a 540° wheel)");
    }
    println!("steering lock (road wheel): {}", distinct(&|s| s.steer_lock_deg));
    let ratio = a.steering_ratio();
    match ratio {
        Some(k) => println!("steering ratio: {k:.1}:1"),
        None => println!("steering ratio: unknown (recording has no steering lock), road-wheel values not shown"),
    }

    println!("\nbalance by phase (deg per g, + understeer / - oversteer):");
    println!("  {:<22} {:>8} {:>8}", "", "wheel", "road");
    for b in &r.balance_by_phase {
        let phase = match b.phase {
            Phase::Braking => "braking*",
            Phase::Entry => "entry",
            Phase::Mid => "mid",
            _ => "exit",
        };
        let road = ratio.map_or("–".to_string(), |k| format!("{:+.2}", b.deg_per_g / k));
        let what = format!("{phase} {:?}", b.speed);
        println!("  {what:<22} {:>+8.1} {road:>8}  ({:.0} s)", b.deg_per_g, b.seconds);
    }
    println!("\ndetected (understeer threshold {} steering-wheel deg/g):", thresholds.understeer_deg_per_g);
    for d in &r.detected {
        let places: Vec<String> =
            d.places.iter().take(3).map(|p| format!("{:.0} m ({}x)", p.distance_m, p.count)).collect();
        println!(
            "  {:<45} {:>3}x  {:.1}/min  {:?}{}  {}",
            rules.symptom_label(&d.symptom),
            d.count,
            d.per_minute,
            d.symptom.severity,
            if d.one_place { "  ONE PLACE" } else { "" },
            places.join(", ")
        );
    }
    if r.detected.is_empty() {
        println!("  nothing");
    }
}
