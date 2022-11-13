use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;
use tokio::net::TcpListener;

mod client;
mod packet;
mod backend;

pub use client::*;
pub use packet::*;
pub use backend::*;

pub struct Server {
    listener: Arc<TcpListener>,
    clients_incoming: Arc<Mutex<Vec<Client>>>,
    clients: Vec<Client>,

    connect_runtime_handle: JoinHandle<()>,
}

impl Server {
    pub async fn new() -> anyhow::Result<Self> {
        let listener = Arc::new(TcpListener::bind("0.0.0.0:48900").await?);
        let listener_ref = Arc::clone(&listener);

        let clients_incoming = Arc::new(Mutex::new(Vec::new()));
        let clients_incoming_ref = Arc::clone(&clients_incoming);

        debug!("Client acception runtime starting...");
        let connect_runtime_handle = tokio::spawn(async move {
            loop {
                match listener_ref.accept().await {
                    Ok((socket, addr)) => {
                        info!("New client connected: {:?}", addr);

                        let mut client = Client::new(socket, addr.ip().to_string());
                        match client.authenticate().await {
                            Ok(_) => {
                                let mut lock = clients_incoming_ref.lock().map_err(|e| error!("{:?}", e)).expect("Failed to acquire lock on mutex!");
                                lock.push(client);
                                drop(lock);
                            },
                            Err(e) => {
                                error!("Authentication error occured, kicking player...");
                                error!("{:?}", e);
                                client.kick("Failed to authenticate player!").await;
                            }
                        }
                    },
                    Err(e) => error!("Failed to accept incoming connection: {:?}", e),
                }
            }
        });
        debug!("Client acception runtime started!");

        Ok(Self {
            listener: listener,
            clients_incoming: clients_incoming,
            clients: Vec::new(),

            connect_runtime_handle: connect_runtime_handle,
        })
    }

    pub async fn process(&mut self) -> anyhow::Result<()> {
        // Bit weird, but this is all to avoid deadlocking the server if anything goes wrong
        // with the client acception runtime. If that one locks, the server won't accept
        // more clients, but it will at least still process all other clients
        if let Ok(mut clients_incoming_lock) = self.clients_incoming.try_lock() {
            if clients_incoming_lock.len() > 0 {
                trace!("Accepting {} incoming clients...", clients_incoming_lock.len());
                for i in 0..clients_incoming_lock.len() {
                    self.clients.push(clients_incoming_lock.swap_remove(i));
                }
                trace!("Accepted incoming clients!");
            }
        }

        // Process all the clients
        for i in 0..self.clients.len() {
            self.clients[i].process().await?;
            if self.clients[i].state == ClientState::Disconnect {
                let id = self.clients[i].id;
                info!("Disconnecting client {}...", id);
                self.clients.remove(i);
                info!("Client {} disconnected!", id);
            }
        }
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Not sure how needed this is but it seems right?
        self.connect_runtime_handle.abort();
    }
}
