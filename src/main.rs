#[macro_use] extern crate log;
use log::LevelFilter;

mod server;
mod config;

// TODO: Doesn't seem like a very clean solution, even though we only have to set it once
//       at startup, based on the config
pub static mut MAP_NAME: String = String::new();
pub fn get_map_name() -> String {
    unsafe { MAP_NAME.to_string() }
}

#[tokio::main]
async fn main() {
    pretty_env_logger::formatted_timed_builder().filter_level(LevelFilter::max()).init();
    debug!("Hello, server!");

    let user_config: config::Config = toml::from_str(
        &std::fs::read_to_string("config.toml").map_err(|_| error!("Failed to read config file!")).expect("Failed to read config file!")
    ).map_err(|_| error!("Failed to parse config file!")).expect("Failed to parse config file!");
    unsafe { MAP_NAME = user_config.game.map.clone(); }

    let mut server = server::Server::new(user_config.network.port.unwrap_or(48900)).await.map_err(|e| error!("{:?}", e)).expect("Failed to start server!");

    loop {
        server.process().await.map_err(|e| error!("{:?}", e)).expect("Failed to process events!");
    }
}
