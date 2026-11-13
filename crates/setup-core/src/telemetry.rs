//! Turns a stream of telemetry samples into symptoms the rules engine understands.
//!
//! Everything here is game-agnostic. Game adapters convert their data into
//! [`Sample`]s. Signs of steering, yaw and lateral values don't matter: the
//! analyzer learns how they relate from normal driving.
//!
//! ## Balance
//! In gentle curves (below ~0.3 g) every car turns exactly as much as its
//! steering geometry says, whatever its setup. From those moments the analyzer
//! learns the car's "geometric" yaw per unit of steering. In real corners it
//! then measures how much *extra* steering the driver needs beyond geometry,
//! per g of cornering: the understeer gradient, in steering-wheel degrees per g.
//! It doesn't depend on corner radius, so slow and fast corners are comparable.
//! Positive = understeer, negative = the car turns more than geometry (oversteer).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::*;

/// Tyre order everywhere: front-left, front-right, rear-left, rear-right.
pub const TYRE_NAMES: [&str; 4] = ["FL", "FR", "RL", "RR"];

const G: f32 = 9.81;
/// Steering-wheel degrees from centre to full lock when the game doesn't say.
const DEFAULT_HALF_RANGE_DEG: f32 = 270.0;
/// Ride heights (m) below this mean "unknown". No car sits half a metre into the ground.
pub const RIDE_HEIGHT_UNKNOWN_BELOW: f32 = -0.5;

/// One physics frame, in SI units.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    /// Seconds, increasing.
    pub time: f64,
    /// m/s
    pub speed: f32,
    /// 0..1
    pub throttle: f32,
    /// 0..1
    pub brake: f32,
    /// Steering wheel input, -1..1 (sign convention doesn't matter).
    pub steer: f32,
    /// rad/s around the vertical axis (sign convention doesn't matter).
    pub yaw_rate: f32,
    /// m/s², sideways.
    pub lat_accel: f32,
    /// m/s, sideways velocity of the car body (for body slip angle).
    pub lat_velocity: f32,
    /// Wheel surface speed in m/s.
    pub wheel_speed: [f32; 4],
    /// °C, [inner, middle, outer]. Values <= 0 mean unknown.
    pub tyre_temp: [[f32; 3]; 4],
    /// kPa, <= 0 = unknown.
    pub tyre_pressure: [f32; 4],
    /// m, below [`RIDE_HEIGHT_UNKNOWN_BELOW`] = unknown (sources use -1). Small negative
    /// readings are real: RaceRoom reports them when the car hits the ground hard.
    pub ride_height: [f32; 4],
    pub gear: i32,
    /// Steering wheel rotation from full left to full right in degrees (0 = unknown).
    #[serde(default)]
    pub steer_range_deg: f32,
    /// m/s², vertical. Any sign convention, with or without gravity: the analyzer
    /// learns the level on flat straights. Used to spot crests (car goes light).
    #[serde(default)]
    pub vert_accel: f32,
    /// Metres from the start line (< 0 = unknown). Used to tell where problems happen.
    #[serde(default = "unknown")]
    pub lap_distance: f32,
    /// Height of the car above some fixed level, metres. Builds the track's elevation map.
    #[serde(default)]
    pub elevation: Option<f32>,
    /// Road-wheel angle in degrees at full steering input, one side (0 = unknown). With
    /// `steer_range_deg` this gives the steering ratio, so understeer can be measured at the
    /// front wheels instead of the steering wheel (#35).
    #[serde(default)]
    pub steer_lock_deg: f32,
}

fn unknown() -> f32 {
    -1.0
}

/// Everything zero, except channels where zero is a real reading: ride height and lap
/// distance default to unknown, so a source that doesn't fill them can't fake bottoming.
impl Default for Sample {
    fn default() -> Self {
        Self {
            time: 0.0,
            speed: 0.0,
            throttle: 0.0,
            brake: 0.0,
            steer: 0.0,
            yaw_rate: 0.0,
            lat_accel: 0.0,
            lat_velocity: 0.0,
            wheel_speed: [0.0; 4],
            tyre_temp: [[0.0; 3]; 4],
            tyre_pressure: [0.0; 4],
            ride_height: [unknown(); 4],
            gear: 0,
            steer_range_deg: 0.0,
            vert_accel: 0.0,
            lap_distance: unknown(),
            elevation: None,
            steer_lock_deg: 0.0,
        }
    }
}

impl Sample {
    /// Steering-wheel degrees per road-wheel degree, when both the wheel range and the
    /// steering lock are known.
    pub fn steering_ratio(&self) -> Option<f32> {
        (self.steer_range_deg > 0.0 && self.steer_lock_deg > 0.0)
            .then(|| self.steer_range_deg / 2.0 / self.steer_lock_deg)
    }
}

/// Which optional channels a source fills in. Speed, pedals, steering, yaw rate, lateral
/// acceleration and velocity, and wheel speeds are always required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channels {
    pub ride_height: bool,
    pub lap_distance: bool,
    pub elevation: bool,
    pub vert_accel: bool,
    pub tyre_temp: bool,
    pub tyre_pressure: bool,
    pub steer_range: bool,
    #[serde(default)]
    pub steer_lock: bool,
}

impl Default for Channels {
    fn default() -> Self {
        Self::ALL
    }
}

impl Channels {
    pub const ALL: Self = Self {
        ride_height: true,
        lap_distance: true,
        elevation: true,
        vert_accel: true,
        tyre_temp: true,
        tyre_pressure: true,
        steer_range: true,
        steer_lock: true,
    };

    /// `s` with the missing channels set to their "unknown" values, so a source that fills
    /// them with placeholders can't produce detections from them.
    pub fn mask(&self, s: &Sample) -> Sample {
        let mut s = s.clone();
        if !self.ride_height {
            s.ride_height = [-1.0; 4];
        }
        if !self.lap_distance {
            s.lap_distance = -1.0;
        }
        if !self.elevation {
            s.elevation = None;
        }
        if !self.vert_accel {
            // A constant reading: the level is learned as that, so nothing looks like a crest.
            s.vert_accel = 0.0;
        }
        if !self.tyre_temp {
            s.tyre_temp = [[0.0; 3]; 4];
        }
        if !self.tyre_pressure {
            s.tyre_pressure = [0.0; 4];
        }
        if !self.steer_range {
            s.steer_range_deg = 0.0;
        }
        if !self.steer_lock {
            s.steer_lock_deg = 0.0;
        }
        s
    }

    /// What can't be done with the channels that are missing, for the driver.
    pub fn unavailable(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut add = |ok: bool, what: &str| {
            if !ok {
                out.push(what.to_string());
            }
        };
        add(self.ride_height, "Bottoming detection (no ride height)");
        add(self.lap_distance, "Where problems happen and the track map (no lap position)");
        add(self.elevation, "Track elevation profile (no car position)");
        add(self.vert_accel, "Crest detection (no vertical acceleration)");
        add(self.tyre_temp, "Tyre temperatures (not provided)");
        add(self.tyre_pressure, "Tyre pressures (not provided)");
        add(self.steer_range, "Exact steering angle (no wheel rotation; assuming 540°)");
        add(self.steer_lock, "Steering ratio (no steering lock; understeer stays in steering-wheel degrees)");
        out
    }
}

/// What the car is doing right now, for the live display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveFrame {
    pub speed_kmh: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub lat_g: f32,
    pub phase: Option<Phase>,
    /// -1 = strong understeer, 0 = neutral, +1 = strong oversteer. None when not cornering.
    pub balance: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detected {
    pub symptom: Symptom,
    /// Filled in by the caller from the rule book.
    pub label: String,
    pub count: u32,
    pub per_minute: f32,
    /// Where on the lap it happened, most frequent first. Empty if lap position is unknown.
    pub places: Vec<Place>,
    /// Most events happened in one spot: maybe the track there (crest, bump, kerb)
    /// rather than the setup.
    pub one_place: bool,
    /// Earlier runs on this track that were compared.
    #[serde(default)]
    pub runs_checked: u32,
    /// Happened at the same spot in every recent run, whatever was changed: likely the track.
    #[serde(default)]
    pub persistent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    /// Metres from the start line.
    pub distance_m: f32,
    pub count: u32,
    /// "T7 The Dipper · compression", once the track map is ready.
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub feature: Option<crate::track::FeatureKind>,
    /// In how many of the recent earlier runs the same problem happened here.
    #[serde(default)]
    pub seen_in_runs: u32,
}

/// The most recent event, even before it counts as a problem (for the overlay).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastEvent {
    pub symptom: Symptom,
    pub label: String,
    /// Increases with every event, so a UI can tell a new one from an old one.
    pub seq: u32,
}

/// Average balance in one corner phase, in steering-wheel degrees per g.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseBalance {
    pub phase: Phase,
    pub speed: CornerSpeed,
    /// > 0 understeer, < 0 oversteer.
    pub deg_per_g: f32,
    /// Seconds of cornering this is based on.
    pub seconds: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TyreCorner {
    pub name: String,
    pub inner: f32,
    pub middle: f32,
    pub outer: f32,
    pub pressure_kpa: Option<f32>,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveReport {
    pub driving_seconds: f32,
    pub live: Option<LiveFrame>,
    pub detected: Vec<Detected>,
    pub tyres: Vec<TyreCorner>,
    /// Understeer detection needs a few gentle curves first.
    pub calibrated: bool,
    pub balance_by_phase: Vec<PhaseBalance>,
    pub last_event: Option<LastEvent>,
}

/// Tuning knobs. Defaults are first guesses; replay recordings to tune them.
#[derive(Debug, Clone)]
pub struct Thresholds {
    /// Below this, cornering counts as "slow".
    pub slow_corner_kmh: f32,
    /// Lateral acceleration (m/s²) that counts as cornering.
    pub cornering_accel: f32,
    /// Lateral acceleration range (m/s²) used to learn the steering geometry.
    pub geometry_accel: (f32, f32),
    /// Extra steering (wheel degrees per g) that counts as understeer.
    pub understeer_deg_per_g: f32,
    pub understeer_min_s: f32,
    /// Body slip angle (degrees) that counts as a slide on its own.
    pub slide_slip_deg: f32,
    pub oversteer_min_s: f32,
    /// Wheel slower than this fraction of car speed = locking.
    pub lockup_ratio: f32,
    pub lockup_min_s: f32,
    pub wheelspin_ratio: f32,
    pub wheelspin_min_s: f32,
    /// Ride height (m) that counts as touching the ground (0 disables the detector). Both
    /// wheels of an axle must be below it; one wheel alone is a kerb.
    /// Not checked against real scraping yet; the track map tells compressions apart.
    pub bottoming_height: f32,
    /// Front wheels this much slower than the car while braking = tyres busy braking.
    pub front_slip_braking: f32,
    /// Vertical load (1 = flat road) below which the car counts as light (crest).
    pub light_load: f32,
    /// Events within this distance (m) count as the same place.
    pub place_radius_m: f32,
    /// Share of events in one place that makes a problem "one place only".
    pub one_place_share: f32,
    /// Events needed before a problem is reported.
    pub min_events: u32,
    /// Driving time before tyre hints are shown.
    pub tyre_hint_after_s: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            slow_corner_kmh: 120.0,
            cornering_accel: 0.35 * G,
            geometry_accel: (0.8, 3.0),
            understeer_deg_per_g: 30.0,
            understeer_min_s: 0.35,
            slide_slip_deg: 6.0,
            oversteer_min_s: 0.15,
            lockup_ratio: 0.85,
            lockup_min_s: 0.08,
            wheelspin_ratio: 1.12,
            wheelspin_min_s: 0.15,
            bottoming_height: 0.008,
            front_slip_braking: 0.04,
            light_load: 0.75,
            place_radius_m: 80.0,
            one_place_share: 0.7,
            min_events: 2,
            tyre_hint_after_s: 30.0,
        }
    }
}

/// Counts one event per continuous episode of a condition.
#[derive(Default, Clone)]
struct Episode {
    on_for: f32,
    off_for: f32,
    counted: bool,
}

impl Episode {
    /// Returns true when a new event starts.
    fn update(&mut self, active: bool, dt: f32, min_on: f32) -> bool {
        if active {
            self.on_for += dt;
            self.off_for = 0.0;
            if !self.counted && self.on_for >= min_on {
                self.counted = true;
                return true;
            }
        } else {
            self.off_for += dt;
            // Short gaps don't split one slide into two events.
            if self.off_for > 0.3 {
                self.on_for = 0.0;
                self.counted = false;
            }
        }
        false
    }
}

#[derive(Default, Clone)]
struct Detectors {
    understeer: Episode,
    /// Phase the current understeer episode belongs to.
    understeer_phase: Option<Phase>,
    oversteer: Episode,
    oversteer_phase: Option<Phase>,
    braking_understeer: Episode,
    /// Seconds left during which the front tyres still count as busy braking.
    braking_slip_for: f32,
    front_lock: Episode,
    rear_lock: Episode,
    wheelspin: Episode,
    bottoming: Episode,
}

type EventKey = (String, Option<Phase>, CornerSpeed);

pub struct Analyzer {
    pub thresholds: Thresholds,
    last_time: Option<f64>,
    driving_s: f32,
    detectors: Detectors,
    events: HashMap<EventKey, u32>,
    /// Lap distance of each event.
    event_places: HashMap<EventKey, Vec<f32>>,
    /// Lap length in metres, if known: places just before and after the start line are one place.
    track_length: Option<f32>,
    /// Optional channels the source provides; the others are treated as unknown.
    channels: Channels,
    last_event: Option<LastEvent>,
    /// Vertical acceleration on flat straights, to learn the "level road" reading.
    verts: Vec<f32>,
    vert_cursor: usize,
    /// (reading on level road, sign so that + = more load)
    vert_level: Option<(f32, f32)>,
    /// Seconds left during which the car still counts as light after a crest.
    light_for: f32,
    /// Learned sign so that sign * steer * yaw > 0 in normal cornering.
    sign_votes: f32,
    sign_samples: u32,
    /// Geometric yaw gain samples (yaw / (speed * steer)) from gentle curves.
    gains: Vec<f32>,
    gain_cursor: usize,
    geometry_gain: Option<f32>,
    pushes_since_geometry: u32,
    /// Smoothed understeer gradient (wheel degrees per g).
    understeer_smooth: Option<f32>,
    balance: Option<f32>,
    /// (phase, speed) -> (sum of deg/g * dt, seconds)
    phase_balance: HashMap<(Phase, CornerSpeed), (f32, f32)>,
    tyre_avg: Option<[[f32; 3]; 4]>,
    pressure_avg: Option<[f32; 4]>,
    last: Option<(Sample, Option<Phase>)>,
}

/// How long front slip under braking holds after the last slipping frame (ABS pulses).
/// Matches the gap an episode tolerates.
const BRAKING_SLIP_HOLD_S: f32 = 0.3;
const GAIN_CAPACITY: usize = 4000;
const GAINS_NEEDED: usize = 100;

impl Default for Analyzer {
    fn default() -> Self {
        Self::new(Thresholds::default())
    }
}

impl Analyzer {
    pub fn new(thresholds: Thresholds) -> Self {
        Self {
            thresholds,
            last_time: None,
            driving_s: 0.0,
            detectors: Detectors::default(),
            events: HashMap::new(),
            event_places: HashMap::new(),
            track_length: None,
            channels: Channels::ALL,
            last_event: None,
            verts: Vec::new(),
            vert_cursor: 0,
            vert_level: None,
            light_for: 0.0,
            sign_votes: 0.0,
            sign_samples: 0,
            gains: Vec::new(),
            gain_cursor: 0,
            geometry_gain: None,
            pushes_since_geometry: 0,
            understeer_smooth: None,
            balance: None,
            phase_balance: HashMap::new(),
            tyre_avg: None,
            pressure_avg: None,
            last: None,
        }
    }

    pub fn reset(&mut self) {
        let (track_length, channels) = (self.track_length, self.channels);
        *self = Self::new(self.thresholds.clone());
        self.track_length = track_length;
        self.channels = channels;
    }

    /// Optional channels the source provides. Survives [`reset`](Self::reset).
    pub fn set_channels(&mut self, channels: Channels) {
        self.channels = channels;
    }

    /// Lap length in metres (None or <= 0 = unknown). Survives [`reset`](Self::reset).
    pub fn set_track_length(&mut self, metres: Option<f32>) {
        self.track_length = metres.filter(|m| *m > 0.0);
    }

    /// Lap distance of the latest sample, if known.
    pub fn last_lap_distance(&self) -> Option<f32> {
        self.last.as_ref().map(|(s, _)| s.lap_distance).filter(|d| *d >= 0.0)
    }

    /// Steering-wheel degrees per road-wheel degree of the latest sample, when known.
    /// Divide a steering-wheel °/g value by it to get road-wheel °/g (#35).
    pub fn steering_ratio(&self) -> Option<f32> {
        self.last.as_ref().and_then(|(s, _)| s.steering_ratio())
    }

    /// Whether the latest sample had to assume the steering wheel's range.
    pub fn wheel_range_assumed(&self) -> bool {
        self.last.as_ref().is_some_and(|(s, _)| s.steer_range_deg <= 0.0)
    }

    /// Learned geometric yaw gain (rad/s per m/s per unit steering), for diagnostics.
    pub fn geometry_gain(&self) -> Option<f32> {
        self.geometry_gain
    }

    fn steer_sign(&self) -> Option<f32> {
        (self.sign_samples >= 50).then(|| if self.sign_votes >= 0.0 { 1.0 } else { -1.0 })
    }

    fn phase_of(&self, s: &Sample) -> Option<Phase> {
        if s.speed < 5.0 {
            return None;
        }
        let cornering = s.lat_accel.abs() > self.thresholds.cornering_accel;
        Some(if !cornering {
            if s.brake > 0.15 {
                Phase::Braking
            } else {
                Phase::Straight
            }
        } else if s.brake > 0.05 {
            Phase::Entry
        } else if s.throttle > 0.6 {
            Phase::Exit
        } else {
            Phase::Mid
        })
    }

    fn speed_class(&self, s: &Sample) -> CornerSpeed {
        if s.speed * 3.6 < self.thresholds.slow_corner_kmh {
            CornerSpeed::Slow
        } else {
            CornerSpeed::Fast
        }
    }

    fn record(&mut self, behavior: &str, phase: Option<Phase>, speed: CornerSpeed, at: f32) {
        let key = (behavior.to_string(), phase, speed);
        *self.events.entry(key.clone()).or_insert(0) += 1;
        if at >= 0.0 {
            self.event_places.entry(key).or_default().push(at);
        }
        let seq = self.last_event.as_ref().map_or(1, |e| e.seq + 1);
        self.last_event = Some(LastEvent {
            symptom: Symptom { behavior: behavior.into(), phase, speed, severity: Severity::Slight },
            label: String::new(),
            seq,
        });
    }

    pub fn push(&mut self, s: &Sample) {
        let masked;
        let s = if self.channels == Channels::ALL {
            s
        } else {
            masked = self.channels.mask(s);
            &masked
        };
        let dt = match self.last_time {
            Some(t) if s.time > t => (s.time - t) as f32,
            Some(t) if s.time == t => return, // duplicate frame (paused, or polled twice)
            Some(_) => {
                // Clock went backwards: the game restarted the session. Carry on from the
                // new time; keep what was learned about the car, drop half-finished events.
                self.last_time = Some(s.time);
                self.detectors = Detectors::default();
                self.balance = None;
                return;
            }
            None => {
                self.last_time = Some(s.time);
                return;
            }
        };
        self.last_time = Some(s.time);
        if dt > 0.5 {
            // Gap in data (pause, menus): start episodes fresh.
            self.detectors = Detectors::default();
            return;
        }
        let dt = dt.min(0.05);
        let t = self.thresholds.clone();
        let phase = self.phase_of(s);
        self.last = Some((s.clone(), phase));
        if phase.is_none() {
            self.balance = None;
            return;
        }
        self.driving_s += dt;
        let speed_class = self.speed_class(s);
        let lat = s.lat_accel.abs();
        let slip_deg = (s.lat_velocity.abs() / s.speed.max(1.0)).atan().to_degrees();

        // --- learn how steering and yaw relate ---
        if s.speed > 12.0 && s.steer.abs() > 0.004 && s.yaw_rate.abs() > 0.02 && slip_deg < 2.0 {
            let v = (s.steer * s.yaw_rate).signum();
            // Recent samples count more, so a wrong early guess fades out.
            self.sign_votes = self.sign_votes * 0.999 + v;
            self.sign_samples += 1;
        }
        let sign = self.steer_sign();
        if let Some(sign) = sign {
            let gentle = s.speed > 12.0
                && s.steer.abs() > 0.004
                && s.yaw_rate.abs() > 0.02
                && (t.geometry_accel.0..t.geometry_accel.1).contains(&lat)
                && s.brake < 0.2
                && slip_deg < 2.0;
            let g = sign * s.yaw_rate / (s.speed * s.steer);
            if gentle && g > 0.0 && g.is_finite() {
                if self.gains.len() < GAIN_CAPACITY {
                    self.gains.push(g);
                } else {
                    self.gains[self.gain_cursor] = g;
                    self.gain_cursor = (self.gain_cursor + 1) % GAIN_CAPACITY;
                }
            }
        }
        // --- learn the vertical reading on level road (to spot crests) ---
        if s.speed > 20.0 && lat < 1.5 && s.brake < 0.1 && s.steer.abs() < 0.05 {
            if self.verts.len() < GAIN_CAPACITY {
                self.verts.push(s.vert_accel);
            } else {
                self.verts[self.vert_cursor] = s.vert_accel;
                self.vert_cursor = (self.vert_cursor + 1) % GAIN_CAPACITY;
            }
        }
        if self.verts.len() >= 100 && (self.vert_level.is_none() || self.pushes_since_geometry >= 199) {
            let mut sorted = self.verts.clone();
            sorted.sort_by(f32::total_cmp);
            let level = sorted[sorted.len() / 2];
            // With gravity included the level reading is about 1 g; its sign tells which way is up.
            let sign = if level.abs() > 5.0 { level.signum() } else { 1.0 };
            self.vert_level = Some((level, sign));
        }
        let load = self.vert_level.map(|(level, sign)| 1.0 + sign * (s.vert_accel - level) / G);
        if matches!(load, Some(l) if l < t.light_load) {
            self.light_for = 0.3;
        } else {
            self.light_for = (self.light_for - dt).max(0.0);
        }
        // Over a crest the tyres lose load whatever the setup: balance isn't judged there.
        let light = self.light_for > 0.0;

        self.pushes_since_geometry += 1;
        if self.gains.len() >= GAINS_NEEDED && (self.geometry_gain.is_none() || self.pushes_since_geometry >= 200) {
            let mut sorted = self.gains.clone();
            sorted.sort_by(f32::total_cmp);
            self.geometry_gain = Some(sorted[sorted.len() / 2]);
            self.pushes_since_geometry = 0;
        }

        // --- balance ---
        let cornering = lat > t.cornering_accel;
        let countersteer = matches!(sign, Some(sign) if sign * s.steer * s.yaw_rate < 0.0)
            && s.yaw_rate.abs() > 0.12
            && s.steer.abs() > 0.02;
        let half_range = if s.steer_range_deg > 0.0 { s.steer_range_deg / 2.0 } else { DEFAULT_HALF_RANGE_DEG };

        // Extra steering beyond geometry, in wheel degrees per g.
        let understeer_deg_per_g = match (self.geometry_gain, cornering && !countersteer && s.speed > 12.0) {
            (Some(k), true) => {
                let geometric_steer = s.yaw_rate.abs() / (s.speed * k);
                let extra = s.steer.abs() - geometric_steer;
                Some(extra * half_range / (lat / G))
            }
            _ => None,
        };
        self.understeer_smooth = match understeer_deg_per_g {
            Some(u) => Some(self.understeer_smooth.map_or(u, |p| p + (u - p) * (dt / 0.2).min(1.0))),
            None => None,
        };

        let sliding = (countersteer && slip_deg > 2.5) || slip_deg > t.slide_slip_deg;
        let target = if sliding || (countersteer && cornering) {
            Some((slip_deg / t.slide_slip_deg).clamp(0.4, 1.0))
        } else {
            self.understeer_smooth.map(|u| (-u / (2.0 * t.understeer_deg_per_g)).clamp(-1.0, 1.0))
        };
        self.balance = match target {
            Some(b) => Some(self.balance.map_or(b, |p| p + (b - p) * (dt / 0.15).min(1.0))),
            None => None,
        };

        // Turn the corner phase into one the rules understand.
        let corner_phase = match phase {
            Some(Phase::Braking) => Phase::Entry,
            Some(Phase::Straight) | None => Phase::Mid,
            Some(p) => p,
        };

        // Front tyres busy braking (ABS / near lockup): understeer here is about braking, not the chassis.
        // ABS lets the wheels spin back up between pulses, so slip holds for a moment while the
        // brake stays pressed; otherwise one corner would count as both kinds of understeer.
        let front_mean = (s.wheel_speed[0].abs() + s.wheel_speed[1].abs()) / 2.0;
        let on_brakes = s.brake > 0.05 && s.speed > 10.0;
        if on_brakes && front_mean < s.speed * (1.0 - t.front_slip_braking) {
            self.detectors.braking_slip_for = BRAKING_SLIP_HOLD_S;
        } else {
            self.detectors.braking_slip_for = (self.detectors.braking_slip_for - dt).max(0.0);
        }
        let braking_slip = on_brakes && self.detectors.braking_slip_for > 0.0;
        let at = s.lap_distance;

        if let (Some(u), true, false) = (understeer_deg_per_g, cornering, light) {
            let bucket = if braking_slip { Phase::Braking } else { corner_phase };
            let e = self.phase_balance.entry((bucket, speed_class)).or_insert((0.0, 0.0));
            e.0 += u * dt;
            e.1 += dt;
        }

        // --- events ---
        let pushing =
            cornering && lat > 5.0 && !light && matches!(self.understeer_smooth, Some(u) if u > t.understeer_deg_per_g);
        let understeer = pushing && !braking_slip;
        if self.detectors.braking_understeer.update(pushing && braking_slip, dt, t.understeer_min_s) {
            self.record("braking_understeer", None, CornerSpeed::Both, at);
        }
        // Understeer that carries on from mid-corner into the exit counts for both phases.
        if understeer && self.detectors.understeer_phase != Some(corner_phase) {
            self.detectors.understeer = Episode::default();
            self.detectors.understeer_phase = Some(corner_phase);
        }
        if self.detectors.understeer.update(understeer, dt, t.understeer_min_s) {
            self.record("understeer", Some(corner_phase), speed_class, at);
        }

        let oversteer = s.speed > 12.0 && (lat > 4.0 || s.yaw_rate.abs() > 0.15) && sliding && !light;
        if oversteer && self.detectors.oversteer_phase != Some(corner_phase) {
            self.detectors.oversteer = Episode::default();
            self.detectors.oversteer_phase = Some(corner_phase);
        }
        if self.detectors.oversteer.update(oversteer, dt, t.oversteer_min_s) {
            self.record("oversteer", Some(corner_phase), speed_class, at);
        }

        let braking = s.brake > 0.3 && s.speed > 10.0;
        let front_min = s.wheel_speed[0].abs().min(s.wheel_speed[1].abs());
        let rear_min = s.wheel_speed[2].abs().min(s.wheel_speed[3].abs());
        if self.detectors.front_lock.update(braking && front_min < s.speed * t.lockup_ratio, dt, t.lockup_min_s) {
            self.record("front_lockup", None, CornerSpeed::Both, at);
        }
        let rear_unstable = braking && (rear_min < s.speed * t.lockup_ratio || slip_deg > 3.5);
        if self.detectors.rear_lock.update(rear_unstable, dt, t.lockup_min_s) {
            self.record("unstable_braking", None, CornerSpeed::Both, at);
        }

        let fastest = s.wheel_speed.iter().fold(0.0f32, |m, w| m.max(w.abs()));
        let spinning = s.throttle > 0.5 && s.speed > 3.0 && fastest > s.speed * t.wheelspin_ratio + 0.5;
        if self.detectors.wheelspin.update(spinning, dt, t.wheelspin_min_s) {
            self.record("wheelspin", Some(Phase::Exit), CornerSpeed::Both, at);
        }

        // Bottoming needs both wheels of an axle low at once. One wheel low (or one side of the
        // car) is a kerb strike, not a setup problem. Real recordings show only those, and their
        // vertical jolts overlap road noise, so the jolt can't tell them apart (#42, ADR-20).
        // Small negative heights are hard strikes and count as low (#50).
        let [fl, fr, rl, rr] = s.ride_height.map(|h| (RIDE_HEIGHT_UNKNOWN_BELOW..t.bottoming_height).contains(&h));
        let touching = s.speed > 25.0 && ((fl && fr) || (rl && rr));
        if self.detectors.bottoming.update(touching, dt, 0.05) {
            self.record("bottoming", None, CornerSpeed::Both, at);
        }

        // --- tyre temperature averages while driving at speed ---
        if s.speed > 15.0 && s.tyre_temp.iter().flatten().all(|v| *v > 0.0) {
            let k = (dt / 10.0).min(1.0);
            let avg = self.tyre_avg.get_or_insert(s.tyre_temp);
            for (a, n) in avg.iter_mut().flatten().zip(s.tyre_temp.iter().flatten()) {
                *a += (n - *a) * k;
            }
            if s.tyre_pressure.iter().all(|p| *p > 0.0) {
                let p = self.pressure_avg.get_or_insert(s.tyre_pressure);
                for (a, n) in p.iter_mut().zip(s.tyre_pressure.iter()) {
                    *a += (n - *a) * k;
                }
            }
        }
    }

    pub fn report(&self) -> LiveReport {
        let minutes = (self.driving_s / 60.0).max(1.0);
        let mut detected: Vec<Detected> = self
            .events
            .iter()
            .filter(|(_, n)| **n >= self.thresholds.min_events)
            .map(|((behavior, phase, speed), n)| {
                let per_minute = *n as f32 / minutes;
                let severity = if per_minute < 1.0 {
                    Severity::Slight
                } else if per_minute < 2.5 {
                    Severity::Moderate
                } else {
                    Severity::Severe
                };
                let key = (behavior.clone(), *phase, *speed);
                let places = self
                    .event_places
                    .get(&key)
                    .map(|d| cluster(d, self.thresholds.place_radius_m, self.track_length))
                    .unwrap_or_default();
                let located: u32 = places.iter().map(|p| p.count).sum();
                let one_place = located >= 3
                    && places
                        .first()
                        .is_some_and(|p| p.count as f32 >= located as f32 * self.thresholds.one_place_share);
                Detected {
                    symptom: Symptom { behavior: behavior.clone(), phase: *phase, speed: *speed, severity },
                    label: String::new(),
                    count: *n,
                    per_minute,
                    places,
                    one_place,
                    runs_checked: 0,
                    persistent: false,
                }
            })
            .collect();
        detected.sort_by(|a, b| b.count.cmp(&a.count).then(a.symptom.behavior.cmp(&b.symptom.behavior)));

        let live = self.last.as_ref().map(|(s, phase)| LiveFrame {
            speed_kmh: s.speed * 3.6,
            gear: s.gear,
            throttle: s.throttle,
            brake: s.brake,
            steer: s.steer,
            lat_g: s.lat_accel.abs() / G,
            phase: *phase,
            balance: self.balance,
        });

        let mut balance_by_phase: Vec<PhaseBalance> = self
            .phase_balance
            .iter()
            .filter(|(_, (_, secs))| *secs >= 1.0)
            .map(|((phase, speed), (sum, secs))| PhaseBalance {
                phase: *phase,
                speed: *speed,
                deg_per_g: sum / secs,
                seconds: *secs,
            })
            .collect();
        let order = |p: Phase| match p {
            Phase::Braking => 0,
            Phase::Entry => 1,
            Phase::Mid => 2,
            _ => 3,
        };
        balance_by_phase.sort_by_key(|b| (order(b.phase), b.speed == CornerSpeed::Fast));

        LiveReport {
            driving_seconds: self.driving_s,
            live,
            detected,
            tyres: self.tyre_report(),
            calibrated: self.geometry_gain.is_some(),
            balance_by_phase,
            last_event: self.last_event.clone(),
        }
    }

    fn tyre_report(&self) -> Vec<TyreCorner> {
        let Some(avg) = self.tyre_avg else { return Vec::new() };
        let show_hints = self.driving_s >= self.thresholds.tyre_hint_after_s;
        (0..4)
            .map(|i| {
                let [inner, middle, outer] = avg[i];
                let mut hints = Vec::new();
                if show_hints {
                    let spread = inner - outer;
                    if spread > 12.0 {
                        hints.push(format!(
                            "Inside {spread:.0}°C hotter than outside: probably too much negative camber."
                        ));
                    } else if spread < 2.0 {
                        hints.push("Inside isn't hotter than outside: could use more negative camber.".into());
                    }
                    let mid = middle - (inner + outer) / 2.0;
                    if mid > 4.0 {
                        hints.push("Middle hotter than the edges: pressure may be too high.".into());
                    } else if mid < -4.0 {
                        hints.push("Middle cooler than the edges: pressure may be too low.".into());
                    }
                }
                TyreCorner {
                    name: TYRE_NAMES[i].into(),
                    inner,
                    middle,
                    outer,
                    pressure_kpa: self.pressure_avg.map(|p| p[i]),
                    hints,
                }
            })
            .collect()
    }
}

/// Groups lap distances into places (events within `radius` metres of a place's first event).
/// With the lap length known, a place just before the start line and one just after it are
/// the same place.
pub(crate) fn cluster(distances: &[f32], radius: f32, lap_length: Option<f32>) -> Vec<Place> {
    let mut d: Vec<f32> = distances.iter().copied().filter(|x| *x >= 0.0).collect();
    d.sort_by(f32::total_cmp);
    let mut groups: Vec<(f32, f32, f32, u32)> = Vec::new(); // (first, last, sum, count)
    for x in d {
        match groups.last_mut() {
            Some(g) if x - g.0 <= radius => {
                g.1 = x;
                g.2 += x;
                g.3 += 1;
            }
            _ => groups.push((x, x, x, 1)),
        }
    }
    if let Some(len) = lap_length {
        if groups.len() >= 2 {
            let (head, tail) = (groups[0], groups[groups.len() - 1]);
            if head.0 + len - tail.1 <= radius {
                // Count the places after the line as one lap further on, then wrap the mean.
                let sum = tail.2 + head.2 + len * head.3 as f32;
                let last = groups.len() - 1;
                groups[last] = (tail.0, head.1 + len, sum, tail.3 + head.3);
                groups.remove(0);
            }
        }
    }
    let mut places: Vec<Place> = groups
        .into_iter()
        .map(|(_, _, sum, n)| Place {
            distance_m: match lap_length {
                Some(len) => (sum / n as f32).rem_euclid(len),
                None => sum / n as f32,
            },
            count: n,
            label: String::new(),
            feature: None,
            seen_in_runs: 0,
        })
        .collect();
    places.sort_by_key(|p| std::cmp::Reverse(p.count));
    places
}
