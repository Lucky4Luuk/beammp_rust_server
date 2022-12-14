use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub network: NetworkSettings,
    pub game: GameSettings,
    pub event: EventSettings,
}

#[derive(Deserialize)]
pub struct NetworkSettings {
    pub port: Option<u16>,
    pub overlay_port: Option<u16>,
}

#[derive(Deserialize)]
pub struct GameSettings {
    pub map: String,
    pub map_limits: Option<String>,
    pub map_limits_pit: Option<String>,
    pub map_limits_pit_exit: Option<String>,
    pub map_finish: Option<String>,

    pub map_spawns_pit: Option<String>,
    pub map_spawns_odd: Option<String>,
    pub map_spawns_even: Option<String>,

    pub map_checkpoints: Option<Vec<String>>,

    pub server_physics: bool,
    pub max_cars: Option<u8>,
    pub max_laps: Option<usize>,
    pub qual_time: Option<usize>,
}

#[derive(Deserialize)]
pub struct EventSettings {
    pub expected_clients: Option<Vec<String>>,
}
