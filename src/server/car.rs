use glam::*;

#[derive(Default, Clone)]
pub struct Car {
    pub car_json: String,

    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub rvel: Vec3,
}

impl Car {
    pub fn new(car_json: String) -> Self {
        Self {
            car_json: car_json,

            ..Default::default()
        }
    }
}
