pub enum Packet {
    Raw(RawPacket),
}

/// Protocol:
/// Header: 4 bytes, contains data size
/// Data: Contains packet data
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
