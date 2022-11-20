#[macro_use]
extern crate async_trait;
#[macro_use]
extern crate log;
use log::LevelFilter;

use std::sync::Arc;

use argh::FromArgs;

mod config;
mod server;
mod ui;

#[derive(FromArgs)]
/// cli options
struct Options {
    /// headless (no ui)
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
    pretty_env_logger::formatted_timed_builder()
        .filter_level(LevelFilter::max())
        .init();
    debug!("Hello, server!");

    let user_config: config::Config = toml::from_str(
        &std::fs::read_to_string("config.toml")
            .map_err(|_| error!("Failed to read config file!"))
            .expect("Failed to read config file!")
        )
        .map_err(|_| error!("Failed to parse config file!"))
        .expect("Failed to parse config file!");
    let user_config = Arc::new(user_config);
    let user_config_ref = Arc::clone(&user_config);

    let mut server = server::Server::new(user_config_ref)
        .await
        .map_err(|e| error!("{:?}", e))
        .expect("Failed to start server!");
    loop {
        if let Err(e) = server.process().await {
            error!("{:?}", e);
        }
    }
}

// TODO: Move to ui/mod.rs
#[tokio::main]
async fn main_ui() {
    tui_logger::init_logger(LevelFilter::max()).expect("Failed to initialize tui logger!");
    tui_logger::set_default_level(LevelFilter::max());
    debug!("Hello, server!");

    let (tx_event, rx_event) = tokio::sync::mpsc::channel::<ui::ServerEvent>(128);
    let (tx_cmd, rx_cmd) = tokio::sync::mpsc::channel::<ui::ServerCommand>(128);

    let user_config: config::Config = toml::from_str(
        &std::fs::read_to_string("config.toml")
            .map_err(|_| error!("Failed to read config file!"))
            .expect("Failed to read config file!"),
    )
    .map_err(|_| error!("Failed to parse config file!"))
    .expect("Failed to parse config file!");
    let user_config = Arc::new(user_config);
    let user_config_ref = Arc::clone(&user_config);

    let _ = std::thread::spawn(|| {
        ui::start(user_config, rx_event, tx_cmd).expect("Failed to run UI!");
    });

    let mut id_name_list: Vec<(u8, server::UserData)> = Vec::new();
    let mut server = server::Server::new(user_config_ref)
        .await
        .map_err(|e| error!("{:?}", e))
        .expect("Failed to start server!");
    loop {
        if let Err(e) = server.process().await {
            error!("{:?}", e);
        }

        // Check if new clients
        let update_clients = if id_name_list.len() != server.clients.len() {
            true
        } else {
            if are_clients_same(&id_name_list, &server.clients) {
                true
            } else {
                false
            }
        };

        if update_clients {
            id_name_list = Vec::new();
            for client in &server.clients {
                if let Some(info) = &client.info {
                    id_name_list.push((client.id, info.clone()));
                }
            }
            tx_event.try_send(ui::ServerEvent::ClientListUpdate(id_name_list.clone()));
        }
    }
}

fn are_clients_same(a: &Vec<(u8, server::UserData)>, b: &Vec<server::Client>) -> bool {
    let matching = a
        .iter()
        .zip(b.iter())
        .filter(|&(a, b)| Some(&a.1) == b.info.as_ref() && a.0 == b.id)
        .count();
    matching == a.len() && matching == b.len()
}
