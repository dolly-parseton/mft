use byteorder::ReadBytesExt;
use chrono::{DateTime, Utc};
use std::io::{Read, Seek};

#[derive(Debug)]
pub struct StandardInformation {
    pub creation_time: DateTime<Utc>,
    pub modification_time: DateTime<Utc>,
    pub mft_modification_time: DateTime<Utc>,
    pub access_time: DateTime<Utc>,
    pub file_attributes: u32,
    pub max_versions: u32,
    pub version_number: u32,
    pub class_id: u32,
    pub owner_id: u32,
    pub security_id: u32,
    pub quota_charged: u64,
    pub update_sequence_number: u64,
}

impl StandardInformation {
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> crate::Result<Self> {
        read_value!(reader, creation_time, read_u64);
        read_value!(reader, modification_time, read_u64);
        read_value!(reader, mft_modification_time, read_u64);
        read_value!(reader, access_time, read_u64);
        read_value!(reader, file_attributes, read_u32);
        read_value!(reader, max_versions, read_u32);
        read_value!(reader, version_number, read_u32);
        read_value!(reader, class_id, read_u32);
        read_value!(reader, owner_id, read_u32);
        read_value!(reader, security_id, read_u32);
        read_value!(reader, quota_charged, read_u64);
        read_value!(reader, update_sequence_number, read_u64);
        Ok(Self {
            creation_time: super::convert_u64_to_datetime(creation_time),
            modification_time: super::convert_u64_to_datetime(modification_time),
            mft_modification_time: super::convert_u64_to_datetime(mft_modification_time),
            access_time: super::convert_u64_to_datetime(access_time),
            file_attributes,
            max_versions,
            version_number,
            class_id,
            owner_id,
            security_id,
            quota_charged,
            update_sequence_number,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn standard_info_test() {
        let data = [
            0x7a, 0xac, 0xec, 0x4f, 0x4c, 0xcc, 0xd8, 0x01, 0x7a, 0xac, 0xec, 0x4f, 0x4c, 0xcc,
            0xd8, 0x01, 0x7a, 0xac, 0xec, 0x4f, 0x4c, 0xcc, 0xd8, 0x01, 0x7a, 0xac, 0xec, 0x4f,
            0x4c, 0xcc, 0xd8, 0x01, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        let mut reader = Cursor::new(data);
        let standard_info = StandardInformation::from_reader(&mut reader).unwrap();
        println!("{:?}", standard_info);
    }
}
