use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::collections::HashMap;

use tokio::net::{TcpStream, tcp::{OwnedReadHalf, OwnedWriteHalf}};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio::sync::Mutex;

use glam::*;

use serde::Deserialize;

use super::packet::*;
use super::backend::*;

static ATOMIC_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(PartialEq)]
pub enum ClientState {
    None,
    Connecting,
    Syncing,
    Disconnect,
}

#[derive(Deserialize, Debug)]
pub struct UserData {
    pub createdAt: String,
    pub guest: bool,
    pub roles: String,
    pub username: String,

    pub identifiers: Vec<String>,
}

pub struct Client {
    pub id: u32,
    ip: String,

    socket: OwnedReadHalf,
    write_half: Arc<Mutex<OwnedWriteHalf>>,
    write_runtime: JoinHandle<()>,
    write_runtime_sender: tokio::sync::mpsc::Sender<Packet>,

    pub state: ClientState,
    pub info: Option<UserData>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.write_runtime.abort();
    }
}

impl Client {
    pub fn new(socket: TcpStream, ip: String) -> Self {
        let id = ATOMIC_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

        let (read_half, write_half) = socket.into_split();
        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        let write_half = Arc::new(Mutex::new(write_half));
        let write_half_ref = Arc::clone(&write_half);
        let handle: JoinHandle<()> = tokio::spawn(async move {
            loop {
                if let Some(packet) = rx.recv().await {
                    trace!("Runtime received packet...");
                    let mut lock = write_half_ref.lock().await;
                    if let Err(e) = lock.writable().await { error!("{:?}", e); }
                    trace!("Runtime sending packet!");
                    let raw_data: Box<[u8]>;
                    let header: u32;
                    match packet {
                        Packet::Raw(packet) => {
                            header = packet.header;
                            raw_data = packet.data.into_boxed_slice();
                        },
                        _ => {
                            error!("Attempting to send unknown packet!");
                            continue;
                        },
                    };
                    if let Err(e) = lock.write(&header.to_le_bytes()).await { error!("{:?}", e); }
                    if let Err(e) = lock.write(&raw_data).await { error!("{:?}", e); }
                    trace!("Runtime sent packet!");
                    drop(lock);
                }
            }
        });

        Self {
            id: id,
            ip: ip,

            socket: read_half,
            write_half: write_half,
            write_runtime: handle,
            write_runtime_sender: tx,

            state: ClientState::Connecting,
            info: None,
        }
    }

    pub async fn authenticate(&mut self) -> anyhow::Result<()> {
        debug!("Authenticating client {}...", self.id);

        self.socket.readable().await?;
        // Authentication works a little differently than normal
        // Not sure why, but the BeamMP source code shows they
        // also only read a single byte during authentication
        let code = self.read_raw(1).await?[0];

        match code as char {
            'C' => {
                // TODO: Check client version
                trace!("Client version packet");
                self.socket.readable().await?;
                let packet = self.read_packet_waiting().await?;
                debug!("{:?}", packet);
            },
            // 'D' => {
            //     trace!("Download packet");
            //     todo!("Implement downloading phase");
            // },
            // 'P' => {
            //     self.queue_packet(Packet::Raw(RawPacket::from_code('P'))).await;
            // },
            _ => return Err(ClientError::AuthenticateError.into()),
        }

        self.write_packet(Packet::Raw(RawPacket::from_code('S'))).await?;
        // self.write_packet(Packet::Raw(RawPacket {
        //     header: ['P' as u8, 5, 0, 0],
        //     data: self.id.to_string().into_bytes(),
        // })).await.map_err(|e| { error!("{:?}", e); e })?;
        // self.write_packet(Packet::Raw(RawPacket {
        //     header: ['M' as u8, 5, 0, 0],
        //     data: crate::get_map_name().into_bytes(),
        // })).await.map_err(|e| { error!("{:?}", e); e })?;

        self.socket.readable().await?;
        if let Some(packet) = self.read_packet_waiting().await? {
            debug!("packet: {:?}", packet);
            if packet.data.len() > 50 {
                self.kick("Player key too big!").await;
                return Err(ClientError::AuthenticateError.into());
            }
            let mut json = HashMap::new();
            json.insert("key".to_string(), packet.data_as_string());
            let user_data: UserData = authentication_request("pkToUser", json).await.map_err(|e| { error!("{:?}", e); e })?;
            debug!("user_data: {:?}", user_data);
            self.info = Some(user_data);
        } else {
            self.kick("Client never sent public key! If this error persists, try restarting your game.").await;
        }

        self.write_packet(Packet::Raw(RawPacket::from_str(&format!("P{}", self.id)))).await?;

        self.state = ClientState::Syncing;

        debug!("Authentication of client {} succesfully completed! Syncing now...", self.id);
        self.sync().await?;

        Ok(())
    }

    // TODO: https://github.com/BeamMP/BeamMP-Server/blob/master/src/TNetwork.cpp#L619
    pub async fn sync(&mut self) -> anyhow::Result<()> {
        'syncing: while self.state == ClientState::Syncing {
            self.socket.readable().await?;
            if let Some(packet) = self.read_packet().await? {
                if packet.data.len() == 0 { continue; }
                if packet.data.len() == 4 {
                    if packet.data == [68, 111, 110, 101] {
                        break 'syncing;
                    }
                }
                match packet.data[0] as char {
                    'S' if packet.data.len() > 1 => {
                        match packet.data[1] as char {
                            'R' => self.write_packet(Packet::Raw(RawPacket::from_code('-'))).await?,
                            _ => error!("Unknown packet! {:?}", packet),
                        }
                    },
                    _ => error!("Unknown packet! {:?}", packet),
                }
            }
        }
        self.state = ClientState::None;
        trace!("Done syncing!");
        Ok(())
    }

    /// This function should never block. It should simply check if there's a
    /// packet, and then and only then should it read it. If this were to block, the server
    /// would come to a halt until this function unblocks.
    pub async fn process(&mut self) -> anyhow::Result<()> {
        if let Some(packet) = self.read_packet().await? {
            debug!("Packet: {:?}", packet);
            self.parse_packet(packet).await?;
        }
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.state = ClientState::Disconnect;
    }

    pub async fn kick(&mut self, msg: &str) {
        // let _ = self.socket.writable().await;
        // let _ = self.write_packet(Packet::Raw(RawPacket::from_str(&format!("K{}", msg)))).await;
        // self.disconnect();
        self.queue_packet(Packet::Raw(RawPacket::from_str(&format!("K{}", msg)))).await;
    }

    async fn parse_packet(&mut self, packet: RawPacket) -> anyhow::Result<()> {
        if packet.data.len() > 0 {
            let packet_identifier = packet.data[0] as char;
            let string_data = String::from_utf8_lossy(&packet.data[..]);
            match packet_identifier {
                _ => debug!("Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`", string_data, packet.data, packet.header),
            }
        }
        Ok(())
    }

    async fn read_raw(&mut self, count: usize) -> anyhow::Result<Vec<u8>> {
        let mut b = vec![0u8; count];
        self.socket.read_exact(&mut b).await?;
        Ok(b)
    }

    async fn read_packet_waiting(&mut self) -> anyhow::Result<Option<RawPacket>> {
        let start = std::time::Instant::now();
        'wait: loop {
            if let Some(packet) = self.read_packet().await? {
                return Ok(Some(packet));
            }
            if start.elapsed().as_secs() >= 5 {
                break 'wait;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(ClientError::ConnectionTimeout.into())
    }

    /// Must be non-blocking
    async fn read_packet(&mut self) -> anyhow::Result<Option<RawPacket>> {
        let mut header = [0u8; 4];
        match self.socket.try_read(&mut header) {
            Ok(0) => {
                error!("Socket is readable, yet has 0 bytes to read! Disconnecting client...");
                self.disconnect();
                return Ok(None);
            },
            Ok(_n) => {},
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(None);
            },
            Err(e) => return Err(e.into()),
        }

        let mut data = vec![0u8; 1024];
        let data_size;
        match self.socket.try_read(&mut data) {
            Ok(0) => {
                error!("Socket is readable, yet has 0 bytes to read! Disconnecting client...");
                self.disconnect();
                return Ok(None);
            },
            Ok(n) => data_size = n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // debug!("Packet appears to be ready, yet can't be read yet!");
                // self.socket.read(&mut data).await?;
                return Ok(None);
            },
            Err(e) => return Err(e.into()),
        }

        Ok(Some(RawPacket {
            header: data_size as u32,
            data: data[..data_size].to_vec(),
        }))
    }

    /// Blocking write
    async fn write_packet(&mut self, packet: Packet) -> anyhow::Result<()> {
        let mut lock = self.write_half.lock().await;
        lock.writable().await?;
        trace!("Sending packet!");
        let raw_data: Box<[u8]>;
        let header: u32;
        match packet {
            Packet::Raw(packet) => {
                header = packet.header;
                raw_data = packet.data.into_boxed_slice();
            },
            _ => {
                error!("Attempting to send unknown packet!");
                return Err(ClientError::WritePacketError.into());
            },
        };
        lock.write(&header.to_le_bytes()).await?;
        lock.write(&raw_data).await?;
        trace!("Packet sent!");
        drop(lock);
        Ok(())
    }

    async fn queue_packet(&mut self, packet: Packet) {
        self.write_runtime_sender.send(packet).await;
    }
}

#[derive(Debug)]
pub enum ClientError {
    WritePacketError,
    AuthenticateError,
    ConnectionTimeout,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{:?}", self)?;
        Ok(())
    }
}

impl std::error::Error for ClientError {}
