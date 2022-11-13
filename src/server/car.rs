pub struct Car {
    car_json: String,
}

impl Car {
    pub fn new(car_json: String) -> Self {
        Self {
            car_json: car_json,
        }
    }
}
