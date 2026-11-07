//! A map of the track, learned from the driver's own laps.
//!
//! Every 10 m of lap distance keeps the average elevation, cornering force and
//! speed. From that come numbered corners (T1, T2, ...), crests and compressions.
//! Detections are then described by place ("T7 The Dipper · compression"), and a
//! problem that keeps showing up at the same spot run after run is flagged as
//! probably the track rather than the setup.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::telemetry::{LiveReport, Sample};

const BIN_M: f32 = 10.0;
const G: f32 = 9.81;
/// Extra vertical acceleration (m/s²) that makes a dip a compression or a hump a crest.
const FEATURE_ACCEL: f32 = 3.0;
/// Cornering force (g) that counts as a corner.
const CORNER_G: f32 = 0.4;
/// How many past runs are kept per track.
const RUNS_KEPT: usize = 12;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackInfo {
    /// Stable id, e.g. "r3e-1691-1692" (game, track, layout).
    pub id: String,
    pub name: String,
    pub length_m: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Bin {
    n: u32,
    elev: f32,
    lat_g: f32,
    speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKind {
    Crest,
    Compression,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub kind: FeatureKind,
    pub start_m: f32,
    pub end_m: f32,
    pub at_m: f32,
    /// Extra (compression) or missing (crest) load at the peak, in g.
    pub strength_g: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Corner {
    pub number: u32,
    pub name: Option<String>,
    pub start_m: f32,
    pub end_m: f32,
    pub apex_m: f32,
    pub min_speed_kmh: f32,
    pub peak_g: f32,
}

impl Corner {
    pub fn title(&self) -> String {
        match &self.name {
            Some(n) if !n.trim().is_empty() => format!("T{} {}", self.number, n.trim()),
            _ => format!("T{}", self.number),
        }
    }
}

/// Where each problem happened in one run, for comparing runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunSummary {
    pub events: Vec<(String, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMap {
    pub info: TrackInfo,
    bins: Vec<Bin>,
    pub laps: u32,
    /// Corner number -> name the driver gave it.
    #[serde(default)]
    pub names: BTreeMap<u32, String>,
    #[serde(default)]
    pub runs: Vec<RunSummary>,
    #[serde(skip)]
    last_distance: Option<f32>,
}

/// What the UI shows of the map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackView {
    pub name: String,
    pub length_m: f32,
    pub laps: u32,
    /// Share of the lap with data, 0..1.
    pub coverage: f32,
    pub ready: bool,
    /// (distance, elevation) every ~20 m.
    pub profile: Vec<(f32, f32)>,
    pub corners: Vec<Corner>,
    pub features: Vec<Feature>,
    pub runs: usize,
}

impl TrackMap {
    pub fn new(info: TrackInfo) -> Self {
        let n = (info.length_m.max(BIN_M) / BIN_M).ceil() as usize + 1;
        Self {
            info,
            bins: vec![Bin::default(); n],
            laps: 0,
            names: BTreeMap::new(),
            runs: Vec::new(),
            last_distance: None,
        }
    }

    fn file(dir: &Path, id: &str) -> PathBuf {
        let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
        dir.join(format!("{safe}.json"))
    }

    /// Loads the saved map for this track, or starts a new one.
    pub fn load_or_new(dir: Option<&Path>, info: TrackInfo) -> Self {
        if let Some(dir) = dir {
            if let Ok(text) = std::fs::read_to_string(Self::file(dir, &info.id)) {
                if let Ok(mut m) = serde_json::from_str::<TrackMap>(&text) {
                    if (m.info.length_m - info.length_m).abs() < 1.0 {
                        m.info = info;
                        return m;
                    }
                }
            }
        }
        Self::new(info)
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let text = serde_json::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(Self::file(dir, &self.info.id), text)
    }

    pub fn add(&mut self, s: &Sample) {
        let d = s.lap_distance;
        if d < 0.0 || s.speed < 5.0 {
            return;
        }
        // Count a lap when the distance wraps from the end back to the start.
        if let Some(prev) = self.last_distance {
            if prev > self.info.length_m * 0.8 && d < self.info.length_m * 0.2 {
                self.laps += 1;
            }
        }
        self.last_distance = Some(d);
        let Some(elev) = s.elevation else { return };
        let i = ((d / BIN_M) as usize).min(self.bins.len() - 1);
        let b = &mut self.bins[i];
        // Running means; older laps fade a little so the map follows the driver's line.
        let k = 1.0 / (b.n.min(200) + 1) as f32;
        b.elev += (elev - b.elev) * if b.n == 0 { 1.0 } else { k };
        b.lat_g += (s.lat_accel.abs() / G - b.lat_g) * if b.n == 0 { 1.0 } else { k };
        b.speed += (s.speed - b.speed) * if b.n == 0 { 1.0 } else { k };
        b.n += 1;
    }

    pub fn coverage(&self) -> f32 {
        self.bins.iter().filter(|b| b.n > 0).count() as f32 / self.bins.len() as f32
    }

    pub fn ready(&self) -> bool {
        self.laps >= 1 && self.coverage() >= 0.9
    }

    /// Smoothed value per bin (None where there's no data).
    fn smoothed(&self, f: impl Fn(&Bin) -> f32, half: usize) -> Vec<Option<f32>> {
        (0..self.bins.len())
            .map(|i| {
                let lo = i.saturating_sub(half);
                let hi = (i + half).min(self.bins.len() - 1);
                let (sum, n) =
                    self.bins[lo..=hi].iter().filter(|b| b.n > 0).fold((0.0, 0), |(s, n), b| (s + f(b), n + 1));
                (self.bins[i].n > 0 && n > 0).then(|| sum / n as f32)
            })
            .collect()
    }

    pub fn corners(&self) -> Vec<Corner> {
        let lat = self.smoothed(|b| b.lat_g, 1);
        let mut segs: Vec<(usize, usize)> = Vec::new();
        for (i, g) in lat.iter().enumerate() {
            if matches!(g, Some(g) if *g > CORNER_G) {
                match segs.last_mut() {
                    // Join corners split by a short gap (S-bends stay separate if the gap is long).
                    Some(s) if i <= s.1 + 3 => s.1 = i,
                    _ => segs.push((i, i)),
                }
            }
        }
        segs.retain(|(a, b)| b - a >= 2);
        segs.iter()
            .enumerate()
            .map(|(n, &(a, b))| {
                let apex = (a..=b).min_by(|x, y| self.bins[*x].speed.total_cmp(&self.bins[*y].speed)).unwrap_or(a);
                let number = n as u32 + 1;
                Corner {
                    number,
                    name: self.names.get(&number).cloned(),
                    start_m: a as f32 * BIN_M,
                    end_m: (b + 1) as f32 * BIN_M,
                    apex_m: (apex as f32 + 0.5) * BIN_M,
                    min_speed_kmh: self.bins[apex].speed * 3.6,
                    peak_g: (a..=b).filter_map(|i| lat[i]).fold(0.0, f32::max),
                }
            })
            .collect()
    }

    pub fn features(&self) -> Vec<Feature> {
        let elev = self.smoothed(|b| b.elev, 1);
        let span = 2; // curvature over ±20 m
        let ds = span as f32 * BIN_M;
        let accel: Vec<Option<f32>> = (0..elev.len())
            .map(|i| {
                if i < span || i + span >= elev.len() {
                    return None;
                }
                let (a, b, c) = (elev[i - span]?, elev[i]?, elev[i + span]?);
                let curvature = (a - 2.0 * b + c) / (ds * ds);
                Some(self.bins[i].speed.powi(2) * curvature)
            })
            .collect();
        let mut out = Vec::new();
        for kind in [FeatureKind::Compression, FeatureKind::Crest] {
            let hit = |a: f32| match kind {
                FeatureKind::Compression => a > FEATURE_ACCEL,
                FeatureKind::Crest => a < -FEATURE_ACCEL,
            };
            let mut i = 0;
            while i < accel.len() {
                if matches!(accel[i], Some(a) if hit(a)) {
                    let start = i;
                    while i + 1 < accel.len() && matches!(accel[i + 1], Some(a) if hit(a)) {
                        i += 1;
                    }
                    let peak = (start..=i)
                        .max_by(|x, y| accel[*x].unwrap().abs().total_cmp(&accel[*y].unwrap().abs()))
                        .unwrap();
                    out.push(Feature {
                        kind,
                        start_m: start as f32 * BIN_M,
                        end_m: (i + 1) as f32 * BIN_M,
                        at_m: (peak as f32 + 0.5) * BIN_M,
                        strength_g: accel[peak].unwrap() / G,
                    });
                }
                i += 1;
            }
        }
        out.sort_by(|a, b| a.start_m.total_cmp(&b.start_m));
        out
    }

    pub fn feature_at(&self, at: f32) -> Option<FeatureKind> {
        self.features().into_iter().find(|f| at >= f.start_m - 15.0 && at <= f.end_m + 15.0).map(|f| f.kind)
    }

    /// "T7 The Dipper · compression", "before T4", "1.24 km · crest".
    pub fn describe(&self, at: f32) -> String {
        let corners = self.corners();
        let mut s = if let Some(c) = corners.iter().find(|c| at >= c.start_m - 20.0 && at <= c.end_m + 20.0) {
            c.title()
        } else if let Some(c) = corners
            .iter()
            .filter(|c| c.start_m > at && c.start_m - at < 250.0)
            .min_by(|a, b| a.start_m.total_cmp(&b.start_m))
        {
            format!("before {}", c.title())
        } else if let Some(c) =
            corners.iter().filter(|c| c.end_m < at && at - c.end_m < 250.0).max_by(|a, b| a.end_m.total_cmp(&b.end_m))
        {
            format!("after {}", c.title())
        } else {
            format!("{:.2} km", at / 1000.0)
        };
        match self.feature_at(at) {
            Some(FeatureKind::Compression) => s.push_str(" · compression"),
            Some(FeatureKind::Crest) => s.push_str(" · crest"),
            None => {}
        }
        s
    }

    pub fn view(&self) -> TrackView {
        let profile = self
            .bins
            .iter()
            .enumerate()
            .step_by(2)
            .filter(|(_, b)| b.n > 0)
            .map(|(i, b)| ((i as f32 + 0.5) * BIN_M, b.elev))
            .collect();
        TrackView {
            name: self.info.name.clone(),
            length_m: self.info.length_m,
            laps: self.laps,
            coverage: self.coverage(),
            ready: self.ready(),
            profile,
            corners: self.corners(),
            features: self.features(),
            runs: self.runs.len(),
        }
    }

    /// Remembers where problems happened in a finished run.
    pub fn add_run(&mut self, report: &LiveReport) {
        let events: Vec<(String, f32)> = report
            .detected
            .iter()
            .flat_map(|d| d.places.iter().map(move |p| (d.symptom.behavior.clone(), p.distance_m)))
            .collect();
        if events.is_empty() {
            return;
        }
        self.runs.push(RunSummary { events });
        if self.runs.len() > RUNS_KEPT {
            self.runs.remove(0);
        }
    }

    /// Adds place names, compression context and run-to-run persistence to a report.
    pub fn annotate(&self, report: &mut LiveReport) {
        let ready = self.ready();
        let recent: Vec<&RunSummary> = self.runs.iter().rev().take(3).collect();
        for d in &mut report.detected {
            for p in &mut d.places {
                if ready {
                    p.label = self.describe(p.distance_m);
                    p.feature = self.feature_at(p.distance_m);
                }
                p.seen_in_runs = recent
                    .iter()
                    .filter(|r| {
                        r.events.iter().any(|(b, at)| *b == d.symptom.behavior && (at - p.distance_m).abs() < 80.0)
                    })
                    .count() as u32;
            }
            d.runs_checked = recent.len() as u32;
            // Same spot in each of the last (at least 2) runs, whatever was changed: likely the track.
            d.persistent =
                recent.len() >= 2 && d.places.first().is_some_and(|p| p.seen_in_runs as usize == recent.len());

            // Bottoming only where the road dips needs different advice than bottoming everywhere.
            if ready && d.symptom.behavior == "bottoming" {
                let total: u32 = d.places.iter().map(|p| p.count).sum();
                let in_dips: u32 =
                    d.places.iter().filter(|p| p.feature == Some(FeatureKind::Compression)).map(|p| p.count).sum();
                if total > 0 && in_dips as f32 >= total as f32 * 0.7 {
                    d.symptom.behavior = "bottoming_compression".into();
                }
            }
        }
    }
}
