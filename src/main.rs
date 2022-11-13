#[macro_use] extern crate log;
use log::LevelFilter;

mod server;

pub static mut MAP_NAME: &'static str = "1988_adelaide";

#[tokio::main]
async fn main() {
    pretty_env_logger::formatted_timed_builder().filter_level(LevelFilter::max()).init();
    debug!("Hello, server!");

    let mut server = server::Server::new().await.map_err(|e| error!("{:?}", e)).expect("Failed to start server!");

    loop {
        server.process().await.map_err(|e| error!("{:?}", e)).expect("Failed to process events!");
    }
}

pub fn get_map_name() -> String {
    unsafe { MAP_NAME.to_string() }
}
