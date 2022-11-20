use serde::Deserialize;

fn lerp<const N: usize>(a: &[f64; N], b: &[f64; N], t: f64) -> [f64; N] {
    let mut out = [0f64; N];
    for i in 0..N {
        out[i] = a[i] + (b[i] - a[i]) * t;
    }
    out
}

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
            let last_two = &self.spawns[self.spawns.len()-1..self.spawns.len()];
            let a_pos = last_two[0].pos;
            let b_pos = last_two[1].pos;
            let a_rot = last_two[0].rot;
            let b_rot = last_two[1].rot;
            let extrapolated_pos = lerp(&a_pos, &b_pos, grid_id as f64 + 1.0);
            let extrapolated_rot = lerp(&a_rot, &b_rot, grid_id as f64 + 1.0);
            return Spawn {
                pos: extrapolated_pos,
                rot: extrapolated_rot,
            };
        }
        Spawn::default()
    }
}
