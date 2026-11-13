//! Saves telemetry samples as CSV so real laps can be replayed to tune the detectors.
//! About 1.5 MB per minute of driving.

use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::live::SourceIdentity;
use crate::telemetry::{Sample, Thresholds};

const HEADER: &str = "time,speed,throttle,brake,steer,yaw_rate,lat_accel,lat_velocity,\
ws_fl,ws_fr,ws_rl,ws_rr,\
t_fl_in,t_fl_mid,t_fl_out,t_fr_in,t_fr_mid,t_fr_out,t_rl_in,t_rl_mid,t_rl_out,t_rr_in,t_rr_mid,t_rr_out,\
p_fl,p_fr,p_rl,p_rr,rh_fl,rh_fr,rh_rl,rh_rr,gear,steer_range_deg,vert_accel,lap_distance,elevation,steer_lock_deg";

pub struct Recorder {
    out: BufWriter<File>,
    rows: u64,
    last_time: Option<f64>,
}

impl Recorder {
    /// Starts a new recording at `path`. Fails if the file exists: recordings are never overwritten.
    pub fn create(path: &Path) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = File::options().write(true).create_new(true).open(path)?;
        let mut out = BufWriter::new(file);
        writeln!(out, "{HEADER}")?;
        Ok(Self { out, rows: 0, last_time: None })
    }

    /// Like [`create`](Self::create), but if `path` is taken, uses `<name>-2.csv`, `<name>-3.csv`, …
    /// Returns the path actually used.
    pub fn create_unique(path: &Path) -> io::Result<(Self, PathBuf)> {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        for n in 1..1000 {
            let candidate = if n == 1 { path.to_path_buf() } else { path.with_file_name(format!("{stem}-{n}{ext}")) };
            match Self::create(&candidate) {
                Ok(r) => return Ok((r, candidate)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("no free name for {}", path.display())))
    }

    pub fn write(&mut self, s: &Sample) -> io::Result<()> {
        let mut f: Vec<String> = vec![format!("{:.4}", s.time)];
        let mut push = |v: f32| f.push(format!("{v:.4}"));
        for v in [s.speed, s.throttle, s.brake, s.steer, s.yaw_rate, s.lat_accel, s.lat_velocity] {
            push(v);
        }
        s.wheel_speed.iter().for_each(|v| push(*v));
        s.tyre_temp.iter().flatten().for_each(|v| push(*v));
        s.tyre_pressure.iter().for_each(|v| push(*v));
        s.ride_height.iter().for_each(|v| push(*v));
        f.push(s.gear.to_string());
        f.push(format!("{}", s.steer_range_deg));
        f.push(format!("{:.4}", s.vert_accel));
        f.push(format!("{:.2}", s.lap_distance));
        f.push(s.elevation.map(|e| format!("{e:.3}")).unwrap_or_default());
        f.push(format!("{}", s.steer_lock_deg));
        writeln!(self.out, "{}", f.join(","))?;
        self.rows += 1;
        self.last_time = Some(s.time);
        Ok(())
    }

    /// Where the recording is now: rows written and the time of the last one.
    pub fn mark(&self) -> Mark {
        Mark { rows: self.rows, time: self.last_time }
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// A recording read back, with what had to be left out.
#[derive(Debug, Default)]
pub struct Imported {
    pub samples: Vec<Sample>,
    /// Rows that were left out because a value was missing or not a number.
    pub rejected: usize,
    /// The first problem found, e.g. "line 12, column 3 (throttle): 'x' is not a number".
    pub first_problem: Option<String>,
}

/// Reads a recording back, leaving out rows that don't parse. See [`read_checked`].
pub fn read(path: &Path) -> io::Result<Vec<Sample>> {
    read_checked(path).map(|r| r.samples)
}

/// Reads a recording back. Every value is read from its own column, so a bad value
/// rejects its row instead of shifting the others. Supported layouts: 34 columns (oldest,
/// no vertical acceleration and lap distance), 36 (no elevation), 37 (no steering lock) and
/// 38 (current); extra columns are ignored. Elevation and steering lock may be empty.
pub fn read_checked(path: &Path) -> io::Result<Imported> {
    let mut out = Imported::default();
    let names: Vec<&str> = HEADER.split(',').collect();
    let mut lines = BufReader::new(File::open(path)?).lines();
    let header = lines.next().transpose()?.unwrap_or_default();
    if !header.starts_with("time,speed,") {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a Setup Assistant recording"));
    }
    for (n, line) in lines.enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').map(str::trim).collect();
        let required = match cols.len() {
            34 => 34,
            36 => 36,
            l if l >= 37 => 36,
            _ => {
                out.rejected += 1;
                out.first_problem.get_or_insert_with(|| {
                    format!("line {}: {} columns, expected 34, 36, 37 or 38", n + 2, cols.len())
                });
                continue;
            }
        };
        let mut v = Vec::with_capacity(required);
        let mut bad = None;
        for (i, x) in cols.iter().take(required).enumerate() {
            match x.parse::<f64>() {
                Ok(x) if x.is_finite() => v.push(x),
                _ => {
                    bad = Some(format!("line {}, column {} ({}): '{x}' is not a number", n + 2, i + 1, names[i]));
                    break;
                }
            }
        }
        // Optional columns: absent or empty = unknown, anything else must be a number.
        let mut optional = |i: usize| match cols.get(i).copied() {
            None | Some("") => None,
            Some(x) => match x.parse::<f32>() {
                Ok(e) if e.is_finite() => Some(e),
                _ => {
                    bad.get_or_insert_with(|| {
                        format!("line {}, column {} ({}): '{x}' is not a number", n + 2, i + 1, names[i])
                    });
                    None
                }
            },
        };
        let elevation = optional(36);
        let steer_lock = optional(37);
        if let Some(problem) = bad {
            out.rejected += 1;
            out.first_problem.get_or_insert(problem);
            continue;
        }
        let f = |i: usize| v[i] as f32;
        let mut s = Sample {
            time: v[0],
            speed: f(1),
            throttle: f(2),
            brake: f(3),
            steer: f(4),
            yaw_rate: f(5),
            lat_accel: f(6),
            lat_velocity: f(7),
            wheel_speed: [f(8), f(9), f(10), f(11)],
            gear: v[32] as i32,
            steer_range_deg: f(33),
            steer_lock_deg: steer_lock.unwrap_or(0.0),
            ..Default::default()
        };
        if v.len() >= 36 {
            s.vert_accel = f(34);
            s.lap_distance = f(35);
            s.elevation = elevation;
        }
        for t in 0..4 {
            s.tyre_temp[t] = [f(12 + t * 3), f(13 + t * 3), f(14 + t * 3)];
            s.tyre_pressure[t] = f(24 + t);
            s.ride_height[t] = f(28 + t);
        }
        out.samples.push(s);
    }
    Ok(out)
}

/// Version of the sidecar layout. Bump it when a field changes meaning; new optional fields
/// don't need it.
pub const SIDECAR_SCHEMA: u32 = 1;

/// A point in a recording: rows written so far and the game time of the last one.
/// Rows are unambiguous; the game clock can restart within a recording (ADR-18).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Mark {
    pub rows: u64,
    /// Game time (s) of the last row, if any was written.
    pub time: Option<f64>,
}

/// Something that happened during a recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SidecarEvent {
    /// The driver cleared the run's analysis ("Reset").
    Reset { at: Mark },
    /// The analysis hit a bug and started over.
    AnalysisRestarted { at: Mark },
    /// The source reported a different game, car, track, session or channels.
    Identity { at: Mark, source: Box<SourceIdentity> },
    /// The recording was closed (next run, new session, demo switched on). Missing if the app
    /// stopped while recording.
    End { at: Mark },
}

/// What a recording can't say about itself, saved as `<recording>.json` next to the CSV.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub schema: u32,
    pub app_version: String,
    /// Git commit the app was built from, if known.
    pub commit: Option<String>,
    /// When the recording started (Unix seconds).
    pub created: u64,
    pub session_id: u64,
    pub run: u32,
    pub demo: bool,
    /// Game, car, track, session and channels when the recording started.
    pub source: Option<SourceIdentity>,
    pub thresholds: Thresholds,
    #[serde(default)]
    pub events: Vec<SidecarEvent>,
}

impl Sidecar {
    /// A sidecar for a recording starting now, made by this build of the app.
    pub fn new(session_id: u64, run: u32, demo: bool, source: Option<SourceIdentity>, thresholds: Thresholds) -> Self {
        Self {
            schema: SIDECAR_SCHEMA,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            commit: option_env!("SETUP_ASSISTANT_COMMIT").map(str::to_string),
            created: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            session_id,
            run,
            demo,
            source,
            thresholds,
            events: Vec::new(),
        }
    }

    /// The identity in effect at the end: the last identity event, else the one at the start.
    pub fn last_source(&self) -> Option<&SourceIdentity> {
        let changed = self.events.iter().rev().find_map(|e| match e {
            SidecarEvent::Identity { source, .. } => Some(&**source),
            _ => None,
        });
        changed.or(self.source.as_ref())
    }
}

/// The sidecar file of the recording at `csv`: same name, `.json`.
pub fn sidecar_path(csv: &Path) -> PathBuf {
    csv.with_extension("json")
}

/// Keeps a recording's sidecar on disk. The whole file is rewritten on every event (they are
/// rare) through a temporary file and a rename, so it is always complete, valid JSON.
pub struct SidecarWriter {
    path: PathBuf,
    data: Sidecar,
}

impl SidecarWriter {
    /// Writes `data` as the sidecar of the recording at `csv`.
    pub fn create(csv: &Path, data: Sidecar) -> io::Result<Self> {
        let w = Self { path: sidecar_path(csv), data };
        w.save()?;
        Ok(w)
    }

    pub fn data(&self) -> &Sidecar {
        &self.data
    }

    /// Adds `event` and saves. An identity event that doesn't change the identity is left out.
    pub fn push(&mut self, event: SidecarEvent) -> io::Result<()> {
        if let SidecarEvent::Identity { source, .. } = &event {
            if self.data.last_source() == Some(&**source) {
                return Ok(());
            }
        }
        self.data.events.push(event);
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(&self.data).map_err(io::Error::other)?;
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// Reads the sidecar of the recording at `csv`. `Ok(None)` if there is none (recordings made
/// before sidecars existed).
pub fn read_sidecar(csv: &Path) -> io::Result<Option<Sidecar>> {
    match std::fs::read(sidecar_path(csv)) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
