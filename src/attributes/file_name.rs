use crate::error::Error;
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, Utc};
use std::io::{Read, Seek};
//
use crate::raw::FileReference;

#[derive(Debug)]
pub struct FileName {
    pub parent_file_reference: FileReference,
    pub creation_time: DateTime<Utc>,
    pub modification_time: DateTime<Utc>,
    pub mft_modification_time: DateTime<Utc>,
    pub access_time: DateTime<Utc>,
    pub allocated_size: u64,
    pub real_size: u64,
    pub flags: u32,
    pub reparse_value: u32,
    pub name_length: u8,
    pub name_space: u8,
    pub name: String,
}

impl FileName {
    pub fn parent_reference_from_buffer(buffer: &Vec<u8>) -> crate::Result<FileReference> {
        let mut reader = std::io::Cursor::new(buffer);
        read_value!(reader, parent_file_reference, read_u64);
        Ok(FileReference::from(parent_file_reference))
    }
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> crate::Result<Self> {
        read_value!(reader, parent_file_reference, read_u64);
        let parent_file_reference = FileReference::from(parent_file_reference);
        read_value!(reader, creation_time, read_u64);
        read_value!(reader, modification_time, read_u64);
        read_value!(reader, mft_modification_time, read_u64);
        read_value!(reader, access_time, read_u64);
        read_value!(reader, allocated_size, read_u64);
        read_value!(reader, real_size, read_u64);
        read_value!(reader, flags, read_u32);
        read_value!(reader, reparse_value, read_u32);
        let name_length = reader
            .read_u8()
            .map_err(|e| Error::into_value_read_error(e.into(), "name_length", "read_u8"))?;
        let name_space = reader
            .read_u8()
            .map_err(|e| Error::into_value_read_error(e.into(), "name_space", "read_u8"))?;

        let mut name = String::new();
        for _ in 0..name_length {
            let c = reader
                .read_u16::<LittleEndian>()
                .map_err(|e| Error::into_value_read_error(e.into(), "name_char", "read_u16"))?;
            name.push(c as u8 as char);
        }

        Ok(Self {
            parent_file_reference,
            creation_time: super::convert_u64_to_datetime(creation_time),
            modification_time: super::convert_u64_to_datetime(modification_time),
            mft_modification_time: super::convert_u64_to_datetime(mft_modification_time),
            access_time: super::convert_u64_to_datetime(access_time),
            allocated_size,
            real_size,
            flags,
            reparse_value,
            name_length,
            name_space,
            name,
        })
    }
}

// 0x6d13400 as decimal is 113,000,000

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_name_test() {
        let data = vec![
            0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x7a, 0xac, 0xec, 0x4f, 0x4c, 0xcc,
            0xd8, 0x01, 0x7a, 0xac, 0xec, 0x4f, 0x4c, 0xcc, 0xd8, 0x01, 0x7a, 0xac, 0xec, 0x4f,
            0x4c, 0xcc, 0xd8, 0x01, 0x7a, 0xac, 0xec, 0x4f, 0x4c, 0xcc, 0xd8, 0x01, 0x00, 0x40,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x03, 0x24, 0x00, 0x4d, 0x00,
            0x46, 0x00, 0x54, 0x00,
        ];
        let mut reader = std::io::Cursor::new(data);
        let file_name = FileName::from_reader(&mut reader).unwrap();
        println!("{:?}", file_name);
    }
}
