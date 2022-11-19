use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub network: NetworkSettings,
    pub game: GameSettings,
}

#[derive(Deserialize)]
pub struct NetworkSettings {
    pub port: Option<u16>,
}

#[derive(Deserialize)]
pub struct GameSettings {
    pub map: String,
    pub server_physics: bool,
}
