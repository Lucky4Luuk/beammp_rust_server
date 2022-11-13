use std::sync::atomic::{AtomicU32, Ordering};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::packet::*;

static ATOMIC_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(PartialEq)]
pub enum ClientState {
    None,
    Connecting,
    Close,
}

pub struct Client {
    pub id: u32,
    ip: String,
    socket: TcpStream,
    pub state: ClientState,
}

impl Client {
    pub fn new(socket: TcpStream, ip: String) -> Self {
        let id = ATOMIC_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            id: id,
            ip: ip,
            socket: socket,
            state: ClientState::Connecting,
        }
    }

    pub async fn authenticate(&mut self) -> anyhow::Result<()> {
        debug!("Authenticating client {}...", self.id);

        self.socket.readable().await?;
        // let packet = self.read_packet().await?.expect("Did not receive packet?");
        // debug!("packet: {:?}", packet);
        let code = self.read_raw(1).await?[0];

        match code as char {
            'C' => {
                // TODO: Check client version
                trace!("Client version packet");
            },
            'D' => {
                trace!("Download packet");
            },
            _ => return Err(ClientError::AuthenticateError.into()),
        }

        self.socket.writable().await?;
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
        debug!("{:?}", self.read_packet().await);

        self.state = ClientState::None;

        debug!("Authentication of client {} succesfully completed!", self.id);
        Ok(())
    }

    pub async fn process(&mut self) -> anyhow::Result<()> {
        if let Some(packet) = self.read_packet().await? {
            debug!("Packet: {:?}", packet);
            self.parse_packet(packet).await?;
        }
        Ok(())
    }

    pub fn close(&mut self) {
        self.state = ClientState::Close;
    }

    async fn parse_packet(&mut self, packet: RawPacket) -> anyhow::Result<()> {
        let packet_identifier = packet.header[0] as char;
        let string_data = String::from_utf8_lossy(&packet.data[..]);
        match packet_identifier {
            _ => debug!("Unknown packet - String data: `{}`; Array: `{:?}`; Header: `{:?}`", string_data, packet.data, packet.header),
        }
        Ok(())
    }

    async fn read_raw(&mut self, count: usize) -> anyhow::Result<Vec<u8>> {
        let mut b = vec![0u8; count];
        self.socket.read_exact(&mut b).await?;
        Ok(b)
    }

    async fn read_packet(&mut self) -> anyhow::Result<Option<RawPacket>> {
        let mut header = [0u8; 4];
        match self.socket.try_read(&mut header) {
            Ok(0) => {
                error!("Socket is readable, yet has 0 bytes to read! Disconnecting client...");
                self.close();
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
                self.close();
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
            header: header,
            data: data[..data_size].to_vec(),
        }))
    }

    async fn write_packet(&mut self, packet: Packet) -> anyhow::Result<()> {
        trace!("Sending packet...");
        let raw_data: Box<[u8]>;
        let header: [u8; 4];
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

        self.socket.writable().await?;
        self.socket.write(&header).await?;
        self.socket.write(&raw_data).await?;
        trace!("Packet sent!");

        Ok(())
    }
}

#[derive(Debug)]
pub enum ClientError {
    WritePacketError,
    AuthenticateError,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{:?}", self)?;
        Ok(())
    }
}

impl std::error::Error for ClientError {}
