use serde::Deserialize;

use parry2d::query::intersection_test;
use parry2d::shape::{Triangle as PTriangle, Cuboid};
use parry2d::math::{Vector, Isometry, Point};

#[derive(Deserialize, Debug)]
struct Triangle {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub c: [f32; 2],
}

#[derive(Deserialize)]
pub struct TrackLimits {
    triangles: Vec<Triangle>,
}

impl TrackLimits {
    pub fn check_limits(&self, client_pos: [f32; 2], client_size_half: [f32; 2]) -> bool {
        let client_cuboid = Cuboid::new(Vector::new(client_size_half[0], client_size_half[1]));
        let client_transform = Isometry::new(Vector::new(client_pos[0], client_pos[1]), 0.0);

        let tri_transform = Isometry::new(Vector::new(0.0, 0.0), 0.0);

        for tri in &self.triangles {
            let tri_shape = PTriangle::new(
                Point::new(tri.a[0], tri.a[1]),
                Point::new(tri.b[0], tri.b[1]),
                Point::new(tri.c[0], tri.c[1])
            );
            match intersection_test(&client_transform, &client_cuboid, &tri_transform, &tri_shape) {
                Ok(hit) => if hit {
                    return true;
                },
                Err(e) => error!("error: {:?}", e),
            }
        }
        false
    }
}
