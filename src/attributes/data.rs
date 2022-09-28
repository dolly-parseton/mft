use crate::error::Error;
use std::io::Read;

#[derive(Debug)]
pub enum Data {
    Base64(String),
    ZoneIdentifier(String),
}

impl Data {
    pub fn from_buffer(buffer: &Vec<u8>, is_zone_identifier: bool) -> crate::Result<Self> {
        if is_zone_identifier {
            let mut reader = std::io::Cursor::new(buffer);
            let mut data = String::new();
            reader
                .read_to_string(&mut data)
                .map_err(|e| Error::into_value_read_error(e.into(), "data", "read_to_string"))?;
            Ok(Self::ZoneIdentifier(data))
        } else {
            let data = base64::encode(buffer);
            Ok(Self::Base64(data))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn data_zone_identifier_test() {
        let data = vec![
            0x5b, 0x5a, 0x6f, 0x6e, 0x65, 0x54, 0x72, 0x61, 0x6e, 0x73, 0x66, 0x65, 0x72, 0x5d,
            0x0d, 0x0a, 0x5a, 0x6f, 0x6e, 0x65, 0x49, 0x64, 0x3d, 0x33, 0x0d, 0x0a, 0x48, 0x6f,
            0x73, 0x74, 0x55, 0x72, 0x6c, 0x3d, 0x61, 0x62, 0x6f, 0x75, 0x74, 0x3a, 0x69, 0x6e,
            0x74, 0x65, 0x72, 0x6e, 0x65, 0x74, 0x0d, 0x0a,
        ];
        let data = Data::from_buffer(&data, true).unwrap();
        println!("{:?}", data);
    }
}
