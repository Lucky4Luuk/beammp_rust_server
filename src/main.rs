#[macro_use] extern crate log;
use log::LevelFilter;

use std::sync::Arc;

use argh::FromArgs;

mod server;
mod config;
mod ui;

#[derive(FromArgs)]
/// cli options
struct Options {
    /// headless, no ui
    #[argh(switch, short = 'h')]
    headless: bool,
}

fn main() {
    let opts: Options = argh::from_env();
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create runtime!");
    if opts.headless {
        runtime.spawn_blocking(main_headless);
    } else {
        runtime.spawn_blocking(main_ui);
    }
}

#[tokio::main]
async fn main_headless() {
    pretty_env_logger::formatted_timed_builder().filter_level(LevelFilter::max()).init();
    debug!("Hello, server!");

    let user_config: config::Config = toml::from_str(
        &std::fs::read_to_string("config.toml").map_err(|_| error!("Failed to read config file!")).expect("Failed to read config file!")
    ).map_err(|_| error!("Failed to parse config file!")).expect("Failed to parse config file!");
    let user_config = Arc::new(user_config);
    let user_config_ref = Arc::clone(&user_config);

    let mut server = server::Server::new(user_config_ref).await.map_err(|e| error!("{:?}", e)).expect("Failed to start server!");
    loop {
        server.process().await.map_err(|e| error!("{:?}", e)).expect("Failed to process events!");
    }
}

#[tokio::main]
async fn main_ui() {
    // pretty_env_logger::formatted_timed_builder().filter_level(LevelFilter::max()).init();
    tui_logger::init_logger(LevelFilter::max()).expect("Failed to initialize tui logger!");
    tui_logger::set_default_level(LevelFilter::max());
    debug!("Hello, server!");

    let user_config: config::Config = toml::from_str(
        &std::fs::read_to_string("config.toml").map_err(|_| error!("Failed to read config file!")).expect("Failed to read config file!")
    ).map_err(|_| error!("Failed to parse config file!")).expect("Failed to parse config file!");
    let user_config = Arc::new(user_config);
    let user_config_ref = Arc::clone(&user_config);

    let _ = std::thread::spawn(|| {
        ui::start(user_config).expect("Failed to run UI!");
    });

    let mut server = server::Server::new(user_config_ref).await.map_err(|e| error!("{:?}", e)).expect("Failed to start server!");
    loop {
        server.process().await.map_err(|e| error!("{:?}", e)).expect("Failed to process events!");
    }

    // let server_handle = tokio::spawn(async move {
    //     let mut server = server::Server::new(user_config_ref).await.map_err(|e| error!("{:?}", e)).expect("Failed to start server!");
    //     loop {
    //         server.process().await.map_err(|e| error!("{:?}", e)).expect("Failed to process events!");
    //     }
    // });
    //
    // server_handle.await.expect("wah");

    // let handle = std::thread::spawn(move || {
    //     let mut runtime = tokio::runtime::Runtime::new().expect("Failed to create runtime!");
    //     runtime.spawn(async move {
    //         let mut server = server::Server::new(user_config_ref).await.map_err(|e| error!("{:?}", e)).expect("Failed to start server!");
    //         loop {
    //             server.process().await.map_err(|e| error!("{:?}", e)).expect("Failed to process events!");
    //         }
    //     });
    // });
    // while !handle.is_finished() {}
}
