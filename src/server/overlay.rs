use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;

use super::ServerState;

pub struct Overlay {
    socket: TcpStream,
}

impl Overlay {
    pub async fn new(socket: TcpStream) -> anyhow::Result<(String, Self)> {
        let mut buf = vec![0u8; 1024];
        socket.readable().await?;
        match socket.try_read(&mut buf) {
            Ok(0) => return Err(OverlayError::ConnectionError.into()),
            Ok(_) => {}
            Err(e) => {
                error!("{:?}", e);
                return Err(OverlayError::ConnectionError.into())
            }
        }
        if buf[0] as char != 'H' {
            return Err(OverlayError::ConnectionError.into());
        }
        let mut end = 1;
        for i in 1..1024 {
            if buf[i] == 0 || buf[i] as char == '\0' {
                end = i;
                break;
            }
        }
        let expected_name = String::from_utf8_lossy(&buf[1..end]).to_string();
        debug!("Overlay belongs to client {}", expected_name);

        Ok( (
            expected_name,
            Self {
                socket: socket,
            }
        ) )
    }

    pub async fn set_laps(&mut self, laps: usize) {
        let _ = self.socket.writable().await;
        if let Err(e) = self.socket.write(format!("L{}", laps).as_bytes()).await {
            error!("{:?}", e);
        }
    }

    pub async fn set_max_laps(&mut self, max_laps: usize) {
        let _ = self.socket.writable().await;
        if let Err(e) = self.socket.write(format!("M{}", max_laps).as_bytes()).await {
            error!("{:?}", e);
        }
    }

    pub async fn set_state(&mut self, state: &ServerState) {
        let _ = self.socket.writable().await;
        let state_id = match state {
            ServerState::Qualifying => 1,
            ServerState::Race => 2,
            _ => 0,
        };
        if let Err(e) = self.socket.write(format!("S{}", state_id).as_bytes()).await {
            error!("{:?}", e);
        }
    }
}

#[derive(Debug)]
pub enum OverlayError {
    ConnectionError,
}

impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{:?}", self)?;
        Ok(())
    }
}

impl std::error::Error for OverlayError {}
