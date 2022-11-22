use nalgebra::*;

use std::time::Instant;

#[derive(Default, Clone, Debug)]
pub struct Car {
    pub car_json: String,

    pub pos: Vector3<f64>,
    pub rot: Quaternion<f64>,
    pub vel: Vector3<f64>,
    pub rvel: Vector3<f64>,
    pub tim: f64,
    pub ping: f64,

    pub needs_packet: bool,
    pub is_corrected: bool,

    pub offtrack_start: Option<Instant>,
    pub in_pits: bool,
    pub intersects_finish: bool,

    pub hitbox_half: [f32; 2],

    pub latest_angle_to_track: f32,
    pub latest_vel_angle_to_track: f32,
    pub laps: usize,
}

impl Car {
    pub fn new(car_json: String) -> Self {
        Self {
            car_json: car_json,
            is_corrected: false,

            offtrack_start: None,
            in_pits: false,
            intersects_finish: false,

            hitbox_half: [1.0, 1.0],

            latest_angle_to_track: 0.0,
            latest_vel_angle_to_track: 0.0,
            laps: 0,

            ..Default::default()
        }
    }
}
