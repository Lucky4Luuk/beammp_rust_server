use std::net::SocketAddr;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::Instant;

use tokio::net::{TcpListener, UdpSocket};
use tokio::task::JoinHandle;

use num_enum::IntoPrimitive;

use nalgebra::*;

mod backend;
mod car;
mod client;
mod packet;
mod track_limits;
mod spawns;
mod track_path;
mod overlay;
mod physics;

pub use backend::*;
pub use car::*;
pub use client::*;
pub use packet::*;
pub use track_limits::*;
pub use spawns::*;
pub use track_path::*;
pub use overlay::*;
pub use physics::*;

pub use crate::config::Config;

#[derive(PartialEq, IntoPrimitive, Copy, Clone, Debug)]
#[repr(u8)]
enum ServerState {
    Unknown = 0,

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
    countdown: u8,

    allow_spawns: bool,
    force_respawn_pits: bool,
    allow_respawns: bool,

    overlay_update_time: Instant,
    physics_timer: Instant,
    generic_timer0: Instant,
    finish_order: Vec<usize>,
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
            countdown: 5,

            allow_spawns: false,
            force_respawn_pits: false,
            allow_respawns: true,

            overlay_update_time: Instant::now(),
            physics_timer: Instant::now(),
            generic_timer0: Instant::now(),
            finish_order: Vec::new(),
        })
    }

    pub fn set_server_state(&mut self, state: ServerState) {
        debug!("new state: {:?}", state);
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
                if let Some(overlay) = self.unconnected_overlays.get(0) {
                    if self.clients.get(j).ok_or(ServerError::ClientDoesntExist)?.info.as_ref().unwrap().username == overlay.0 {
                        self.clients[j].overlay = Some(self.unconnected_overlays.swap_remove(0).1);
                    }
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

            if self.overlay_update_time.elapsed().as_millis() > 100 {
                let max_laps = if self.server_state == ServerState::Race { self.config.game.max_laps.unwrap_or(0) } else { 0 };
                if let Some(overlay) = &mut client.overlay {
                    overlay.set_max_laps(max_laps).await;
                    overlay.set_state(&self.server_state).await;
                }
                self.overlay_update_time = Instant::now();
            }
        }

        // Physics
        if self.server_state == ServerState::Qualifying || self.server_state == ServerState::Race {
            if self.config.game.server_physics && self.physics_timer.elapsed().as_millis() > 100 {
                // todo!("Not yet implemented! Can't correct the players position and velocity without respawning right now, so this will have to wait for that!");
                // if let Some(client) = &mut self.clients.get_mut(0) {
                    // client.trigger_client_event("SetVelocity", "1;1;1#2;2;2").await;
                // }
                let mut clients = Vec::new();
                for client in &self.clients {
                    if let Some((_, car)) = client.cars.get(0) {
                        let pos: [f64; 3] = car.pos.into();
                        let pos = [pos[0] as f32, pos[1] as f32, pos[2] as f32];
                        let vel: [f64; 3] = car.vel.into();
                        let vel = [vel[0] as f32, vel[1] as f32, vel[2] as f32];
                        let angvel: [f64; 3] = car.rvel.into();
                        let angvel = [angvel[0] as f32, angvel[1] as f32, angvel[2] as f32];
                        let unit_quat = nalgebra::geometry::UnitQuaternion::from_quaternion(car.rot);
                        let rot = unit_quat.euler_angles();
                        let rot = [rot.0 as f32, rot.1 as f32, rot.2 as f32];
                        let hbox = [1.0, 1.0, 1.0];
                        clients.push((client.id, pos, vel, angvel, rot, hbox, false));
                    }
                }
                check_physics(&mut clients);
                for (id, pos, vel, angvel, rot, hbox, has_hit) in &mut clients {
                    if *has_hit {
                        'l: for client in &self.clients {
                            if client.id == *id {
                                if let Some((_, car)) = client.cars.get(0) {
                                    let og_vel: [f64; 3] = car.vel.into();
                                    let og_angvel: [f64; 3] = car.rvel.into();
                                    for i in 0..3 {
                                        vel[i] = vel[i];
                                        angvel[i] = -angvel[i];
                                    }
                                    let data = format!("{};{};{}#{};{};{}", vel[0], vel[1], vel[2], angvel[0], angvel[1], angvel[2]);
                                    client.trigger_client_event("SetVelocity", data).await;
                                }
                                break 'l;
                            }
                        }
                    }
                }
                self.physics_timer = Instant::now();
            }
        }

        if self.server_state == ServerState::Qualifying || self.server_state == ServerState::Race {
            // Track limits
            if let Some(client) = &mut self.clients.get_mut(self.track_limits_client as usize) {
                for (_, car) in &mut client.cars {
                    if let Some(limits) = &self.track_limits {
                        if limits.check_limits([car.pos.x as f32, car.pos.y as f32], car.hitbox_half) {
                            if let Some(start) = car.offtrack_start {
                                let offtrack_time = start.elapsed().as_secs_f32();
                                debug!("Client went {} seconds offtrack!", offtrack_time);
                                client.incidents += 1;
                                // TODO: Time penalty if velocity stays high?
                            }
                            car.offtrack_start = None;
                        } else {
                            let mut intersects_pit = false;
                            if let Some(limits) = &self.track_limits_pit {
                                intersects_pit = limits.check_limits([car.pos.x as f32, car.pos.y as f32], car.hitbox_half);
                            }
                            if car.offtrack_start.is_none() && intersects_pit {
                                car.offtrack_start = Some(Instant::now());
                            }
                        }
                    }

                    // if let Some(limits) = &self.track_limits_pit_exit {
                    //     limits.check_limits([car.pos.x as f32, car.pos.y as f32], [1.0, 1.0]);
                    // }
                }
            }

            // Track path
            if self.server_state == ServerState::Qualifying || self.server_state == ServerState::Race {
                if let Some(client) = &mut self.clients.get_mut(self.track_limits_client as usize) {
                    for (_, car) in &mut client.cars {
                        let active_cp = if car.next_checkpoint == 0 {
                            self.track_checkpoints.len() - 1
                        } else {
                            car.next_checkpoint - 1
                        };
                        if let Some(path) = &self.track_checkpoints.get(active_cp) {
                            // let unit_quat = nalgebra::geometry::UnitQuaternion::from_quaternion(car.rot);
                            // let car_angle = unit_quat.euler_angles().2 / std::f64::consts::PI * 180.0;
                            // let car_forward = car.vel.xy().normalize();
                            // let car_vel_angle = car_forward.y.atan2(car_forward.x) as f32 / std::f32::consts::PI * 180.0;
                            // let track_angle = path.get_angle_at_pos([car.pos.x as f32, car.pos.y as f32]);
                            // let angle_diff = (car_angle as f32 - track_angle).abs() % 360.0;
                            // car.latest_angle_to_track = angle_diff;
                            // let angle_vel_diff = car_vel_angle as f32 - track_angle;
                            // car.latest_vel_angle_to_track = angle_vel_diff;
                            // debug!("track angle: {}", track_angle);
                            // debug!("car angle: {}", car_angle);
                            // debug!("car vel angle: {}", car_vel_angle);
                            // debug!("angle diff: {}", angle_diff);
                            // debug!("angle vel diff: {}", angle_vel_diff);
                            let progress = path.get_percentage_along_track([car.pos.x as f32, car.pos.y as f32]);
                            car.last_progress = progress;
                            // debug!("progress: {}", progress);

                            let car_rot_vel = car.rvel.z;
                            // debug!("car rot vel z {}", car_rot_vel);
                        }
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
                                    car.active_checkpoint = self.track_checkpoints.len() - 1;
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
                                    car.active_checkpoint = car.next_checkpoint;
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
        // let _ = self.udp_socket.writable().await;
        // for i in 0..self.clients.len() {
        //     for k in 0..self.clients[i].cars.len() {
        //         if self.clients[i].cars[k].1.needs_packet {
        //             self.clients[i].cars[k].1.needs_packet = false;
        //             let pos_data = TransformPacket {
        //                 rvel: self.clients[i].cars[k].1.rvel.into(),
        //                 tim: self.clients[i].cars[k].1.tim,
        //                 pos: self.clients[i].cars[k].1.pos.into(),
        //                 ping: self.clients[i].cars[k].1.ping,
        //                 rot: self.clients[i].cars[k].1.rot.coords.into(),
        //                 vel: self.clients[i].cars[k].1.vel.into(),
        //             };
        //             if let Ok(json) = serde_json::to_string(&pos_data) {
        //                 let data = format!("Zp:{}-{}:{}", self.clients[i].id, self.clients[i].cars[k].0, json);
        //                 if self.clients[i].cars[k].1.is_corrected {
        //                     todo!("Not yet implemented! Can't correct the players position and velocity without respawning right now, so this will have to wait for that!");
        //                     self.clients[i].cars[k].1.is_corrected = false;
        //                     if let Some(udp_addr) = self.clients[i].udp_addr {
        //                         // TODO: This breaks all force feedback for a car
        //                         self.send_udp(udp_addr, &Packet::Raw(RawPacket::from_str(&format!("Zp:{}-{}:{}", self.clients[i].id, self.clients[i].cars[k].0, json)))).await;
        //                     }
        //                 }
        //                 let p = Packet::Raw(RawPacket::from_str(&data));
        //                 self.broadcast(p, Some(self.clients[i].id)).await;
        //             }
        //         }
        //     }
        // }

        // Check if clients are allowed to be on the server
        let required_clients = self.config.event.expected_clients.as_ref().unwrap();
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

        // Handle server states
        let elapsed = self.server_state_start.elapsed();
        match self.server_state {
            ServerState::WaitingForClients => {
                if self.config.event.expected_clients.is_some() && elapsed.as_secs() < 150 {
                    let mut joined_clients = required_clients.clone();
                    joined_clients.retain(|name| {
                        for client in &self.clients {
                            if client.info.as_ref().unwrap().username.trim() == name.trim() {
                                return false;
                            }
                        }
                        true
                    });
                    if joined_clients.len() == 0 {
                        // All expected clients are in the server
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
                    self.connect_runtime_handle.abort(); // Only abort here, otherwise we might stop joining clients from finishing joining
                    self.set_server_state(ServerState::WaitingForSpawns);

                    for client in &mut self.clients {
                        client.ready = false;
                    }
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
                            let data = format!(
                                "{};{};{}#{};{};{};{}",
                                spawn.pos[0],
                                spawn.pos[1],
                                spawn.pos[2],

                                spawn.rot[0],
                                spawn.rot[1],
                                spawn.rot[2],
                                spawn.rot[3],
                            );
                            client.trigger_client_event("Respawn", data).await;
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
                    lap_id.sort_unstable_by(|(ida, timea), (idb, timeb)| timea.cmp(timeb));
                    let mut j = 1;
                    for (i, _) in &lap_id {
                        self.clients[*i].grid_spot = j;
                        j += 1;
                    }

                    // Reset client lap counters and such
                    for client in &mut self.clients {
                        for (_, car) in &mut client.cars {
                            car.laps = 0;
                            car.lap_times = Vec::new();
                            car.next_checkpoint = 0;
                            car.lap_start = None;
                            car.offtrack_start = None;
                        }
                    }

                    self.set_server_state(ServerState::LiningUp);
                    self.allow_respawns = false;
                    self.allow_spawns = false;
                    self.force_respawn_pits = false;
                    self.generic_timer0 = Instant::now();

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
                        let gs = client.grid_spot;
                        let grid_spot;
                        if gs % 2 == 0 {
                            // Even
                            let grid_spot_id = gs / 2;
                            grid_spot = self.track_spawns_even.as_ref().expect("Map does not have spawns set up!").get_client_spawn(grid_spot_id as u8 - 1);
                        } else if gs % 2 == 1 {
                            // Odd
                            let grid_spot_id = gs / 2;
                            grid_spot = self.track_spawns_odd.as_ref().expect("Map does not have spawns set up!").get_client_spawn(grid_spot_id as u8);
                        } else {
                            unreachable!();
                        }
                        // let grid_spot = self.track_spawns_odd.as_ref().expect("Map does not have spawns set up!").get_client_spawn(gs as u8 - 1);
                        if let Some((id, car)) = client.cars.get(0) {
                            let deltaxy = crate::util::distance([grid_spot.pos[0] as f32, grid_spot.pos[1] as f32], [car.pos.x as f32, car.pos.y as f32]);
                            let deltaz = (grid_spot.pos[2] - car.pos.z).abs();
                            debug!("deltaxy: {:?}", deltaxy);
                            debug!("deltaz: {}", deltaz);
                            if deltaxy > 1.0 || deltaz > 3.0 {
                                let data = format!(
                                    "{};{};{}#{};{};{};{}",
                                    grid_spot.pos[0],
                                    grid_spot.pos[1],
                                    grid_spot.pos[2],

                                    grid_spot.rot[0],
                                    grid_spot.rot[1],
                                    grid_spot.rot[2],
                                    grid_spot.rot[3],
                                );
                                client.trigger_client_event("Respawn", data).await;
                            }
                        }
                    }
                    if all_ready {
                        self.set_server_state(ServerState::Countdown);
                        self.generic_timer0 = Instant::now();
                    }
                }
                if self.server_state_start.elapsed().as_secs() > 45 {
                    // Time out on lining up
                    // Kick all clients who are not ready
                    for i in 0..self.clients.len() {
                        if !self.clients[i].ready {
                            self.clients[i].kick("Not ready in time!").await;
                        } else {
                            if let Some(overlay) = &mut self.clients[i].overlay {
                                overlay.set_state(&ServerState::Countdown).await;
                            }
                        }
                    }
                    self.set_server_state(ServerState::Countdown);
                    self.generic_timer0 = Instant::now();
                }
            }
            ServerState::Countdown => {
                // TODO: Prevent jumping the start
                if self.generic_timer0.elapsed().as_millis() > 1000 && self.countdown > 0 {
                    self.countdown -= 1;
                    for client in &mut self.clients {
                        if let Some(overlay) = &mut client.overlay {
                            overlay.set_countdown(self.countdown).await;
                        }
                    }
                    if self.countdown == 0 {
                        self.set_server_state(ServerState::Race);
                        self.force_respawn_pits = true;
                    }
                    self.generic_timer0 = Instant::now();
                }
            }
            ServerState::Race => {
                if self.generic_timer0.elapsed().as_millis() > 50 {
                    self.generic_timer0 = Instant::now();
                    let mut order = Vec::new();
                    for (i, client) in self.clients.iter().enumerate() {
                        let progress = client.get_progress();
                        order.push((i, progress));
                    }
                    order.sort_unstable_by(|(ia, pa), (ib, pb)| pb.partial_cmp(pa).unwrap());
                    let player_count = order.len();
                    let mut j = 1;
                    for (i, _) in &order {
                        if let Some(client) = self.clients.get_mut(*i) {
                            if let Some(overlay) = &mut client.overlay {
                                overlay.set_position(j, player_count).await;
                            }
                        }
                        j += 1;
                    }
                }

                let mut all_finished = true;
                for i in 0..self.clients.len() {
                    let client = self.clients.get(i);
                    if client.is_none() { continue; }
                    drop(client);
                    if self.clients[i].cars[0].1.laps > self.config.game.max_laps.unwrap_or(5) && self.clients[i].finished == false {
                        self.clients[i].finished = true;
                        self.finish_order.push(i);
                    }
                    if !self.clients[i].finished {
                        all_finished = false;
                    }
                }
                if all_finished {
                    self.set_server_state(ServerState::Finish);
                    self.generic_timer0 = Instant::now();
                }
            }
            ServerState::Finish => {
                if self.generic_timer0.elapsed().as_secs() > 30 {
                    std::process::exit(0);
                }
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
        let data = packet.get_data();
        if data.len() > 400 {
            trace!("Compressing...");
            let mut compressed: Vec<u8> = Vec::with_capacity(100_000);
            let mut compressor = flate2::Compress::new(flate2::Compression::best(), true);
            if let Err(e) = compressor.compress_vec(
                data,
                &mut compressed,
                flate2::FlushCompress::Sync,
            ) {
                error!("Compression failed!");
                return;
            }
            let mut new_data = "ABG:".as_bytes()[..4].to_vec();
            new_data.append(&mut compressed);
            if let Err(e) = self.udp_socket.try_send_to(&new_data, udp_addr) {
                error!("UDP Packet send error: {:?}", e);
            }
        } else {
            if let Err(e) = self.udp_socket.try_send_to(&data, udp_addr) {
                error!("UDP Packet send error: {:?}", e);
            }
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
                                } else {
                                    if let Some(udp_addr) = self.clients[i].udp_addr {
                                        self.send_udp(udp_addr, &p).await;
                                    }
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
                        // if let Some(udp_addr) = self.clients[i].udp_addr {
                        //     self.write_udp(udp_addr, &response).await;
                        // }
                        self.clients[i].write_packet(response.clone()).await;
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
                    // if let Some(udp_addr) = self.clients[i].udp_addr {
                    //     self.send_udp(udp_addr, &Packet::Raw(packet.clone())).await;
                    // }
                    self.clients[i].write_packet(Packet::Raw(packet.clone())).await;
                }
                info!("Deleted car for client #{}!", client_id);
            }
            'r' => {
                // TODO: Handle self.allow_respawns (give time penalty in pits? DQ?)
                if self.force_respawn_pits {
                    debug!("Respawning in pits!");
                    let client_id = packet.data[3] - 48;
                    let car_id = packet.data[5] - 48;
                    let json = String::from_utf8_lossy(&packet.data[7..]).to_string();
                    let data: RespawnPacketData = serde_json::from_str(&json)?;
                    for client in &mut self.clients {
                        if client.id == client_id {
                            for (id, car) in &mut client.cars {
                                if *id == car_id {
                                    car.next_checkpoint = 0;
                                    // Yucky code
                                    if self.server_state == ServerState::Qualifying {
                                        car.lap_start = None;
                                        car.offtrack_start = None;
                                    }
                                }
                            }
                        }
                    }
                    debug!("client_id: {} / car_id: {}", client_id, car_id);
                    let spawn = self.track_spawns_pit.as_ref().expect("Map did not have pit lane spawns set up!").get_client_spawn(client_id);
                    if self.server_state != ServerState::LiningUp && self.server_state != ServerState::Countdown && crate::util::distance3d(spawn.pos, [data.pos.x, data.pos.y, data.pos.z]) > 1.0 {
                        let data = format!(
                            "{};{};{}#{};{};{};{}",
                            spawn.pos[0],
                            spawn.pos[1],
                            spawn.pos[2],

                            spawn.rot[0],
                            spawn.rot[1],
                            spawn.rot[2],
                            spawn.rot[3],
                        );
                        self.clients[client_idx].trigger_client_event("Respawn", data).await;
                    }
                }
                self.broadcast(Packet::Raw(packet), Some(self.clients[client_idx].id)).await;
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
