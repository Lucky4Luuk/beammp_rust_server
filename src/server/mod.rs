use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::task::JoinHandle;
use tokio::net::{TcpListener, UdpSocket};

mod client;
mod packet;
mod backend;
mod car;

pub use client::*;
pub use packet::*;
pub use backend::*;
pub use car::*;

pub use crate::config::Config;

pub struct Server {
    tcp_listener: Arc<TcpListener>,
    udp_socket: Arc<UdpSocket>,

    clients_incoming: Arc<Mutex<Vec<Client>>>,
    clients: Vec<Client>,

    connect_runtime_handle: JoinHandle<()>,

    config: Arc<Config>,
}

impl Server {
    pub async fn new(config: Arc<Config>) -> anyhow::Result<Self> {
        let config_ref = Arc::clone(&config);

        let port = config.network.port.unwrap_or(48900);
        debug!("Server started on port {}", port);

        let bind_addr = &format!("0.0.0.0:{}", port);
        let tcp_listener = Arc::new(TcpListener::bind(bind_addr).await?);
        let tcp_listener_ref = Arc::clone(&tcp_listener);

        let udp_socket = Arc::new(UdpSocket::bind(bind_addr).await?);

        let clients_incoming = Arc::new(Mutex::new(Vec::new()));
        let clients_incoming_ref = Arc::clone(&clients_incoming);

        debug!("Client acception runtime starting...");
        let connect_runtime_handle = tokio::spawn(async move {
            loop {
                match tcp_listener_ref.accept().await {
                    Ok((socket, addr)) => {
                        info!("New client connected: {:?}", addr);

                        let mut client = Client::new(socket, addr.ip().to_string());
                        match client.authenticate(&config_ref).await {
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
            tcp_listener: tcp_listener,
            udp_socket: udp_socket,

            clients_incoming: clients_incoming,
            clients: Vec::new(),

            connect_runtime_handle: connect_runtime_handle,

            config: config,
        })
    }

    pub async fn process(&mut self) -> anyhow::Result<()> {
        // Bit weird, but this is all to avoid deadlocking the server if anything goes wrong
        // with the client acception runtime. If that one locks, the server won't accept
        // more clients, but it will at least still process all other clients
        let mut joined_names = Vec::new();
        if let Ok(mut clients_incoming_lock) = self.clients_incoming.try_lock() {
            if clients_incoming_lock.len() > 0 {
                trace!("Accepting {} incoming clients...", clients_incoming_lock.len());
                for i in 0..clients_incoming_lock.len() {
                    joined_names.push(clients_incoming_lock[i].info.as_ref().unwrap().username.clone());
                    self.clients.push(clients_incoming_lock.swap_remove(i));
                }
                trace!("Accepted incoming clients!");
            }
        }

        // self.broadcast(Packet::Notification(String::from("test"))).await;

        // Process UDP packets
        // TODO: Use a UDP addr -> client ID look up table
        for (addr, packet) in self.read_udp_packets().await {
            if packet.data.len() == 0 { continue; }
            let id = packet.data[0] - 1; // Offset by 1
            let data = packet.data[2..].to_vec();
            let packet_processed = RawPacket {
                header: data.len() as u32,
                data: data,
            };
            'search: for i in 0..self.clients.len() {
                if self.clients[i].id == id {
                    self.parse_packet_udp(i, addr, packet_processed).await?;
                    break 'search;
                }
            }
        }

        // Process all the clients (TCP)
        let mut packets: Vec<(usize, RawPacket)> = Vec::new();
        for i in 0..self.clients.len() {
            if let Some(client) = self.clients.get_mut(i) {
                if let Some(raw_packet) = client.process().await? {
                    packets.push((i, raw_packet.clone()));
                }

                // More efficient than broadcasting as we are already looping
                for name in joined_names.iter() {
                    self.clients[i].queue_packet(
                        Packet::Notification(NotificationPacket::new(format!("Welcome {}!", name.to_string())))
                    ).await;
                }

                if self.clients[i].state == ClientState::Disconnect {
                    let id = self.clients[i].id;
                    info!("Disconnecting client {}...", id);
                    self.clients.remove(i);
                    info!("Client {} disconnected!", id);
                }
            }
        }
        for (i, packet) in packets {
            self.parse_packet(i, packet).await?
        }
        Ok(())
    }

    async fn broadcast(&self, packet: Packet) {
        for client in &self.clients {
            client.queue_packet(packet.clone()).await;
        }
    }

    async fn send_udp(&self, udp_addr: SocketAddr, packet: Packet) {
        if let Err(e) = self.udp_socket.try_send_to(&packet.get_data(), udp_addr) {
            error!("UDP Packet send error: {:?}", e);
        }
    }

    async fn read_udp_packets(&self) -> Vec<(SocketAddr, RawPacket)> {
        let mut packets = Vec::new();
        'read: loop {
            let mut data = vec![0u8; 4096];
            let data_size;
            let data_addr;

            match self.udp_socket.try_recv_from(&mut data) {
                Ok((0, _)) => {
                    error!("UDP socket is readable, yet has 0 bytes to read!");
                    break 'read;
                },
                Ok((n, addr)) => (data_size, data_addr) = (n, addr),
                Err(_) => break 'read,
            }

            let packet = RawPacket {
                header: data_size as u32,
                data: data[..data_size].to_vec(),
            };
            debug!("udp packet: {:?}", packet);
            packets.push((data_addr, packet));
        }
        if packets.len() > 0 { trace!("UDP packets read: {}", packets.len()); }
        packets
    }

    async fn parse_packet_udp(&mut self, client_idx: usize, udp_addr: SocketAddr, mut packet: RawPacket) -> anyhow::Result<()> {
        if packet.data.len() > 0 {
            let client = &mut self.clients[client_idx];

            // Check if compressed
            let mut is_compressed = false;
            if packet.data.len() > 3 {
                let string_data = String::from_utf8_lossy(&packet.data[..4]);
                if string_data.starts_with("ABG:") {
                    is_compressed = true;
                    trace!("Packet is compressed!");
                }
            }

            if is_compressed {
                let compressed = &packet.data[4..];
                let mut decompressed: Vec<u8> = Vec::with_capacity(100_000);
                let mut decompressor = flate2::Decompress::new(true);
                decompressor.decompress_vec(compressed, &mut decompressed, flate2::FlushDecompress::None)?;
                packet.data = decompressed;
                // let string_data = String::from_utf8_lossy(&packet.data[..]);
                // debug!("Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`", string_data, packet.data, packet.header);
            }

            // Check packet identifier
            let packet_identifier = packet.data[0] as char;
            match packet_identifier {
                'p' => {
                    self.send_udp(udp_addr, Packet::Raw(RawPacket::from_code('p'))).await;
                },
                _ => {
                    let string_data = String::from_utf8_lossy(&packet.data[..]);
                    debug!("Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`", string_data, packet.data, packet.header);
                },
            }
        }
        Ok(())
    }

    async fn parse_packet(&mut self, client_idx: usize, mut packet: RawPacket) -> anyhow::Result<()> {
        if packet.data.len() > 0 {
            let client = &mut self.clients[client_idx];

            // Check if compressed
            let mut is_compressed = false;
            if packet.data.len() > 3 {
                let string_data = String::from_utf8_lossy(&packet.data[..4]);
                if string_data.starts_with("ABG:") {
                    is_compressed = true;
                    trace!("Packet is compressed!");
                }
            }

            if is_compressed {
                let compressed = &packet.data[4..];
                let mut decompressed: Vec<u8> = Vec::with_capacity(100_000);
                let mut decompressor = flate2::Decompress::new(true);
                decompressor.decompress_vec(compressed, &mut decompressed, flate2::FlushDecompress::None)?;
                packet.data = decompressed;
                // let string_data = String::from_utf8_lossy(&packet.data[..]);
                // debug!("Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`", string_data, packet.data, packet.header);
            }

            // Check packet identifier
            let packet_identifier = packet.data[0] as char;
            match packet_identifier {
                'H' => {
                    // Full sync with server
                    client.queue_packet(Packet::Raw(RawPacket::from_str(&format!("Sn{}", client.info.as_ref().unwrap().username.clone())))).await;
                    // TODO: Send vehicle data
                },
                'O' => self.parse_vehicle_packet(client_idx, packet).await?,
                _ => {
                    let string_data = String::from_utf8_lossy(&packet.data[..]);
                    debug!("Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`", string_data, packet.data, packet.header);
                },
            }
        }
        Ok(())
    }

    async fn parse_vehicle_packet(&mut self, client_idx: usize, packet: RawPacket) -> anyhow::Result<()> {
        if packet.data.len() < 6 {
            error!("Vehicle packet too small!");
            return Ok(()); // TODO: Return error here
        }
        let code = packet.data[1] as char;
        match code {
            's' => {
                let client = &mut self.clients[client_idx];
                let car_json_str = String::from_utf8_lossy(&packet.data[6..]);
                // let car_json: serde_json::Value = serde_json::from_str(&car_json_str)?;
                let car_id = client.register_car(Car::new(car_json_str.to_string()));
                let client_id = client.get_id();
                let packet_data = format!("Os:{}:{}:{}-{}:{}", client.get_roles(), client.get_name(), client_id, car_id, car_json_str);
                let response = RawPacket::from_str(&packet_data);
                self.broadcast(Packet::Notification(NotificationPacket::new(format!("Client {} spawned a car (#{})!", client_id, car_id)))).await;
                self.broadcast(Packet::Raw(response)).await;
            },
            _ => error!("Unknown vehicle related packet!"), // TODO: Return error here
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
