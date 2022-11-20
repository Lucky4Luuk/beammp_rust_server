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
}

impl Car {
    pub fn new(car_json: String) -> Self {
        Self {
            car_json: car_json,
            is_corrected: false,

            offtrack_start: None,
            in_pits: false,

            ..Default::default()
        }
    }
}
