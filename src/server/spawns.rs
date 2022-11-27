use serde::Deserialize;

use crate::util::*;

#[derive(Deserialize)]
pub struct Spawns {
    pub extrapolate: bool,
    pub spawns: Vec<Spawn>,
}

#[derive(Deserialize, Default, Copy, Clone)]
pub struct Spawn {
    pub pos: [f64; 3],
    pub rot: [f64; 4],
}

impl Spawns {
    pub fn get_client_spawn(&self, grid_id: u8) -> Spawn {
        if (grid_id as usize) < self.spawns.len() {
            return self.spawns[grid_id as usize];
        } else if self.extrapolate && self.spawns.len() > 1 {
            let a_pos = &self.spawns[0].pos;
            let b_pos = &self.spawns[1].pos;
            let a_rot = &self.spawns[0].rot;
            let b_rot = &self.spawns[1].rot;
            let extrapolated_pos = lerp(&a_pos, &b_pos, grid_id as f64);
            let extrapolated_rot = lerp(&a_rot, &b_rot, grid_id as f64);
            return Spawn {
                pos: extrapolated_pos,
                rot: extrapolated_rot,
            };
        }
        Spawn::default()
    }
}
