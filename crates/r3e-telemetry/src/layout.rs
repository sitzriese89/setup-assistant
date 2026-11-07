//! Mirror of RaceRoom's `r3e.h` (shared memory version 3.6), up to and including
//! `num_cars`. The header uses `#pragma pack(push, 1)`, hence `packed`.
//! Source: https://github.com/sector3studios/r3e-api (sample-c/src/r3e.h)
//!
//! Field order and types must match the header exactly; `tests::offsets_match_r3e_h`
//! checks the offsets against values printed by MSVC's offsetof().

#![allow(dead_code)]

pub const VERSION_MAJOR: i32 = 3;
pub const SHARED_MEMORY_NAME: &str = "$R3E";

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Vec3F32 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Vec3F64 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct OriF32 {
    pub pitch: f32,
    pub yaw: f32,
    pub roll: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct PlayerData {
    pub user_id: i32,
    /// 1 tick = 1/400 s
    pub game_simulation_ticks: i32,
    pub game_simulation_time: f64,
    pub position: Vec3F64,
    pub velocity: Vec3F64,
    /// m/s, +X left, +Y up, +Z back
    pub local_velocity: Vec3F64,
    pub acceleration: Vec3F64,
    /// m/s²
    pub local_acceleration: Vec3F64,
    pub orientation: Vec3F64,
    pub rotation: Vec3F64,
    pub angular_acceleration: Vec3F64,
    pub angular_velocity: Vec3F64,
    /// rad/s
    pub local_angular_velocity: Vec3F64,
    pub local_g_force: Vec3F64,
    pub steering_force: f64,
    pub steering_force_percentage: f64,
    pub engine_torque: f64,
    pub current_downforce: f64,
    pub voltage: f64,
    pub ers_level: f64,
    pub power_mgu_h: f64,
    pub power_mgu_k: f64,
    pub torque_mgu_k: f64,
    pub suspension_deflection: [f64; 4],
    pub suspension_velocity: [f64; 4],
    pub camber: [f64; 4],
    /// m
    pub ride_height: [f64; 4],
    pub front_wing_height: f64,
    pub front_roll_angle: f64,
    pub rear_roll_angle: f64,
    pub third_spring_suspension_deflection_front: f64,
    pub third_spring_suspension_velocity_front: f64,
    pub third_spring_suspension_deflection_rear: f64,
    pub third_spring_suspension_velocity_rear: f64,
    pub unused1: f64,
    pub unused2: f64,
    pub unused3: f64,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Flags {
    pub yellow: i32,
    pub yellow_caused_it: i32,
    pub yellow_overtake: i32,
    pub yellow_positions_gained: i32,
    pub sector_yellow: [i32; 3],
    pub closest_yellow_distance_into_track: f32,
    pub blue: i32,
    pub black: i32,
    pub green: i32,
    pub checkered: i32,
    pub white: i32,
    pub black_and_white: i32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct CarDamage {
    pub engine: f32,
    pub transmission: f32,
    pub aerodynamics: f32,
    pub suspension: f32,
    pub unused1: f32,
    pub unused2: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct CutTrackPenalties {
    pub drive_through: f32,
    pub stop_and_go: f32,
    pub pit_stop: f32,
    pub time_deduction: f32,
    pub slow_down: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Drs {
    pub equipped: i32,
    pub available: i32,
    pub num_activations_left: i32,
    pub engaged: i32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct PushToPass {
    pub available: i32,
    pub engaged: i32,
    pub amount_left: i32,
    pub engaged_time_left: f32,
    pub wait_time_left: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TireTemp {
    /// °C, [left, center, right]
    pub current_temp: [f32; 3],
    pub optimal_temp: f32,
    pub cold_temp: f32,
    pub hot_temp: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct BrakeTemp {
    pub current_temp: f32,
    pub optimal_temp: f32,
    pub cold_temp: f32,
    pub hot_temp: f32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct AidSettings {
    pub abs: i32,
    pub tc: i32,
    pub esp: i32,
    pub countersteer: i32,
    pub cornering: i32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DriverInfo {
    pub name: [u8; 64],
    pub car_number: i32,
    pub class_id: i32,
    pub model_id: i32,
    pub team_id: i32,
    pub livery_id: i32,
    pub manufacturer_id: i32,
    pub user_id: i32,
    pub slot_id: i32,
    pub class_performance_index: i32,
    pub engine_type: i32,
    pub car_width: f32,
    pub car_length: f32,
    pub rating: f32,
    pub reputation: f32,
    pub unused1: f32,
    pub unused2: f32,
}

/// The start of `r3e_shared`, up to `num_cars` (the per-driver array after it isn't needed).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Shared {
    pub version_major: i32,
    pub version_minor: i32,
    pub all_drivers_offset: i32,
    pub driver_data_size: i32,
    pub game_mode: i32,
    pub game_paused: i32,
    pub game_in_menus: i32,
    pub game_in_replay: i32,
    pub game_using_vr: i32,
    pub game_player_in_garage: i32,

    pub player: PlayerData,

    pub track_name: [u8; 64],
    pub layout_name: [u8; 64],
    pub track_id: i32,
    pub layout_id: i32,
    pub layout_length: f32,
    pub sector_start_factors: [f32; 3],
    pub race_session_laps: [i32; 3],
    pub race_session_minutes: [i32; 3],
    pub event_index: i32,
    pub session_type: i32,
    pub session_iteration: i32,
    pub session_length_format: i32,
    pub session_pit_speed_limit: f32,
    pub session_phase: i32,
    pub start_lights: i32,
    pub tire_wear_active: i32,
    pub fuel_use_active: i32,
    pub number_of_laps: i32,
    pub session_time_duration: f32,
    pub session_time_remaining: f32,
    pub max_incident_points: i32,
    pub start_mode: i32,
    pub start_group_count: i32,
    pub pit_window_status: i32,
    pub pit_window_start: i32,
    pub pit_window_end: i32,
    pub in_pitlane: i32,
    pub pit_menu_selection: i32,
    pub pit_menu_state: [i32; 12],
    pub pit_state: i32,
    pub pit_total_duration: f32,
    pub pit_elapsed_time: f32,
    pub pit_action: i32,
    pub num_pitstops: i32,
    pub pit_min_duration_total: f32,
    pub pit_min_duration_left: f32,

    pub flags: Flags,

    pub position: i32,
    pub position_class: i32,
    pub finish_status: i32,
    pub cut_track_warnings: i32,
    pub penalties: CutTrackPenalties,
    pub num_penalties: i32,
    pub completed_laps: i32,
    pub current_lap_valid: i32,
    pub track_sector: i32,
    pub lap_distance: f32,
    pub lap_distance_fraction: f32,
    pub lap_time_best_leader: f32,
    pub lap_time_best_leader_class: f32,
    pub session_best_lap_sector_times: [f32; 3],
    pub lap_time_best_self: f32,
    pub sector_time_best_self: [f32; 3],
    pub lap_time_previous_self: f32,
    pub sector_time_previous_self: [f32; 3],
    pub lap_time_current_self: f32,
    pub sector_time_current_self: [f32; 3],
    pub lap_time_delta_leader: f32,
    pub lap_time_delta_leader_class: f32,
    pub time_delta_front: f32,
    pub time_delta_behind: f32,
    pub time_delta_best_self: f32,
    pub best_individual_sector_time_self: [f32; 3],
    pub best_individual_sector_time_leader: [f32; 3],
    pub best_individual_sector_time_leader_class: [f32; 3],
    pub incident_points: i32,
    pub lap_valid_state: i32,
    pub prev_lap_valid: i32,
    pub discharge_rate: f32,
    pub brake_regen: f32,
    pub unused1: f32,

    pub vehicle_info: DriverInfo,
    pub player_name: [u8; 64],
    /// 0 = player, 1 = AI, 2 = remote, 3 = replay
    pub control_type: i32,
    /// m/s
    pub car_speed: f32,
    pub engine_rps: f32,
    pub max_engine_rps: f32,
    pub upshift_rps: f32,
    /// -2 = N/A, -1 = reverse, 0 = neutral, 1.. = gears
    pub gear: i32,
    pub num_gears: i32,
    pub car_cg_location: Vec3F32,
    pub car_orientation: OriF32,
    pub local_acceleration: Vec3F32,
    pub total_mass: f32,
    pub fuel_left: f32,
    pub fuel_capacity: f32,
    pub fuel_per_lap: f32,
    pub virtual_energy_left: f32,
    pub virtual_energy_capacity: f32,
    pub virtual_energy_per_lap: f32,
    pub engine_temp: f32,
    pub engine_oil_temp: f32,
    pub fuel_pressure: f32,
    pub engine_oil_pressure: f32,
    pub turbo_pressure: f32,
    pub throttle: f32,
    pub throttle_raw: f32,
    pub brake: f32,
    pub brake_raw: f32,
    pub clutch: f32,
    pub clutch_raw: f32,
    /// -1..1
    pub steer_input_raw: f32,
    pub steer_lock_degrees: i32,
    pub steer_wheel_range_degrees: i32,
    pub aid_settings: AidSettings,
    pub drs: Drs,
    pub pit_limiter: i32,
    pub push_to_pass: PushToPass,
    pub brake_bias: f32,
    pub drs_num_activations_total: i32,
    pub ptp_num_activations_total: i32,
    pub battery_soc: f32,
    pub water_left: f32,
    pub abs_setting: i32,
    pub headlights: i32,
    pub steer_wheel_max_rotation: i32,
    pub tire_type: i32,
    pub tire_rps: [f32; 4],
    /// m/s
    pub tire_speed: [f32; 4],
    pub tire_grip: [f32; 4],
    pub tire_wear: [f32; 4],
    pub tire_flatspot: [i32; 4],
    /// kPa
    pub tire_pressure: [f32; 4],
    pub tire_dirt: [f32; 4],
    pub tire_temp: [TireTemp; 4],
    pub tire_type_front: i32,
    pub tire_type_rear: i32,
    pub tire_subtype_front: i32,
    pub tire_subtype_rear: i32,
    pub brake_temp: [BrakeTemp; 4],
    pub brake_pressure: [f32; 4],
    pub traction_control_setting: i32,
    pub engine_map_setting: i32,
    pub engine_brake_setting: i32,
    pub traction_control_percent: f32,
    pub tire_on_mtrl: [i32; 4],
    pub tire_load: [f32; 4],
    pub car_damage: CarDamage,
    pub num_cars: i32,
}
