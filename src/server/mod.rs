use std::net::SocketAddr;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::Instant;

use tokio::net::{TcpListener, UdpSocket};
use tokio::task::JoinHandle;

use nalgebra::*;

mod backend;
mod car;
mod client;
mod packet;
mod track_limits;
mod spawns;
mod track_path;
mod overlay;

pub use backend::*;
pub use car::*;
pub use client::*;
pub use packet::*;
pub use track_limits::*;
pub use spawns::*;
pub use track_path::*;
pub use overlay::*;

pub use crate::config::Config;

#[derive(PartialEq)]
enum ServerState {
    WaitingForClients,
    WaitingForReady,
    WaitingForSpawns,
    Qualifying,
    LiningUp,
    Countdown,
    Race,
    Finish,
}

pub struct Server {
    tcp_listener: Arc<TcpListener>,
    tcp_listener_overlay: Arc<TcpListener>,
    udp_socket: Arc<UdpSocket>,

    clients_incoming: Arc<Mutex<Vec<Client>>>,
    overlay_incoming: Arc<Mutex<Vec<(String, Overlay)>>>,

    pub clients: Vec<Client>,
    unconnected_overlays: Vec<(String, Overlay)>,

    connect_runtime_handle: JoinHandle<()>,
    connect_overlay_runtime_handle: JoinHandle<()>,

    config: Arc<Config>,

    track_limits: Option<TrackLimits>,
    track_limits_pit: Option<TrackLimits>,
    track_limits_pit_exit: Option<TrackLimits>,
    track_limits_client: u8, // The client to check this loop, also serves as a timer for checking

    track_spawns_pit: Option<Spawns>,
    track_spawns_odd: Option<Spawns>,
    track_spawns_even: Option<Spawns>,

    track_checkpoints: Vec<TrackPath>,

    server_state: ServerState,
    server_state_start: Instant,

    allow_spawns: bool,
    force_respawn_pits: bool,
    allow_respawns: bool,

    overlay_update_time: Instant,
    generic_timer0: Instant,
}

impl Server {
    pub async fn new(config: Arc<Config>) -> anyhow::Result<Self> {
        let config_ref = Arc::clone(&config);

        let port = config.network.port.unwrap_or(48900);
        let overlay_port = config.network.overlay_port.unwrap_or(48901);
        debug!("Server started on port {} / {}", port, overlay_port);

        let tcp_listener = {
            let bind_addr = &format!("0.0.0.0:{}", port);
            Arc::new(TcpListener::bind(bind_addr).await?)
        };
        let tcp_listener_ref = Arc::clone(&tcp_listener);

        let tcp_listener_overlay = {
            let bind_addr = &format!("0.0.0.0:{}", overlay_port);
            Arc::new(TcpListener::bind(bind_addr).await?)
        };
        let tcp_listener_overlay_ref = Arc::clone(&tcp_listener_overlay);

        let udp_socket = {
            let bind_addr = &format!("0.0.0.0:{}", port);
            Arc::new(UdpSocket::bind(bind_addr).await?)
        };

        let clients_incoming = Arc::new(Mutex::new(Vec::new()));
        let clients_incoming_ref = Arc::clone(&clients_incoming);
        debug!("Client acception runtime starting...");
        let connect_runtime_handle = tokio::spawn(async move {
            loop {
                match tcp_listener_ref.accept().await {
                    Ok((socket, addr)) => {
                        info!("New client connected: {:?}", addr);

                        let mut client = Client::new(socket);
                        match client.authenticate(&config_ref).await {
                            Ok(_) => {
                                let mut lock = clients_incoming_ref
                                    .lock()
                                    .map_err(|e| error!("{:?}", e))
                                    .expect("Failed to acquire lock on mutex!");
                                lock.push(client);
                                drop(lock);
                            }
                            Err(e) => {
                                error!("Authentication error occured, kicking player...");
                                error!("{:?}", e);
                                client.kick("Failed to authenticate player!").await;
                            }
                        }
                    }
                    Err(e) => error!("Failed to accept incoming connection: {:?}", e),
                }
            }
        });
        debug!("Client acception runtime started!");

        let overlay_incoming = Arc::new(Mutex::new(Vec::new()));
        let overlay_incoming_ref = Arc::clone(&overlay_incoming);
        debug!("Overlay acception runtime starting...");
        let connect_overlay_runtime_handle = tokio::spawn(async move {
            loop {
                match tcp_listener_overlay_ref.accept().await {
                    Ok((socket, addr)) => {
                        info!("New overlay connected: {:?}", addr);

                        match Overlay::new(socket).await {
                            Ok(overlay) => {
                                let mut lock = overlay_incoming_ref
                                    .lock()
                                    .map_err(|e| error!("{:?}", e))
                                    .expect("Failed to acquire lock on mutex!");
                                lock.push(overlay);
                                drop(lock);
                            }
                            Err(e) => {
                                error!("Overlay connection error occurred...");
                                error!("{:?}", e);
                            }
                        }
                    }
                    Err(e) => error!("Failed to accept incoming connection: {:?}", e),
                }
            }
        });
        debug!("Overlay acception runtime started!");

        let track_limits = if let Some(limits_file) = &config.game.map_limits {
            Some(serde_json::from_str(&std::fs::read_to_string(limits_file)?)?)
        } else {
            None
        };

        let track_limits_pit = if let Some(limits_file) = &config.game.map_limits_pit {
            Some(serde_json::from_str(&std::fs::read_to_string(limits_file)?)?)
        } else {
            None
        };

        let track_limits_pit_exit = if let Some(limits_file) = &config.game.map_limits_pit_exit {
            Some(serde_json::from_str(&std::fs::read_to_string(limits_file)?)?)
        } else {
            None
        };

        let track_spawns_pit = if let Some(spawns_file) = &config.game.map_spawns_pit {
            Some(serde_json::from_str(&std::fs::read_to_string(spawns_file)?)?)
        } else {
            None
        };

        let track_spawns_odd = if let Some(spawns_file) = &config.game.map_spawns_odd {
            Some(serde_json::from_str(&std::fs::read_to_string(spawns_file)?)?)
        } else {
            None
        };

        let track_spawns_even = if let Some(spawns_file) = &config.game.map_spawns_even {
            Some(serde_json::from_str(&std::fs::read_to_string(spawns_file)?)?)
        } else {
            None
        };

        let track_checkpoints = if let Some(cp_list) = &config.game.map_checkpoints {
            // Some(cp_list.iter().map(|file| serde_json::from_str(&std::fs::read_to_string(path_file)?)?).collect())
            let mut list = Vec::new();
            for file in cp_list {
                list.push(serde_json::from_str(&std::fs::read_to_string(file)?)?);
            }
            list
        } else {
            Vec::new()
        };

        Ok(Self {
            tcp_listener: tcp_listener,
            tcp_listener_overlay: tcp_listener_overlay,
            udp_socket: udp_socket,

            clients_incoming: clients_incoming,
            overlay_incoming: overlay_incoming,

            clients: Vec::new(),
            unconnected_overlays: Vec::new(),

            connect_runtime_handle: connect_runtime_handle,
            connect_overlay_runtime_handle: connect_overlay_runtime_handle,

            config: config,

            track_limits: track_limits,
            track_limits_pit: track_limits_pit,
            track_limits_pit_exit: track_limits_pit_exit,
            track_limits_client: 0,

            track_spawns_pit: track_spawns_pit,
            track_spawns_odd: track_spawns_odd,
            track_spawns_even: track_spawns_even,

            track_checkpoints: track_checkpoints,

            server_state: ServerState::WaitingForClients,
            server_state_start: Instant::now(),

            allow_spawns: false,
            force_respawn_pits: false,
            allow_respawns: true,

            overlay_update_time: Instant::now(),
            generic_timer0: Instant::now(),
        })
    }

    pub fn set_server_state(&mut self, state: ServerState) {
        self.server_state = state;
        self.server_state_start = Instant::now();
    }

    pub async fn process(&mut self) -> anyhow::Result<()> {
        // Bit weird, but this is all to avoid deadlocking the server if anything goes wrong
        // with the client acception runtime. If that one locks, the server won't accept
        // more clients, but it will at least still process all other clients
        let mut joined_names = Vec::new();
        if let Ok(mut clients_incoming_lock) = self.clients_incoming.try_lock() {
            if clients_incoming_lock.len() > 0 {
                trace!(
                    "Accepting {} incoming clients...",
                    clients_incoming_lock.len()
                );
                for i in 0..clients_incoming_lock.len() {
                    joined_names.push(
                        clients_incoming_lock[i]
                            .info
                            .as_ref()
                            .unwrap()
                            .username
                            .clone(),
                    );
                    self.clients.push(clients_incoming_lock.swap_remove(i));
                }
                trace!("Accepted incoming clients!");
            }
        }

        // Same thing here, but instead accepting overlay connections
        if let Ok(mut overlay_incoming_lock) = self.overlay_incoming.try_lock() {
            if overlay_incoming_lock.len() > 0 {
                trace!(
                    "Accepting {} incoming overlay connections...",
                    overlay_incoming_lock.len()
                );
                for i in 0..overlay_incoming_lock.len() {
                    self.unconnected_overlays.push(overlay_incoming_lock.swap_remove(i));
                }
                trace!("Accepted incoming overlay connections!");
            }
        }
        if self.unconnected_overlays.len() > 0 {
            for j in 0..self.clients.len() {
                if self.clients.get(j).ok_or(ServerError::ClientDoesntExist)?.info.as_ref().unwrap().username == self.unconnected_overlays[0].0 {
                    self.clients[j].overlay = Some(self.unconnected_overlays.swap_remove(0).1);
                }
            }
        }

        // Process UDP packets
        // TODO: Use a UDP addr -> client ID look up table
        for (addr, packet) in self.read_udp_packets().await {
            if packet.data.len() == 0 {
                continue;
            }
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
                match client.process().await {
                    Ok(packet_opt) => {
                        if let Some(raw_packet) = packet_opt {
                            packets.push((i, raw_packet.clone()));
                        }
                    }
                    Err(e) => client.kick(&format!("Kicked: {:?}", e)).await,
                }

                // More efficient than broadcasting as we are already looping
                for name in joined_names.iter() {
                    self.clients[i]
                        .queue_packet(Packet::Notification(NotificationPacket::new(format!(
                            "Welcome {}!",
                            name.to_string()
                        ))))
                        .await;
                }
            }
        }
        for (i, packet) in packets {
            self.parse_packet(i, packet).await?
        }

        // I'm sorry for this code :(
        for i in 0..self.clients.len() {
            if self.clients.get(i).ok_or(ServerError::ClientDoesntExist)?.state == ClientState::Disconnect {
                let id = self.clients.get(i).ok_or(ServerError::ClientDoesntExist)?.id;
                for j in 0..self.clients.get(i).ok_or(ServerError::ClientDoesntExist)?.cars.len() {
                    let car_id = self.clients.get(i).ok_or(ServerError::ClientDoesntExist)?.cars[j].0;
                    let delete_packet = format!("Od:{}-{}", id, car_id);
                    self.broadcast(Packet::Raw(RawPacket::from_str(&delete_packet)), None)
                        .await;
                }
                info!("Disconnecting client {}...", id);
                self.clients.remove(i);
                info!("Client {} disconnected!", id);
            }
        }

        // Overlay updating
        for client in &mut self.clients {
            client.update_overlay().await;

            if self.overlay_update_time.elapsed().as_secs() > 1 {
                let max_laps = if self.server_state == ServerState::Race { self.config.game.max_laps.unwrap_or(0) } else { 0 };
                if let Some(overlay) = &mut client.overlay {
                    overlay.set_max_laps(max_laps).await;
                    overlay.set_state(&self.server_state).await;
                }
                self.overlay_update_time = Instant::now();
            }
        }

        // Physics
        if self.config.game.server_physics {
            todo!("Not yet implemented! Can't correct the players position and velocity without respawning right now, so this will have to wait for that!");
        }

        if self.server_state == ServerState::Qualifying || self.server_state == ServerState::Race {
            // Track limits
            if let Some(client) = &mut self.clients.get_mut(self.track_limits_client as usize) {
                for (_, car) in &mut client.cars {
                    if let Some(limits) = &self.track_limits {
                        if limits.check_limits([car.pos.x as f32, car.pos.y as f32], car.hitbox_half) {
                            if let Some(start) = car.offtrack_start {
                                debug!("Client went {} seconds offtrack!", start.elapsed().as_secs_f32());
                            }
                            car.offtrack_start = None;
                        } else {
                            if car.offtrack_start.is_none() {
                                car.offtrack_start = Some(Instant::now());
                            }
                        }
                    }

                    if let Some(limits) = &self.track_limits_pit {
                        limits.check_limits([car.pos.x as f32, car.pos.y as f32], car.hitbox_half);
                    }

                    // if let Some(limits) = &self.track_limits_pit_exit {
                    //     limits.check_limits([car.pos.x as f32, car.pos.y as f32], [1.0, 1.0]);
                    // }
                }
            }

            // Track path
            if let Some(client) = &mut self.clients.get_mut(self.track_limits_client as usize) {
                for (_, car) in &mut client.cars {
                    let active_cp = if car.next_checkpoint == 0 {
                        self.track_checkpoints.len() - 1
                    } else {
                        car.next_checkpoint - 1
                    };
                    if let Some(path) = &self.track_checkpoints.get(active_cp) {
                        let unit_quat = nalgebra::geometry::UnitQuaternion::from_quaternion(car.rot);
                        let car_angle = unit_quat.euler_angles().2 / std::f64::consts::PI * 180.0;
                        let car_forward = car.vel.xy().normalize();
                        let car_vel_angle = -(car_forward.y.atan2(car_forward.x) as f32 / std::f32::consts::PI * 180.0) + 90.0;
                        let track_angle = -path.get_angle_at_pos([car.pos.x as f32, car.pos.y as f32]) + 90.0;
                        let angle_diff = (car_angle as f32 - track_angle).abs() % 360.0;
                        car.latest_angle_to_track = angle_diff;
                        let angle_vel_diff = (180.0 - ((car_vel_angle as f32 - track_angle).abs() % 360.0)).max(0.0);
                        car.latest_vel_angle_to_track = angle_vel_diff;
                        // debug!("angle diff: {}", angle_diff);
                        // debug!("angle vel diff: {}", angle_vel_diff);
                        let progress = path.get_percentage_along_track([car.pos.x as f32, car.pos.y as f32]);
                        // debug!("progress: {}", progress);
                    }
                }
            }

            // Checkpoints
            for client in &mut self.clients {
                for (_, car) in &mut client.cars {
                    if let Some(cp) = self.track_checkpoints.get(car.next_checkpoint) {
                        if cp.check_limits([car.pos.x as f32, car.pos.y as f32], car.hitbox_half) {
                            if !car.intersects_cp {
                                if car.next_checkpoint == 0 {
                                    // Start/finish checkpoint
                                    if car.latest_vel_angle_to_track > 135.0 {
                                        car.laps -= 1;
                                        car.lap_start = None;
                                    } else {
                                        if let Some(last) = car.lap_start {
                                            car.add_lap_time(last.elapsed());
                                        }
                                        car.laps += 1;
                                        car.laps_ui_dirty = true;
                                        car.lap_start = Some(Instant::now());
                                        car.next_checkpoint = 1;
                                    }
                                } else {
                                    // Not the last checkpoint
                                    car.next_checkpoint += 1;
                                    if car.next_checkpoint == self.track_checkpoints.len() {
                                        car.next_checkpoint = 0;
                                    }
                                }
                            }
                            // debug!("checkpoint: {}", car.next_checkpoint);
                            // debug!("lap: {}", car.laps);
                            car.intersects_cp = true;
                        } else {
                            car.intersects_cp = false;
                        }
                    }
                }
            }
        }

        // Send position packets
        let _ = self.udp_socket.writable().await;
        for i in 0..self.clients.len() {
            for k in 0..self.clients[i].cars.len() {
                if self.clients[i].cars[k].1.needs_packet {
                    self.clients[i].cars[k].1.needs_packet = false;
                    let pos_data = TransformPacket {
                        rvel: self.clients[i].cars[k].1.rvel.into(),
                        tim: self.clients[i].cars[k].1.tim,
                        pos: self.clients[i].cars[k].1.pos.into(),
                        ping: self.clients[i].cars[k].1.ping,
                        rot: self.clients[i].cars[k].1.rot.coords.into(),
                        vel: self.clients[i].cars[k].1.vel.into(),
                    };
                    if let Ok(json) = serde_json::to_string(&pos_data) {
                        let data = format!("Zp:{}-{}:{}", self.clients[i].id, self.clients[i].cars[k].0, json);
                        if self.clients[i].cars[k].1.is_corrected {
                            todo!("Not yet implemented! Can't correct the players position and velocity without respawning right now, so this will have to wait for that!");
                            self.clients[i].cars[k].1.is_corrected = false;
                            if let Some(udp_addr) = self.clients[i].udp_addr {
                                // TODO: This breaks all force feedback for a car
                                self.send_udp(udp_addr, &Packet::Raw(RawPacket::from_str(&format!("Zp:{}-{}:{}", self.clients[i].id, self.clients[i].cars[k].0, json)))).await;
                            }
                        }
                        let p = Packet::Raw(RawPacket::from_str(&data));
                        self.broadcast(p, Some(self.clients[i].id)).await;
                    }
                }
            }
        }

        // Handle server states
        let elapsed = self.server_state_start.elapsed();
        match self.server_state {
            ServerState::WaitingForClients => {
                if self.config.event.expected_clients.is_some() && elapsed.as_secs() < 150 {
                    let required_clients = self.config.event.expected_clients.as_ref().unwrap();
                    let mut joined_clients = required_clients.clone();
                    joined_clients.retain(|name| {
                        for client in &self.clients {
                            if client.info.as_ref().unwrap().username.trim() == name.trim() {
                                return false;
                            }
                        }
                        true
                    });
                    let mut kick = Vec::new();
                    for (i, client) in self.clients.iter().enumerate() {
                        let mut allowed = false;
                        'search: for name in required_clients {
                            if client.info.as_ref().unwrap().username.trim() == name.trim() {
                                allowed = true;
                                break 'search;
                            }
                        }
                        if !allowed {
                            kick.push(i);
                            debug!("Kicking client! They are not allowed into the server.");
                        }
                    }
                    for i in kick {
                        self.clients[i].kick("Not whitelisted for this server!").await;
                    }
                    if joined_clients.len() == 0 {
                        // All expected clients are in the server
                        self.connect_runtime_handle.abort();
                        info!("All clients connected!");
                        self.set_server_state(ServerState::WaitingForReady);
                    }
                } else {
                    self.connect_runtime_handle.abort();
                    info!("Clients no longer allowed to join!");
                    self.set_server_state(ServerState::WaitingForReady);
                }
            }
            ServerState::WaitingForReady => {
                let mut all_ready = true;
                for client in &self.clients {
                    if !client.ready {
                        all_ready = false;
                    }
                }
                if all_ready {
                    self.allow_spawns = true;
                    self.set_server_state(ServerState::WaitingForSpawns);
                }
            }
            ServerState::WaitingForSpawns => {
                let mut has_spawned = 0;
                for client in &self.clients {
                    if client.cars.len() > 0 {
                        has_spawned += 1;
                    }
                }
                if has_spawned == self.clients.len() {
                    info!("All clients have spawned a car!");
                    self.set_server_state(ServerState::Qualifying);
                    self.allow_spawns = false;
                    self.force_respawn_pits = true;
                    let mut i = 0;
                    for client in &self.clients {
                        for (id, car) in &client.cars {
                            let spawn = self.track_spawns_pit.as_ref().expect("Map did not have pit lane spawns set up!").get_client_spawn(i);
                            if let Ok(json) = serde_json::to_string(&RespawnPacketData {
                                pos: RespawnPacketDataPos {
                                    x: spawn.pos[0],
                                    y: spawn.pos[1],
                                    z: spawn.pos[2],
                                },
                                rot: RespawnPacketDataRot {
                                    x: spawn.rot[0],
                                    y: spawn.rot[1],
                                    z: spawn.rot[2],
                                    w: spawn.rot[3]
                                },
                            }) {
                                let packet_data = format!("Or:{}-{}:{}", client.id, id, json);
                                self.broadcast(Packet::Raw(RawPacket::from_str(&packet_data)), None).await;
                            }
                            i += 1;
                        }
                    }
                }
            }
            ServerState::Qualifying => {
                if self.server_state_start.elapsed().as_secs() > self.config.game.qual_time.unwrap_or(120) as u64 {
                    // Qualifying is over!
                    debug!("Qualifying is over!");
                    self.allow_respawns = false;

                    // Gather fastest times
                    let mut lap_id = Vec::new();
                    for (i, client) in self.clients.iter().enumerate() {
                        let mut fastest_lap = u128::MAX;
                        for (id, car) in &client.cars {
                            for lap in &car.lap_times {
                                if lap.as_millis() < fastest_lap {
                                    fastest_lap = lap.as_millis();
                                }
                            }
                        }
                        lap_id.push((i, fastest_lap));
                    }

                    // TODO: Store client ids in grid order

                    self.set_server_state(ServerState::LiningUp);
                    self.allow_respawns = false;
                    self.allow_spawns = false;

                    for client in &mut self.clients {
                        client.ready = false; // Require them to go ready again!
                    }
                }
            }
            ServerState::LiningUp => {
                if self.generic_timer0.elapsed().as_secs() > 0 {
                    self.generic_timer0 = Instant::now();
                    let mut all_ready = true;
                    for (i, client) in self.clients.iter().enumerate() {
                        if !client.ready {
                            all_ready = false;
                        }

                        // Check client delta to their grid spot
                        // TODO: Get grid spot based on grid order
                        let grid_spot;
                        if i % 2 == 0 {
                            // Even
                            let grid_spot_id = i / 2;
                            grid_spot = self.track_spawns_even.as_ref().expect("Map does not have spawns set up!").get_client_spawn(grid_spot_id as u8);
                        } else if i % 2 == 1 {
                            // Odd
                            let grid_spot_id = i / 2 -1;
                            grid_spot = self.track_spawns_odd.as_ref().expect("Map does not have spawns set up!").get_client_spawn(grid_spot_id as u8);
                        } else {
                            unreachable!();
                        }
                        if let Some((id, car)) = client.cars.get(0) {
                            let delta = crate::util::distance3d(grid_spot.pos, [car.pos.x, car.pos.y, car.pos.z]);
                            if delta > 1.0 {
                                if let Ok(json) = serde_json::to_string(&RespawnPacketData {
                                    pos: RespawnPacketDataPos {
                                        x: grid_spot.pos[0],
                                        y: grid_spot.pos[1],
                                        z: grid_spot.pos[2],
                                    },
                                    rot: RespawnPacketDataRot {
                                        x: grid_spot.rot[0],
                                        y: grid_spot.rot[1],
                                        z: grid_spot.rot[2],
                                        w: grid_spot.rot[3]
                                    },
                                }) {
                                    let packet_data = format!("Or:{}-{}:{}", client.id, id, json);
                                    self.broadcast(Packet::Raw(RawPacket::from_str(&packet_data)), None).await;
                                }
                            }
                        }
                    }
                    if all_ready {
                        self.set_server_state(ServerState::Countdown);
                        // TODO: Update the overlay immediately, to prevent countdown desync
                    }
                }
            }
            ServerState::Countdown => {

            }
            ServerState::Race => {

            }
            _ => todo!()
        }

        self.track_limits_client = self.track_limits_client.wrapping_add(1);

        Ok(())
    }

    async fn broadcast(&self, packet: Packet, owner: Option<u8>) {
        for client in &self.clients {
            if let Some(id) = owner {
                if id == client.id {
                    continue;
                }
            }
            client.queue_packet(packet.clone()).await;
        }
    }

    async fn broadcast_udp(&self, packet: Packet, owner: Option<u8>) {
        for client in &self.clients {
            if let Some(id) = owner {
                if id == client.id {
                    continue;
                }
            }
            // client.queue_packet(packet.clone()).await;
            if let Some(udp_addr) = client.udp_addr {
                self.send_udp(udp_addr, &packet).await;
            }
        }
    }

    async fn send_udp(&self, udp_addr: SocketAddr, packet: &Packet) {
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
                }
                Ok((n, addr)) => (data_size, data_addr) = (n, addr),
                Err(_) => break 'read,
            }

            let packet = RawPacket {
                header: data_size as u32,
                data: data[..data_size].to_vec(),
            };
            packets.push((data_addr, packet));
        }
        packets
    }

    async fn parse_packet_udp(
        &mut self,
        client_idx: usize,
        udp_addr: SocketAddr,
        mut packet: RawPacket,
    ) -> anyhow::Result<()> {
        if packet.data.len() > 0 {
            let client = &mut self.clients[client_idx];
            let client_id = client.get_id();

            client.udp_addr = Some(udp_addr);

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
                decompressor.decompress_vec(
                    compressed,
                    &mut decompressed,
                    flate2::FlushDecompress::Finish,
                )?;
                packet.header = decompressed.len() as u32;
                packet.data = decompressed;
                // let string_data = String::from_utf8_lossy(&packet.data[..]);
                // debug!("Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`", string_data, packet.data, packet.header);
            }

            // Check packet identifier
            let packet_identifier = packet.data[0] as char;
            if packet.data[0] >= 86 && packet.data[0] <= 89 {
                self.broadcast_udp(Packet::Raw(packet), Some(client_id))
                    .await;
            } else {
                match packet_identifier {
                    'p' => {
                        self.send_udp(udp_addr, &Packet::Raw(RawPacket::from_code('p')))
                            .await;
                    }
                    'Z' => {
                        if packet.data.len() < 7 {
                            error!("Position packet too small!");
                            return Err(ServerError::BrokenPacket.into());
                        } else {
                            // Sent as text so removing 48 brings it from [48-57] to [0-9]
                            let client_id = packet.data[3] - 48;
                            let car_id = packet.data[5] - 48;

                            let pos_json = &packet.data[7..];
                            let pos_data: TransformPacket =
                                serde_json::from_str(&String::from_utf8_lossy(pos_json))?;

                            let p = Packet::Raw(packet);

                            for i in 0..self.clients.len() {
                                if self.clients[i].id == client_id {
                                    let client = &mut self.clients[i];
                                    let car = client
                                        .get_car_mut(car_id)
                                        .ok_or(ServerError::CarDoesntExist)?;
                                    car.pos = pos_data.pos.into();
                                    car.rot = Quaternion::new(
                                        pos_data.rot[3],
                                        pos_data.rot[0],
                                        pos_data.rot[1],
                                        pos_data.rot[2],
                                    );
                                    car.vel = pos_data.vel.into();
                                    car.rvel = pos_data.rvel.into();
                                    car.tim = pos_data.tim;
                                    car.ping = pos_data.ping;
                                    car.needs_packet = true;
                                }
                            }
                        }
                    }
                    _ => {
                        let string_data = String::from_utf8_lossy(&packet.data[..]);
                        debug!(
                            "Unknown packet UDP - String data: `{}`; Array: `{:?}`; Header: `{:?}`",
                            string_data, packet.data, packet.header
                        );
                    }
                }
            }
        }
        Ok(())
    }

    async fn parse_packet(
        &mut self,
        client_idx: usize,
        mut packet: RawPacket,
    ) -> anyhow::Result<()> {
        if packet.data.len() > 0 {
            let client_id = {
                let client = &mut self.clients[client_idx];
                client.get_id()
            };

            // Check if compressed
            let mut is_compressed = false;
            if packet.data.len() > 3 {
                let string_data = String::from_utf8_lossy(&packet.data[..4]);
                if string_data.starts_with("ABG:") {
                    is_compressed = true;
                    // trace!("Packet is compressed!");
                }
            }

            if is_compressed {
                let compressed = &packet.data[4..];
                let mut decompressed: Vec<u8> = Vec::with_capacity(100_000);
                let mut decompressor = flate2::Decompress::new(true);
                decompressor.decompress_vec(
                    compressed,
                    &mut decompressed,
                    flate2::FlushDecompress::Finish,
                )?;
                packet.header = decompressed.len() as u32;
                packet.data = decompressed;
                // let string_data = String::from_utf8_lossy(&packet.data[..]);
                // debug!("Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`", string_data, packet.data, packet.header);
            }

            // Check packet identifier
            if packet.data[0] >= 86 && packet.data[0] <= 89 {
                self.broadcast(Packet::Raw(packet), Some(client_id)).await;
            } else {
                let packet_identifier = packet.data[0] as char;
                match packet_identifier {
                    'H' => {
                        // Full sync with server
                        self.clients[client_idx]
                            .queue_packet(Packet::Raw(RawPacket::from_str(&format!(
                                "Sn{}",
                                self.clients[client_idx]
                                    .info
                                    .as_ref()
                                    .unwrap()
                                    .username
                                    .clone()
                            ))))
                            .await;
                        // TODO: Sync all existing cars on server
                        //       Could be very annoying if you join an in-progress practice server
                    }
                    'O' => self.parse_vehicle_packet(client_idx, packet).await?,
                    'C' => {
                        // TODO: Chat filtering?
                        let packet_data = packet.data_as_string();
                        let message = packet_data.split(":").collect::<Vec<&str>>().get(2).map(|s| s.to_string()).unwrap_or(String::new());
                        let message = message.trim();
                        if message.starts_with("!") {
                            if message == "!ready" {
                                self.clients[client_idx].ready = true;
                                self.clients[client_idx].queue_packet(Packet::Raw(RawPacket::from_str("C:Server:You are now ready!"))).await;
                            } else if message == "!pos" {
                                let car = &self.clients[client_idx].cars.get(0).ok_or(ServerError::CarDoesntExist)?.1;
                                trace!("car transform (pos/rot/vel/rvel): {:?}", (car.pos, car.rot, car.vel, car.rvel));
                            } else {
                                self.clients[client_idx].queue_packet(Packet::Raw(RawPacket::from_str("C:Server:Unknown command!"))).await;
                            }
                        } else {
                            self.broadcast(Packet::Raw(packet), None).await;
                        }
                    }
                    _ => {
                        let string_data = String::from_utf8_lossy(&packet.data[..]);
                        debug!(
                            "Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`",
                            string_data, packet.data, packet.header
                        );
                    }
                }
            }
        }
        Ok(())
    }

    async fn parse_vehicle_packet(
        &mut self,
        client_idx: usize,
        packet: RawPacket,
    ) -> anyhow::Result<()> {
        if packet.data.len() < 6 {
            error!("Vehicle packet too small!");
            return Ok(()); // TODO: Return error here
        }
        let code = packet.data[1] as char;
        match code {
            's' => {
                let client = &mut self.clients[client_idx];
                let mut allowed = self.allow_spawns;
                if let Some(max_cars) = self.config.game.max_cars {
                    if client.cars.len() >= max_cars as usize { allowed = false; }
                }
                // trace!("Packet string: `{}`", packet.data_as_string());
                let split_data = packet
                    .data_as_string()
                    .splitn(3, ':')
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>();
                let car_json_str = &split_data.get(2).ok_or(std::fmt::Error)?;
                // let car_json: serde_json::Value = serde_json::from_str(&car_json_str)?;
                let car_id = client.register_car(Car::new(car_json_str.to_string()));
                let client_id = client.get_id();
                if allowed {
                    let packet_data = format!(
                        "Os:{}:{}:{}-{}:{}",
                        client.get_roles(),
                        client.get_name(),
                        client_id,
                        car_id,
                        car_json_str
                    );
                    let response = RawPacket::from_str(&packet_data);
                    self.broadcast(
                        Packet::Notification(NotificationPacket::new(format!(
                            "Client {} spawned a car (#{})!",
                            client_id, car_id
                        ))),
                        None,
                    )
                    .await;
                    self.broadcast(Packet::Raw(response), None).await;
                    info!("Spawned car for client #{}!", client_id);
                } else {
                    let packet_data = format!(
                        "Os:{}:{}:{}-{}:{}",
                        client.get_roles(),
                        client.get_name(),
                        client_id,
                        car_id,
                        car_json_str
                    );
                    let response = RawPacket::from_str(&packet_data);
                    client.write_packet(Packet::Raw(response)).await;
                    let packet_data = format!(
                        "Od:{}-{}",
                        client_id,
                        car_id,
                    );
                    let response = RawPacket::from_str(&packet_data);
                    client.write_packet(Packet::Raw(response)).await;
                    client.unregister_car(car_id);
                    info!("Blocked spawn for client #{}!", client_id);
                }
            }
            'c' => {
                // let split_data = packet.data_as_string().splitn(3, ':').map(|s| s.to_string()).collect::<Vec<String>>();
                // let car_json_str = &split_data.get(2).ok_or(std::fmt::Error)?;
                let client_id = packet.data[3] - 48;
                let car_id = packet.data[5] - 48;
                let car_json = String::from_utf8_lossy(&packet.data[7..]).to_string();
                let response = Packet::Raw(packet.clone());
                for i in 0..self.clients.len() {
                    if self.clients[i].id == client_id {
                        if let Some(car) = self.clients[i].get_car_mut(car_id) {
                            car.car_json = car_json.clone();
                        }
                    } else {
                        // Already looping so more efficient to send here
                        if let Some(udp_addr) = self.clients[i].udp_addr {
                            self.send_udp(udp_addr, &response).await;
                        }
                    }
                }
            }
            'd' => {
                debug!("packet: {:?}", packet);
                let split_data = packet
                    .data_as_string()
                    .splitn(3, [':', '-'])
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>();
                let client_id = split_data[1].parse::<u8>()?;
                let car_id = split_data[2].parse::<u8>()?;
                for i in 0..self.clients.len() {
                    if self.clients[i].id == client_id {
                        self.clients[i].unregister_car(car_id);
                    }
                    // Don't broadcast, we are already looping anyway
                    if let Some(udp_addr) = self.clients[i].udp_addr {
                        self.send_udp(udp_addr, &Packet::Raw(packet.clone())).await;
                    }
                }
                info!("Deleted car for client #{}!", client_id);
            }
            'r' => {
                // TODO: Handle self.allow_respawns (give time penalty in pits? DQ?)
                if self.force_respawn_pits {
                    debug!("Respawning in pits!");
                    let client_id = packet.data[3] - 48;
                    let car_id = packet.data[5] - 48;
                    debug!("client_id: {} / car_id: {}", client_id, car_id);
                    let spawn = self.track_spawns_pit.as_ref().expect("Map did not have pit lane spawns set up!").get_client_spawn(client_id);
                    if let Ok(json) = serde_json::to_string(&RespawnPacketData {
                        pos: RespawnPacketDataPos {
                            x: spawn.pos[0],
                            y: spawn.pos[1],
                            z: spawn.pos[2],
                        },
                        rot: RespawnPacketDataRot {
                            x: spawn.rot[0],
                            y: spawn.rot[1],
                            z: spawn.rot[2],
                            w: spawn.rot[3]
                        },
                    }) {
                        let packet_data = format!("Or:{}-{}:{}", client_id, car_id, json);
                        self.broadcast(Packet::Raw(RawPacket::from_str(&packet_data)), None).await;
                    } else {
                        // TODO: Handle this edge case better!
                        self.broadcast(Packet::Raw(packet), Some(self.clients[client_idx].id)).await;
                    }
                } else {
                    self.broadcast(Packet::Raw(packet), Some(self.clients[client_idx].id)).await;
                }
            }
            't' => {
                self.broadcast(Packet::Raw(packet), Some(self.clients[client_idx].id))
                    .await;
            }
            'm' => {
                self.broadcast(Packet::Raw(packet), None).await;
            }
            _ => error!("Unknown vehicle related packet!\n{:?}", packet), // TODO: Return error here
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

#[derive(Debug)]
pub enum ServerError {
    BrokenPacket,
    CarDoesntExist,
    ClientDoesntExist,
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{:?}", self)?;
        Ok(())
    }
}

impl std::error::Error for ServerError {}
