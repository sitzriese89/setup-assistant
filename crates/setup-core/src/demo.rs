//! Synthetic laps with known problems, for tests and for trying the app
//! without the game running.
//!
//! - `Mixed`: understeer mid-corner in the first hairpin; a front lockup,
//!   wheelspin and power oversteer at corner 4. FL runs too much camber,
//!   RL too much pressure.
//! - `Loose`: oversteer on exit of both slow corners and rear instability
//!   under braking. What a driver might start with.
//! - `Pushing`: the same car after the oversteer is fixed: understeers through
//!   the middle and exit of every corner.

use crate::live::{CarInfo, SessionKind, SourceIdentity};
use crate::telemetry::{Channels, Sample};
use crate::track::TrackInfo;

/// Identity of the demo source on `track`.
pub fn identity(track: TrackInfo) -> SourceIdentity {
    SourceIdentity {
        game: "Demo".into(),
        api_version: "1".into(),
        car: Some(CarInfo {
            id: "demo-car".into(),
            name: Some("Demo GT car".into()),
            class_id: "demo-class".into(),
            class_name: Some("Demo GT".into()),
        }),
        track: Some(track),
        session: SessionKind::Practice,
        channels: Channels::ALL,
    }
}

const DT: f64 = 0.01;
const WHEELBASE: f32 = 2.7;
/// Road-wheel angle (rad) at full steering input.
const STEER_RANGE: f32 = 0.3;
/// Steering wheel lock to lock, degrees.
const WHEEL_RANGE_DEG: f32 = 540.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Mixed,
    Loose,
    Pushing,
}

#[derive(Default, Clone, Copy)]
struct Corner {
    v_in: f32,
    v_apex: f32,
    radius: f32,
    /// Extra steering in wheel degrees per g (understeer), mid-corner and exit.
    understeer_mid: f32,
    understeer_exit: f32,
    oversteer_exit: bool,
    lock_front: bool,
    unstable_braking: bool,
    wheelspin: bool,
    /// Trail braking with the fronts slipping and the car not turning.
    braking_understeer: bool,
    /// Car goes light over a crest mid-corner and pushes (not a setup problem).
    crest_mid: bool,
    /// Scrapes the ground in the compression in this corner's braking zone.
    bottoms_in_compression: bool,
}

fn corners(variant: Variant) -> Vec<Corner> {
    let base = [
        Corner { v_in: 60.0, v_apex: 18.0, radius: 25.0, ..Default::default() },
        Corner { v_in: 50.0, v_apex: 25.0, radius: 120.0, ..Default::default() },
        Corner { v_in: 65.0, v_apex: 45.0, radius: 220.0, ..Default::default() },
        Corner { v_in: 55.0, v_apex: 20.0, radius: 35.0, ..Default::default() },
        Corner { v_in: 45.0, v_apex: 22.0, radius: 90.0, ..Default::default() },
    ];
    let mut c = base.to_vec();
    match variant {
        Variant::Mixed => {
            c[0].understeer_mid = 45.0;
            c[0].braking_understeer = true;
            c[2].crest_mid = true;
            c[3].bottoms_in_compression = true;
            c[3].oversteer_exit = true;
            c[3].lock_front = true;
            c[3].wheelspin = true;
        }
        Variant::Loose => {
            c[0].oversteer_exit = true;
            c[3].oversteer_exit = true;
            c[1].unstable_braking = true;
            for x in c.iter_mut() {
                x.understeer_mid = -8.0;
            }
        }
        Variant::Pushing => {
            for x in c.iter_mut() {
                x.understeer_mid = 45.0;
                x.understeer_exit = 40.0;
            }
        }
    }
    c
}

/// Generates one lap of samples starting at `t0`.
pub fn lap(t0: f64) -> Vec<Sample> {
    lap_variant(t0, Variant::Mixed)
}

pub fn lap_variant(t0: f64, variant: Variant) -> Vec<Sample> {
    let corners = corners(variant);
    let mut out = Vec::new();
    let mut t = t0;
    let mut v_prev = corners[corners.len() - 1].v_apex * 1.6;

    let mut dist = 0.0f32;
    let mut emit = |t: &mut f64, mut s: Sample| {
        s.time = *t;
        s.lap_distance = dist;
        dist += s.speed * DT as f32;
        *t += DT;
        out.push(s);
    };
    let steps = |secs: f64| (secs / DT).round() as usize;
    let lerp = |a: f32, b: f32, u: f32| a + (b - a) * u;

    for (i, c) in corners.iter().enumerate() {
        let dir = if i % 2 == 0 { 1.0 } else { -1.0 };

        // Straight: accelerate to the braking point, with a gentle kink in the middle.
        let n = steps(3.0);
        for k in 0..n {
            let u = k as f32 / n as f32;
            let v = lerp(v_prev, c.v_in, u);
            let mut s = base(v, 1.0, 0.0);
            if (0.35..0.65).contains(&u) {
                // About 0.2 g: every car follows its steering geometry here.
                let r = v * v / 2.0;
                corner(&mut s, v, r, -dir, 0.0);
            }
            emit(&mut t, s);
        }

        // Braking in a straight line.
        let n = steps(1.8);
        for k in 0..n {
            let u = k as f32 / n as f32;
            let v = lerp(c.v_in, c.v_apex * 1.15, u);
            let mut s = base(v, 0.0, 0.9);
            if c.lock_front && (0.3..0.5).contains(&u) {
                s.wheel_speed[0] = v * 0.6;
                s.wheel_speed[1] = v * 0.6;
            }
            if c.unstable_braking && (0.4..0.6).contains(&u) {
                s.wheel_speed[2] = v * 0.7;
                s.wheel_speed[3] = v * 0.7;
            }
            emit(&mut t, s);
        }

        // Entry: trail braking, radius tightening.
        let n = steps(1.2);
        for k in 0..n {
            let u = k as f32 / n as f32;
            let v = lerp(c.v_apex * 1.15, c.v_apex, u);
            let r = c.radius * (3.0 - 2.0 * u);
            let mut s = base(v, 0.0, 0.3 * (1.0 - u) + 0.06);
            if c.braking_understeer {
                corner(&mut s, v, r, dir, 50.0);
                s.wheel_speed[0] = v * 0.92;
                s.wheel_speed[1] = v * 0.92;
            } else {
                corner(&mut s, v, r, dir, 0.0);
            }
            emit(&mut t, s);
        }

        // Mid-corner.
        let n = steps(1.5);
        for k in 0..n {
            let u = k as f32 / n as f32;
            let mut s = base(c.v_apex, 0.3, 0.0);
            let crest = c.crest_mid && (0.3..0.6).contains(&u);
            corner(&mut s, c.v_apex, c.radius, dir, if crest { 60.0 } else { c.understeer_mid });
            if crest {
                s.vert_accel -= 4.5;
            }
            emit(&mut t, s);
        }

        // Exit: throttle, radius opening.
        let n = steps(1.5);
        for k in 0..n {
            let u = k as f32 / n as f32;
            let v = lerp(c.v_apex, c.v_apex * 1.6, u);
            let r = c.radius * (1.0 + 2.0 * u);
            let mut s = base(v, 0.85, 0.0);
            corner(&mut s, v, r, dir, c.understeer_exit);
            if c.wheelspin && (0.2..0.4).contains(&u) {
                s.wheel_speed[2] = v * 1.25;
                s.wheel_speed[3] = v * 1.25;
            }
            if c.oversteer_exit && (0.35..0.6).contains(&u) {
                // Rear steps out: more yaw, driver countersteers, body slides.
                s.yaw_rate *= 1.5;
                s.steer = -s.steer * 0.3;
                s.lat_velocity = dir * v * 7f32.to_radians().tan();
            }
            emit(&mut t, s);
        }
        v_prev = c.v_apex * 1.6;
    }
    add_elevation(&mut out, &corners);
    out
}

/// Samples per corner block: straight 300, braking 180, entry 120, mid 150, exit 150.
const BLOCK: usize = 900;

/// Gives the lap an elevation profile: gentle hills, a crest on the straight
/// before corner 2 and a compression in corner 4's braking zone. Vertical
/// acceleration follows from it (v² times the road's curvature).
fn add_elevation(out: &mut [Sample], corners: &[Corner]) {
    let len = out.last().map_or(1.0, |s| s.lap_distance + s.speed * DT as f32);
    let crest_at = out[BLOCK + 45].lap_distance;
    let dip_at = out[3 * BLOCK + 300 + 90].lap_distance;
    let bump = |x: f32, at: f32, height: f32, sigma: f32| {
        let z = (x - at) / sigma;
        let e = (-z * z / 2.0).exp();
        (height * e, height / (sigma * sigma) * (z * z - 1.0) * e)
    };
    for s in out.iter_mut() {
        let x = s.lap_distance;
        let w = std::f32::consts::TAU / len;
        let (h1, c1) = (8.0 * (w * x).sin(), -8.0 * w * w * (w * x).sin());
        let (h2, c2) = bump(x, crest_at, 3.0, 25.0);
        let (h3, c3) = bump(x, dip_at, -1.5, 20.0);
        s.elevation = Some(100.0 + h1 + h2 + h3);
        s.vert_accel += s.speed * s.speed * (c1 + c2 + c3);
        if corners[3].bottoms_in_compression && (x - dip_at).abs() < 8.0 {
            s.ride_height = [0.004, 0.004, 0.03, 0.03];
        }
    }
}

/// The demo track, for the track map.
pub fn track_info() -> TrackInfo {
    let lap = lap_variant(0.0, Variant::Mixed);
    let len = lap.last().map_or(0.0, |s| s.lap_distance + s.speed * DT as f32);
    TrackInfo { id: "demo".into(), name: "Demo track".into(), length_m: len }
}

fn base(v: f32, throttle: f32, brake: f32) -> Sample {
    Sample {
        speed: v,
        throttle,
        brake,
        wheel_speed: [v; 4],
        tyre_temp: [[95.0, 86.0, 80.0], [90.0, 88.0, 84.0], [85.0, 93.0, 84.0], [89.0, 87.0, 83.0]],
        tyre_pressure: [182.0, 180.0, 188.0, 179.0],
        ride_height: [0.05, 0.05, 0.07, 0.07],
        gear: ((v / 12.0) as i32 + 1).clamp(1, 6),
        steer_range_deg: WHEEL_RANGE_DEG,
        // Like an accelerometer: reads +1 g on level road.
        vert_accel: 9.81,
        ..Default::default()
    }
}

/// Fills in steering, yaw and lateral acceleration for steady cornering.
/// `understeer_deg_per_g` adds that much steering-wheel angle per g on top of geometry.
fn corner(s: &mut Sample, v: f32, r: f32, dir: f32, understeer_deg_per_g: f32) {
    let lat_g = v * v / r / 9.81;
    let geometric = WHEELBASE / r / STEER_RANGE;
    let extra = understeer_deg_per_g / (WHEEL_RANGE_DEG / 2.0) * lat_g;
    s.steer = (dir * (geometric + extra)).clamp(-1.0, 1.0);
    s.yaw_rate = dir * v / r;
    s.lat_accel = dir * v * v / r;
}

/// Plays demo laps in real time (times `speedup`).
pub struct DemoSource {
    start: std::time::Instant,
    speedup: f64,
    variant: Variant,
    buffer: Vec<Sample>,
    next: usize,
    lap_start: f64,
    track: TrackInfo,
}

impl DemoSource {
    pub fn new(speedup: f64, variant: Variant) -> Self {
        Self {
            start: std::time::Instant::now(),
            speedup,
            variant,
            buffer: lap_variant(0.0, variant),
            next: 0,
            lap_start: 0.0,
            track: track_info(),
        }
    }

    pub fn track(&self) -> TrackInfo {
        self.track.clone()
    }

    /// The demo is its own game, car and track, with every channel.
    pub fn identity(&self) -> SourceIdentity {
        identity(self.track.clone())
    }

    /// Switches the car for the next lap (e.g. after "next run").
    pub fn set_variant(&mut self, variant: Variant) {
        self.variant = variant;
        // Start a fresh lap with the new car right away.
        let t = self.buffer.get(self.next).map_or(self.lap_start, |s| s.time);
        self.lap_start = t;
        self.buffer = lap_variant(t, variant);
        self.next = 0;
    }

    /// All samples that are due by now.
    pub fn due(&mut self) -> Vec<Sample> {
        let now = self.start.elapsed().as_secs_f64() * self.speedup;
        let mut out = Vec::new();
        loop {
            if self.next >= self.buffer.len() {
                self.lap_start = self.buffer.last().map_or(0.0, |s| s.time + DT);
                self.buffer = lap_variant(self.lap_start, self.variant);
                self.next = 0;
            }
            let s = &self.buffer[self.next];
            if s.time > now {
                break;
            }
            out.push(s.clone());
            self.next += 1;
        }
        out
    }
}
