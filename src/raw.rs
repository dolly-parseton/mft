use crate::error::Error;
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};

// Helper bits

// Value reader macro, handles error translation as well
macro_rules! read_value {
    ($reader:ident, $value:ident, $type:ident) => {
        let $value = $reader.$type::<byteorder::LittleEndian>().map_err(|e| {
            crate::error::Error::into_value_read_error(
                e.into(),
                stringify!($value),
                stringify!($type),
            )
        })?;
    };
}

// File reference - Used in header and a few attributes to reference other entires (.entry), we manually create an entry n with i on parse.

#[derive(Clone, Debug)]
// https://github.com/libyal/libfsntfs/blob/main/documentation/New%20Technologies%20File%20System%20(NTFS).asciidoc#53-the-file-reference
pub struct FileReference {
    pub entry: u64,
    pub sequence: u16,
}

impl From<u64> for FileReference {
    fn from(value: u64) -> Self {
        let mut bytes = value.to_le_bytes();
        let sequence = LittleEndian::read_u16(&bytes[6..8]);
        bytes[6] = 0;
        bytes[7] = 0;
        let entry = LittleEndian::read_u64(&bytes);
        Self { entry, sequence }
    }
}

impl PartialEq<u64> for FileReference {
    fn eq(&self, other: &u64) -> bool {
        // Just match entry
        self.entry == *other
    }
}

impl PartialEq for FileReference {
    fn eq(&self, other: &Self) -> bool {
        // Just match entry
        self.entry == other.entry
    }
}

#[derive(Debug)]
pub struct Entry {
    // Meta
    pub offset: u64,
    pub entry_n: u64,
    // Content
    pub header: Header,
    pub attributes: Vec<Attribute>,
}

impl Entry {
    pub fn get_entry_bytes<R: Read + Seek>(reader: &mut R, offset: u64) -> crate::Result<Vec<u8>> {
        // Ensure we're at the right offset
        reader.seek(SeekFrom::Start(offset))?;
        // Read first 48 bytes
        let mut buffer: Vec<u8> = Vec::new();
        reader
            .take(48)
            .read_to_end(&mut buffer)
            .map_err(|e| Error::into_buffer_fill_error(e.into(), offset, 48))?;
        // Generate header from first 48 bytes
        let header = Header::from_buffer(&buffer)?;
        // End early if header is zeroed
        if header.is_zeroed() {
            // If zeroed assume default size
            reader
                .take(crate::MFT_RECORD_SIZE - 48)
                .read_to_end(&mut buffer)
                .map_err(|e| {
                    Error::into_buffer_fill_error(
                        e.into(),
                        offset + 48,
                        crate::MFT_RECORD_SIZE - 48,
                    )
                })?;
            return Ok(buffer);
        }
        // Read the rest of the data
        reader
            .take(header.total_entry_size as u64 - 48)
            .read_to_end(&mut buffer)
            .map_err(|e| {
                Error::into_buffer_fill_error(
                    e.into(),
                    offset + 48,
                    header.total_entry_size as u64 - 48,
                )
            })?;
        // Get and apply fixup
        let fix_up: Vec<u8> = buffer[header.offset_to_fixup as usize
            ..(header.offset_to_fixup + header.num_of_fixup * 2 + 2) as usize]
            .to_vec();
        for i in 1..(fix_up.len() / 2) {
            // Replace last 2 bytes of each 512 sector
            let replace_offset = i * 512 - 2;
            let fix_up_offset = i * 2;
            if replace_offset > buffer.len() {
                break;
            }
            buffer[replace_offset] = fix_up[fix_up_offset];
            buffer[replace_offset + 1] = fix_up[fix_up_offset + 1];
        }
        // Return full entry buffer
        Ok(buffer)
    }

    pub fn from_reader<R: Read + Seek>(
        reader: &mut R,
        prev_entry: Option<Self>,
    ) -> crate::Result<Self> {
        let (file_offset, entry_n) = match &prev_entry {
            Some(prev) => (
                prev.offset
                    + if prev.header.total_entry_size == 0 {
                        1024
                    } else {
                        prev.header.total_entry_size as u64
                    },
                prev.entry_n + 1,
            ),

            None => (0, 0),
        };
        // Get Entry Header to peek size of the rest of the entry
        let mut buffer: Vec<u8> = Vec::new();
        reader
            .take(48)
            .read_to_end(&mut buffer)
            .map_err(|e| Error::into_buffer_fill_error(e.into(), file_offset, 48))?;
        // Generate header from first 48 bytes
        let header = Header::from_buffer(&buffer)?;

        // println!("Header: {:?}", header);
        // println!("Offset: {:?}", file_offset);

        if header.is_zeroed() {
            // If zeroed assume default size
            reader.seek(SeekFrom::Start(file_offset + crate::MFT_RECORD_SIZE))?;
            return Ok(Self {
                offset: file_offset,
                entry_n,
                header,
                attributes: Vec::new(),
            });
        }

        // Read rest of entry to buffer for generating attributes
        reader
            .take(header.total_entry_size as u64 - 48)
            .read_to_end(&mut buffer)
            .map_err(|e| {
                Error::into_buffer_fill_error(
                    e.into(),
                    file_offset + 48,
                    header.total_entry_size as u64 - 48,
                )
            })?;

        // Get and apply fixup
        let fix_up: Vec<u8> = buffer[header.offset_to_fixup as usize
            ..(header.offset_to_fixup + header.num_of_fixup * 2 + 2) as usize]
            .to_vec();
        for i in 1..(fix_up.len() / 2) {
            // Replace last 2 bytes of each 512 sector
            let replace_offset = i * 512 - 2;
            let fix_up_offset = i * 2;
            if replace_offset > buffer.len() {
                break;
            }
            buffer[replace_offset] = fix_up[fix_up_offset];
            buffer[replace_offset + 1] = fix_up[fix_up_offset + 1];
        }

        // Get attributes
        let mut attributes: Vec<Attribute> = Vec::new();
        let mut cursor = Cursor::new(&buffer);
        let mut offset = header.attrs_offset as u64;
        cursor.seek(SeekFrom::Start(offset))?;
        // Iterate over buffer to get all attributes
        while let Some(attribute) = Attribute::from_buffer(&buffer, offset)? {
            if !Attribute::is_valid_type_code(attribute.type_code) {
                break;
            }
            offset += attribute.record_len as u64;
            cursor.seek(SeekFrom::Start(offset))?;
            attributes.push(attribute);
        }

        // Return entry
        Ok(Entry {
            // Meta
            offset: file_offset,
            entry_n,
            // Content
            header,
            attributes,
        })
    }
}

#[derive(Debug)]
pub struct Header {
    // MULTI_SECTOR_HEADER
    pub sig: [u8; 4],
    pub offset_to_fixup: u16,
    pub num_of_fixup: u16,
    //
    pub log_sequence_number: u64,
    pub sequence_number: u16,
    pub link_count: u16,
    pub attrs_offset: u16,
    pub flags: u16,
    pub used_entry_size: u32,
    pub total_entry_size: u32,
    pub base_mft_record: FileReference,
    pub next_attr_id: u16,
}

impl Header {
    pub fn is_zeroed(&self) -> bool {
        self.sig == [0, 0, 0, 0]
            && self.offset_to_fixup == 0
            && self.num_of_fixup == 0
            && self.log_sequence_number == 0
            && self.sequence_number == 0
            && self.link_count == 0
            && self.attrs_offset == 0
            && self.flags == 0
            && self.used_entry_size == 0
            // && self.total_entry_size == 0 // as we overwrite this value in case of zeroed header
            && self.base_mft_record == 0
            && self.next_attr_id == 0
    }
    pub fn from_buffer(buffer: &[u8]) -> crate::Result<Self> {
        let mut reader = BufReader::new(buffer);
        //
        let mut sig_buffer: [u8; 4] = [0; 4];
        reader
            .read_exact(&mut sig_buffer)
            .map_err(|e| Error::into_value_read_error(e.into(), "sig_buffer", "read_u8 * 4"))?;
        //
        read_value!(reader, offset_to_fixup, read_u16);
        read_value!(reader, num_of_fixup, read_u16);
        read_value!(reader, log_sequence_number, read_u64);
        read_value!(reader, sequence_number, read_u16);
        read_value!(reader, link_count, read_u16);
        read_value!(reader, attrs_offset, read_u16);
        read_value!(reader, flags, read_u16);
        read_value!(reader, used_entry_size, read_u32);
        read_value!(reader, total_entry_size, read_u32); // We overwrite total_entry_size in case of zeroed header
        read_value!(reader, record, read_u64);
        let base_mft_record = FileReference::from(record);
        read_value!(reader, next_attr_id, read_u16);
        // Consolidate errors
        Ok(Header {
            sig: sig_buffer,
            offset_to_fixup,
            num_of_fixup,
            log_sequence_number,
            sequence_number,
            link_count,
            attrs_offset,
            flags,
            used_entry_size,
            total_entry_size,
            base_mft_record,
            next_attr_id,
        })
    }
}

#[derive(Debug)]
pub struct Attribute {
    pub offset: u64,
    pub type_code: u32,
    pub record_len: u32,
    pub form_code: u8,
    pub name_len: u8,
    pub name_offset: u16,
    //
    pub name: Option<String>,
    //
    pub flags: u16,
    pub instance: u16,
    pub data: AttributeData,
}

#[derive(Debug)]
pub enum AttributeData {
    Resident {
        data_size: u32,
        data_offset: u16,
        indexed_flag: u8,
        _padding: u8,
    },
    NonResident {
        lowest_vcn: u64,
        highest_vcn: u64,
        data_run_offset: u16,
        compression_unit_size: u16,
        _padding: u32,
        allocated_size: u64,
        data_size: u64,
        initialized_size: u64,
        compressed_size: Option<u64>,
    },
}

impl Attribute {
    pub fn from_buffer(buffer: &[u8], offset: u64) -> crate::Result<Option<Self>> {
        let mut reader = Cursor::new(buffer);
        // Seek to offset
        reader.seek(SeekFrom::Start(offset))?;
        //
        read_value!(reader, type_code, read_u32);
        if !Self::is_valid_type_code(type_code) {
            return Ok(None);
        }
        read_value!(reader, record_len, read_u32);
        let form_code = reader
            .read_u8()
            .map_err(|e| Error::into_value_read_error(e.into(), "form_code", "read_u8"))?;
        let name_len = reader
            .read_u8()
            .map_err(|e| Error::into_value_read_error(e.into(), "name_len", "read_u8"))?;
        read_value!(reader, name_offset, read_u16);
        read_value!(reader, flags, read_u16);
        read_value!(reader, instance, read_u16);
        let data = match form_code {
            0x00 => {
                read_value!(reader, data_size, read_u32);
                read_value!(reader, data_offset, read_u16);
                let indexed_flag = reader.read_u8().map_err(|e| {
                    Error::into_value_read_error(e.into(), "indexed_flag", "read_u8")
                })?;
                let _padding = reader
                    .read_u8()
                    .map_err(|e| Error::into_value_read_error(e.into(), "_padding", "read_u8"))?;
                AttributeData::Resident {
                    data_size,
                    data_offset,
                    indexed_flag,
                    _padding,
                }
            }
            0x01 => {
                read_value!(reader, lowest_vcn, read_u64);
                read_value!(reader, highest_vcn, read_u64);
                read_value!(reader, data_run_offset, read_u16);
                read_value!(reader, compression_unit_size, read_u16);
                read_value!(reader, _padding, read_u32);
                read_value!(reader, allocated_size, read_u64);
                read_value!(reader, data_size, read_u64);
                read_value!(reader, initialized_size, read_u64);
                let compressed_size = match compression_unit_size > 0 {
                    true => {
                        read_value!(reader, compressed_size_raw, read_u64);
                        Some(compressed_size_raw)
                    }
                    false => None,
                };
                AttributeData::NonResident {
                    lowest_vcn,
                    highest_vcn,
                    data_run_offset,
                    compression_unit_size,
                    _padding,
                    allocated_size,
                    data_size,
                    initialized_size,
                    compressed_size,
                }
            }
            _ => unreachable!(),
        };
        // Get name
        reader.seek(SeekFrom::Start(offset + name_offset as u64))?;

        let name = if name_len > 0 {
            let mut s = String::new();
            for _ in 0..name_len {
                let c = reader
                    .read_u16::<LittleEndian>()
                    .map_err(|e| Error::into_value_read_error(e.into(), "name_char", "read_u16"))?;
                s.push(c as u8 as char);
            }
            Some(s)
        } else {
            None
        };
        //
        Ok(Some(Attribute {
            offset,
            type_code,
            record_len,
            form_code,
            name_len,
            name_offset,
            name,
            flags,
            instance,
            data,
        }))
    }

    pub fn is_valid_type_code(type_code: u32) -> bool {
        static VALID_CODES: [u32; 18] = [
            0x0, 0x00000010, 0x00000020, 0x00000030, 0x00000040, 0x00000050, 0x00000060,
            0x00000070, 0x00000080, 0x00000090, 0x000000a0, 0x000000b0, 0x000000c0, 0x000000d0,
            0x000000e0, 0x000000f0, 0x00000100, 0x00001000,
        ];
        VALID_CODES.contains(&type_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_parse() {
        let data = vec![
            0x46, 0x49, 0x4c, 0x45, 0x30, 0x00, 0x03, 0x00, 0x4a, 0xcc, 0x37, 0x0c, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, 0x0a, 0x00, 0x38, 0x00, 0x01, 0x00, 0xa0, 0x02, 0x00, 0x00,
            0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00,
            0x00, 0x00, 0xa9, 0xa2, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x10, 0x00, 0x00, 0x00, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00, 0x50, 0xaa, 0xbc, 0xa8,
            0x0d, 0xad, 0xd5, 0x01, 0x50, 0xaa, 0xbc, 0xa8, 0x0d, 0xad, 0xd5, 0x01, 0x32, 0x5f,
            0xbf, 0x58, 0x44, 0xcc, 0xd8, 0x01, 0x32, 0x5f, 0xbf, 0x58, 0x44, 0xcc, 0xd8, 0x01,
            0x20, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc4, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x08, 0x6c, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00,
            0x00, 0x00, 0x48, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0xc0, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x01,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x31, 0x01, 0x8e, 0x8d, 0x0a, 0x00, 0x00, 0x00,
            0x30, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x0d, 0x00, 0x66, 0x00, 0x00, 0x00, 0x18, 0x00, 0x01, 0x00, 0x0e, 0x01, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, 0x50, 0xaa, 0xbc, 0xa8, 0x0d, 0xad, 0xd5, 0x01, 0x50, 0xaa,
            0xbc, 0xa8, 0x0d, 0xad, 0xd5, 0x01, 0xd7, 0x7c, 0xee, 0x5d, 0x4c, 0xcc, 0xd8, 0x01,
            0x6c, 0xdc, 0xfc, 0x79, 0x04, 0xbe, 0xd6, 0x01, 0x00, 0x30, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xb8, 0x21, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x63, 0x00, 0x6c, 0x00, 0x72, 0x00, 0x63, 0x00,
            0x6f, 0x00, 0x6d, 0x00, 0x70, 0x00, 0x72, 0x00, 0x65, 0x00, 0x73, 0x00, 0x73, 0x00,
            0x69, 0x00, 0x6f, 0x00, 0x6e, 0x00, 0x2e, 0x00, 0x64, 0x00, 0x6c, 0x00, 0x6c, 0x00,
            0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x0b, 0x00, 0x66, 0x00, 0x00, 0x00, 0x18, 0x00, 0x01, 0x00, 0x1d, 0x01,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x50, 0xaa, 0xbc, 0xa8, 0x0d, 0xad, 0xd5, 0x01,
            0x50, 0xaa, 0xbc, 0xa8, 0x0d, 0xad, 0xd5, 0x01, 0xd7, 0x7c, 0xee, 0x5d, 0x4c, 0xcc,
            0xd8, 0x01, 0x6c, 0xdc, 0xfc, 0x79, 0x04, 0xbe, 0xd6, 0x01, 0x00, 0x30, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0xb8, 0x21, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x63, 0x00, 0x6c, 0x00, 0x72, 0x00,
            0x63, 0x00, 0x6f, 0x00, 0x6d, 0x00, 0x70, 0x00, 0x72, 0x00, 0x65, 0x00, 0x73, 0x00,
            0x73, 0x00, 0x69, 0x00, 0x6f, 0x00, 0x6e, 0x00, 0x2e, 0x00, 0x64, 0x00, 0x6c, 0x00,
            0x6c, 0x00, 0x00, 0x00, 0xd0, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x08, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00,
            0x6f, 0x00, 0x00, 0x00, 0x7c, 0x00, 0x05, 0x00, 0xe0, 0x00, 0x00, 0x00, 0x98, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x7c, 0x00, 0x00, 0x00,
            0x18, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x16, 0x1e, 0x00, 0x24, 0x4b,
            0x45, 0x52, 0x4e, 0x45, 0x4c, 0x2e, 0x50, 0x55, 0x52, 0x47, 0x45, 0x2e, 0x45, 0x53,
            0x42, 0x43, 0x41, 0x43, 0x48, 0x45, 0x00, 0x1e, 0x00, 0x00, 0x00, 0x03, 0x00, 0x02,
            0x06, 0x78, 0x4c, 0x24, 0x47, 0x44, 0xcc, 0xd8, 0x01, 0x80, 0x66, 0x42, 0xa5, 0x70,
            0x73, 0xd3, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3c, 0x00,
            0x00, 0x00, 0x00, 0x19, 0x18, 0x00, 0x24, 0x4b, 0x45, 0x52, 0x4e, 0x45, 0x4c, 0x2e,
            0x50, 0x55, 0x52, 0x47, 0x45, 0x2e, 0x41, 0x50, 0x50, 0x58, 0x46, 0x49, 0x43, 0x41,
            0x43, 0x48, 0x45, 0x00, 0x78, 0x4c, 0x24, 0x47, 0x44, 0xcc, 0xd8, 0x01, 0x08, 0x6c,
            0x1d, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x4c, 0xcc, 0xd8, 0x01, 0xff, 0xff, 0xff, 0xff, 0x82, 0x79, 0x47, 0x11,
            0x00, 0x30, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0xb8, 0x21, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x63, 0x00,
            0x6c, 0x00, 0x72, 0x00, 0x63, 0x00, 0x6f, 0x00, 0x6d, 0x00, 0x70, 0x00, 0x72, 0x00,
            0x65, 0x00, 0x73, 0x00, 0x73, 0x00, 0x69, 0x00, 0x6f, 0x00, 0x6e, 0x00, 0x2e, 0x00,
            0x64, 0x00, 0x6c, 0x00, 0x6c, 0x00, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x80, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0e, 0x00, 0x66, 0x00, 0x00, 0x00,
            0x18, 0x00, 0x01, 0x00, 0xd5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x50, 0xaa,
            0xbc, 0xa8, 0x0d, 0xad, 0xd5, 0x01, 0x50, 0xaa, 0xbc, 0xa8, 0x0d, 0xad, 0xd5, 0x01,
            0xd7, 0x7c, 0xee, 0x5d, 0x4c, 0xcc, 0xd8, 0x01, 0x6c, 0xdc, 0xfc, 0x79, 0x04, 0xbe,
            0xd6, 0x01, 0x00, 0x30, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0xb8, 0x21, 0x01, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00,
            0x63, 0x00, 0x6c, 0x00, 0x72, 0x00, 0x63, 0x00, 0x6f, 0x00, 0x6d, 0x00, 0x70, 0x00,
            0x72, 0x00, 0x65, 0x00, 0x73, 0x00, 0x73, 0x00, 0x69, 0x00, 0x6f, 0x00, 0x6e, 0x00,
            0x2e, 0x00, 0x64, 0x00, 0x6c, 0x00, 0x6c, 0x00, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00,
            0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x66, 0x00,
            0x00, 0x00, 0x18, 0x00, 0x01, 0x00, 0xd5, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x50, 0xaa, 0xbc, 0xa8, 0x0d, 0xad, 0xd5, 0x01, 0x50, 0xaa, 0xbc, 0xa8, 0x0d, 0xad,
            0xd5, 0x01, 0xd7, 0x7c, 0xee, 0x5d, 0x4c, 0xcc, 0xd8, 0x01, 0x6c, 0xdc, 0xfc, 0x79,
            0x04, 0xbe, 0xd6, 0x01, 0x00, 0x30, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0xb8, 0x21,
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x12, 0x00, 0x63, 0x00, 0x6c, 0x00, 0x72, 0x00, 0x63, 0x00, 0x6f, 0x00, 0x6d, 0x00,
            0x70, 0x00, 0x72, 0x00, 0x65, 0x00, 0x73, 0x00, 0x73, 0x00, 0x69, 0x00, 0x6f, 0x00,
            0x6e, 0x00, 0x2e, 0x00, 0x64, 0x00, 0x6c, 0x00, 0x6c, 0x00, 0x00, 0x00, 0xff, 0xff,
            0xff, 0xff, 0x82, 0x79, 0x47, 0x11, 0x64, 0x00, 0x6c, 0x00, 0x6c, 0x00, 0x00, 0x00,
            0xff, 0xff, 0xff, 0xff, 0x82, 0x79, 0x47, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x05, 0x00,
        ];
        // Same data as Ox hex

        println!("Parsing {} bytes to entry", data.len());
        //
        let mut reader = Cursor::new(&data[..]);
        let entry = Entry::from_reader(&mut reader, None);
        println!("Parsed entry: {:#x?}", entry);
        assert!(entry.is_ok());
    }
}
