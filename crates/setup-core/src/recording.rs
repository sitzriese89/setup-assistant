//! Saves telemetry samples as CSV so real laps can be replayed to tune the detectors.
//! About 1.5 MB per minute of driving.

use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use crate::telemetry::Sample;

const HEADER: &str = "time,speed,throttle,brake,steer,yaw_rate,lat_accel,lat_velocity,\
ws_fl,ws_fr,ws_rl,ws_rr,\
t_fl_in,t_fl_mid,t_fl_out,t_fr_in,t_fr_mid,t_fr_out,t_rl_in,t_rl_mid,t_rl_out,t_rr_in,t_rr_mid,t_rr_out,\
p_fl,p_fr,p_rl,p_rr,rh_fl,rh_fr,rh_rl,rh_rr,gear,steer_range_deg,vert_accel,lap_distance,elevation";

pub struct Recorder {
    out: BufWriter<File>,
}

impl Recorder {
    pub fn create(path: &Path) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut out = BufWriter::new(File::create(path)?);
        writeln!(out, "{HEADER}")?;
        Ok(Self { out })
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
        writeln!(self.out, "{}", f.join(","))
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// Reads a recording back. Rows that don't parse are skipped. Older files
/// without vertical acceleration and lap distance read those as unknown.
pub fn read(path: &Path) -> io::Result<Vec<Sample>> {
    let mut out = Vec::new();
    for line in BufReader::new(File::open(path)?).lines().skip(1) {
        let line = line?;
        let cols: Vec<&str> = line.split(',').collect();
        let v: Vec<f64> = cols.iter().take(36).filter_map(|x| x.trim().parse().ok()).collect();
        if v.len() < 34 {
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
            ..Default::default()
        };
        if v.len() >= 36 {
            s.vert_accel = f(34);
            s.lap_distance = f(35);
            s.elevation = cols.get(36).and_then(|x| x.trim().parse::<f32>().ok()).filter(|e| e.is_finite());
        } else {
            s.lap_distance = -1.0;
        }
        for t in 0..4 {
            s.tyre_temp[t] = [f(12 + t * 3), f(13 + t * 3), f(14 + t * 3)];
            s.tyre_pressure[t] = f(24 + t);
            s.ride_height[t] = f(28 + t);
        }
        out.push(s);
    }
    Ok(out)
}
