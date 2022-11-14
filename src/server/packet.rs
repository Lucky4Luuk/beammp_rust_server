#[derive(Clone)]
pub enum Packet {
    Raw(RawPacket),
    Notification(NotificationPacket),
}

impl Packet {
    pub fn get_header(&self) -> u32 {
        match self {
            Self::Raw(raw) => raw.header,
            Self::Notification(msg) => self.get_data().len() as u32,
        }
    }

    pub fn get_data(&self) -> &[u8] {
        match self {
            Self::Raw(raw) => &raw.data,
            Self::Notification(p) => p.0.as_bytes(),
        }
    }
}

#[derive(Clone)]
pub struct NotificationPacket(String);

impl NotificationPacket {
    pub fn new<S: Into<String>>(msg: S) -> Self {
        Self(format!("J{}", msg.into()))
    }
}

/// Protocol:
/// Header: 4 bytes, contains data size
/// Data: Contains packet data
#[derive(Clone)]
pub struct RawPacket {
    pub header: u32,
    pub data: Vec<u8>,
}

impl RawPacket {
    pub fn from_code(code: char) -> Self {
        Self {
            header: 1,
            data: vec![code as u8],
        }
    }

    pub fn from_str(str_data: &str) -> Self {
        let data = str_data.as_bytes().to_vec();
        Self {
            header: data.len() as u32,
            data: data,
        }
    }

    pub fn data_as_string(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }
}

impl std::fmt::Debug for RawPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "Header: `{:?}` - Bytes: `{:?}` - String: `{}`", self.header, self.data, self.data_as_string())?;
        Ok(())
    }
}
