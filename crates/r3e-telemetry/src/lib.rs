//! RaceRoom telemetry through the official `$R3E` shared memory.

pub mod layout;

use layout::Shared;
use setup_core::live::{Poll, Source};
use setup_core::telemetry::Sample;

/// Converts one RaceRoom frame into a game-agnostic sample.
pub fn to_sample(d: &Shared) -> Sample {
    // Copy out of the packed struct before use (no references to packed fields).
    let p = d.player;
    let local_velocity = p.local_velocity;
    let local_accel = p.local_acceleration;
    let ang = p.local_angular_velocity;
    let ride = p.ride_height;
    let temps = d.tire_temp;
    let tire_speed = d.tire_speed;
    let pressure = d.tire_pressure;

    // RaceRoom gives [left, center, right] per tyre, seen from the car. On left
    // tyres the left edge is the outside; on right tyres it's the inside.
    let mut tyre_temp = [[0.0f32; 3]; 4];
    for (i, t) in temps.iter().enumerate() {
        let [l, c, r] = t.current_temp;
        let left_side = i == 0 || i == 2;
        tyre_temp[i] = if left_side { [r, c, l] } else { [l, c, r] };
    }

    Sample {
        time: p.game_simulation_time,
        speed: d.car_speed.abs(),
        throttle: d.throttle.clamp(0.0, 1.0),
        brake: d.brake.clamp(0.0, 1.0),
        steer: d.steer_input_raw,
        yaw_rate: ang.y as f32,
        lat_accel: local_accel.x as f32,
        lat_velocity: local_velocity.x as f32,
        wheel_speed: tire_speed.map(f32::abs),
        tyre_temp,
        tyre_pressure: pressure,
        ride_height: ride.map(|h| h as f32),
        gear: d.gear,
        steer_range_deg: if d.steer_wheel_range_degrees > 0 { d.steer_wheel_range_degrees as f32 } else { 0.0 },
        vert_accel: local_accel.y as f32,
        lap_distance: d.lap_distance,
        elevation: Some(p.position.y as f32),
    }
}

/// Polls RaceRoom's shared memory. Reconnects on its own when the game starts.
pub struct R3eSource {
    track: Option<setup_core::track::TrackInfo>,
    map: Option<platform::Mapping>,
    last_tick: i32,
    next_attempt: std::time::Instant,
}

impl Default for R3eSource {
    fn default() -> Self {
        Self { track: None, map: None, last_tick: i32::MIN, next_attempt: std::time::Instant::now() }
    }
}

impl R3eSource {
    pub fn new() -> Self {
        Self::default()
    }
}

fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn track_info(d: &Shared) -> Option<setup_core::track::TrackInfo> {
    let (track, layout, length) = (d.track_id, d.layout_id, d.layout_length);
    if track <= 0 || length <= 0.0 {
        return None;
    }
    let (name, layout_name) = (text(&d.track_name), text(&d.layout_name));
    Some(setup_core::track::TrackInfo {
        id: format!("r3e-{track}-{layout}"),
        name: if layout_name.is_empty() { name } else { format!("{name} · {layout_name}") },
        length_m: length,
    })
}

impl Source for R3eSource {
    fn track(&self) -> Option<setup_core::track::TrackInfo> {
        self.track.clone()
    }

    fn poll(&mut self) -> Poll {
        if self.map.is_none() {
            if std::time::Instant::now() < self.next_attempt {
                return Poll::NotRunning;
            }
            self.next_attempt = std::time::Instant::now() + std::time::Duration::from_secs(1);
            self.map = platform::Mapping::open();
            if self.map.is_none() {
                return Poll::NotRunning;
            }
        }
        let Some(d) = self.map.as_ref().and_then(|m| m.read()) else {
            self.map = None;
            return Poll::NotRunning;
        };
        if d.version_major != layout::VERSION_MAJOR {
            return Poll::NotRunning;
        }
        // The memory stays mapped for a moment after the game quits; a frozen
        // tick counter while "driving" is handled by the session's 1 s timeout.
        let driving = d.game_paused == 0
            && d.game_in_menus == 0
            && d.game_in_replay == 0
            && d.game_player_in_garage == 0
            && d.control_type == 0;
        if !driving {
            return Poll::Idle;
        }
        self.track = track_info(&d);
        let tick = d.player.game_simulation_ticks;
        if tick == self.last_tick {
            return Poll::Samples(Vec::new());
        }
        self.last_tick = tick;
        Poll::Samples(vec![to_sample(&d)])
    }
}

#[cfg(windows)]
mod platform {
    use super::layout::{Shared, SHARED_MEMORY_NAME};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
    };

    pub struct Mapping {
        handle: HANDLE,
        view: MEMORY_MAPPED_VIEW_ADDRESS,
    }

    // The mapping is read-only and only used from the telemetry thread.
    unsafe impl Send for Mapping {}

    impl Mapping {
        pub fn open() -> Option<Self> {
            let name: Vec<u16> = SHARED_MEMORY_NAME.encode_utf16().chain(Some(0)).collect();
            unsafe {
                let handle = OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr());
                if handle.is_null() {
                    return None;
                }
                let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, std::mem::size_of::<Shared>());
                if view.Value.is_null() {
                    CloseHandle(handle);
                    return None;
                }
                Some(Self { handle, view })
            }
        }

        pub fn read(&self) -> Option<Shared> {
            // The game writes while we read; a torn frame is possible but harmless
            // for these statistics.
            Some(unsafe { std::ptr::read_unaligned(self.view.Value as *const Shared) })
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe {
                UnmapViewOfFile(self.view);
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::layout::Shared;

    /// RaceRoom only runs on Windows (or under Proton, where the shared memory
    /// lives inside the Wine prefix). Not supported yet.
    pub struct Mapping;

    impl Mapping {
        pub fn open() -> Option<Self> {
            None
        }
        pub fn read(&self) -> Option<Shared> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::layout::*;
    use std::mem::{offset_of, size_of};

    /// Offsets computed from r3e.h v3.6 with pack(1).
    #[test]
    fn offsets_match_r3e_h() {
        assert_eq!(offset_of!(Shared, player), 40);
        assert_eq!(offset_of!(Shared, player) + offset_of!(PlayerData, game_simulation_time), 48);
        assert_eq!(offset_of!(Shared, player) + offset_of!(PlayerData, local_velocity), 104);
        assert_eq!(offset_of!(Shared, player) + offset_of!(PlayerData, local_acceleration), 152);
        assert_eq!(offset_of!(Shared, player) + offset_of!(PlayerData, local_angular_velocity), 272);
        assert_eq!(offset_of!(Shared, player) + offset_of!(PlayerData, ride_height), 488);
        assert_eq!(offset_of!(Shared, track_name), 600);
        assert_eq!(offset_of!(Shared, flags), 932);
        assert_eq!(offset_of!(Shared, vehicle_info), 1196);
        assert_eq!(offset_of!(Shared, control_type), 1388);
        assert_eq!(offset_of!(Shared, car_speed), 1392);
        assert_eq!(offset_of!(Shared, gear), 1408);
        assert_eq!(offset_of!(Shared, throttle), 1500);
        assert_eq!(offset_of!(Shared, brake), 1508);
        assert_eq!(offset_of!(Shared, steer_input_raw), 1524);
        assert_eq!(offset_of!(Shared, tire_speed), 1648);
        assert_eq!(offset_of!(Shared, tire_pressure), 1712);
        assert_eq!(offset_of!(Shared, tire_temp), 1744);
        assert_eq!(offset_of!(Shared, brake_temp), 1856);
        assert_eq!(offset_of!(Shared, tire_load), 1968);
        assert_eq!(offset_of!(Shared, num_cars), 2008);
        assert_eq!(size_of::<Shared>(), 2012);
    }

    #[test]
    fn left_tyres_flip_inner_and_outer() {
        let mut d: Shared = unsafe { std::mem::zeroed() };
        for i in 0..4 {
            d.tire_temp[i].current_temp = [1.0, 2.0, 3.0]; // left, center, right
        }
        let s = super::to_sample(&d);
        assert_eq!(s.tyre_temp[0], [3.0, 2.0, 1.0], "FL: inner is the right edge");
        assert_eq!(s.tyre_temp[1], [1.0, 2.0, 3.0], "FR: inner is the left edge");
    }
}
