//! Replays a recorded run through the analyzer, for tuning the detectors.
//!
//!   cargo run -p setup-core --example replay -- <recording.csv> [understeer_deg_per_g]

use std::path::PathBuf;

use setup_core::recording;
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

    let samples = recording::read(&path).expect("can't read recording");
    let mut a = Analyzer::new(thresholds.clone());

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
    println!("\nbalance by phase (wheel deg per g, + understeer / - oversteer):");
    for b in &r.balance_by_phase {
        let phase = match b.phase {
            Phase::Braking => "braking*",
            Phase::Entry => "entry",
            Phase::Mid => "mid",
            _ => "exit",
        };
        println!("  {phase:<6} {:?}: {:+6.1}  ({:.0} s)", b.speed, b.deg_per_g, b.seconds);
    }
    println!("\ndetected (understeer threshold {} deg/g):", thresholds.understeer_deg_per_g);
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
