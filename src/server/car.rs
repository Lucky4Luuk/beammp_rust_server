use nalgebra::*;

use std::time::{Instant, Duration};

#[derive(Default, Clone, Debug)]
pub struct Car {
    pub car_json: String,

    pub pos: Vector3<f64>,
    pub rot: Quaternion<f64>,
    pub vel: Vector3<f64>,
    pub rvel: Vector3<f64>,
    pub tim: f64,
    pub ping: f64,
    pub last_pos_update: Option<Instant>,

    pub offtrack_start: Option<Instant>,
    pub in_pits: bool,
    pub intersects_cp: bool,

    pub hitbox_half: [f32; 3],

    pub latest_angle_to_track: f32,
    pub latest_vel_angle_to_track: f32,

    pub laps: usize,
    pub laps_ui_dirty: bool,
    pub lap_start: Option<Instant>,
    pub lap_times: Vec<Duration>,

    pub next_checkpoint: usize,
    pub active_checkpoint: usize,
    pub last_progress: f32,
}

impl Car {
    pub fn new(car_json: String) -> Self {
        Self {
            car_json: car_json,

            offtrack_start: None,
            in_pits: false,
            intersects_cp: false,

            hitbox_half: [1.0, 1.0, 1.0],

            latest_angle_to_track: 0.0,
            latest_vel_angle_to_track: 0.0,

            laps: 0,
            laps_ui_dirty: true,
            lap_start: None,
            lap_times: Vec::new(),

            next_checkpoint: 0,
            active_checkpoint: 0,
            last_progress: 0.0,

            ..Default::default()
        }
    }

    pub fn add_lap_time(&mut self, duration: Duration) {
        debug!("lap time: {}:{}.{}", (duration.as_secs_f32() / 60.0).floor(), (duration.as_secs_f32() % 60.0) as usize, duration.subsec_millis());
        self.lap_times.push(duration);
    }

    pub fn pos(&self) -> Vector3<f64> {
        self.pos + self.vel * self.last_pos_update.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)
    }

    pub fn rotation(&self) -> Quaternion<f64> {
        let t = self.last_pos_update.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
        self.rot + UnitQuaternion::from_euler_angles(self.rvel.x * t, self.rvel.y * t, self.rvel.z * t).quaternion()
    }
}
