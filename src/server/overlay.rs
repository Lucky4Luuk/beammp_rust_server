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

    async fn write(&mut self, data: &[u8]) {
        if let Err(e) = self.socket.write(&(data.len() as u32).to_le_bytes()).await {
            error!("{:?}", e);
        }
        if let Err(e) = self.socket.write(data).await {
            error!("{:?}", e);
        }
    }

    pub async fn set_laps(&mut self, laps: usize) {
        let data = format!("L{}", laps);
        let data = data.as_bytes();
        let _ = self.socket.writable().await;
        self.write(&data).await;
    }

    pub async fn set_max_laps(&mut self, max_laps: usize) {
        let data = format!("M{}", max_laps);
        let data = data.as_bytes();
        let _ = self.socket.writable().await;
        self.write(&data).await;
    }

    pub async fn set_state(&mut self, state: &ServerState) {
        let state_id = match state {
            ServerState::Qualifying => 1,
            ServerState::Race => 2,
            _ => 0,
        };
        let data = format!("S{}", state_id);
        let data = data.as_bytes();
        let _ = self.socket.writable().await;
        self.write(&data).await;
    }

    pub async fn set_lap_times(&mut self, laps: &Vec<std::time::Duration>) {
        let data = laps
            .iter()
            .map(|duration| format!("{}:{}.{}", (duration.as_secs_f32() / 60.0).floor(), (duration.as_secs_f32() % 60.0) as usize, duration.subsec_millis()))
            .collect::<Vec<String>>()
            .join("-");
        let data = format!("Q{}", data);
        let data = data.as_bytes();
        let _ = self.socket.writable().await;
        self.write(&data).await;
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
