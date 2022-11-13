pub enum Packet {
    Raw(RawPacket),
}

pub struct RawPacket {
    pub header: [u8; 4],
    pub data: Vec<u8>,
}

impl RawPacket {
    pub fn from_code(code: char) -> Self {
        Self {
            header: [code as u8, 5, 0, 0],
            data: Vec::new(),
        }
    }

    pub fn data_as_string(&self) -> String {
        String::from_utf8_lossy(&self.data[..]).to_string()
    }
}

impl std::fmt::Debug for RawPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "Header: `{:?}` - Bytes: `{:?}` - String: `{}`", self.header, self.data, self.data_as_string())?;
        Ok(())
    }
}
