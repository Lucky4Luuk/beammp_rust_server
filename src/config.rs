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
}

#[derive(Deserialize)]
pub struct GameSettings {
    pub map: String,
    pub map_limits: Option<String>,
    pub map_limits_pit: Option<String>,
    pub map_limits_pit_exit: Option<String>,
    pub map_finish: Option<String>,
    pub map_spawns_pit: Option<String>,
    pub map_path: Option<String>,
    pub server_physics: bool,
    pub max_cars: Option<u8>,
}

#[derive(Deserialize)]
pub struct EventSettings {
    pub expected_clients: Option<Vec<String>>,
}
