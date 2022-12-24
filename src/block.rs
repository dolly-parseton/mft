use std::io::{Read, Seek};

#[derive(Debug, Clone)]
pub struct Block {
    pub blocks: Vec<SectionPointer>,
    pub entry_id: u64,
}

impl Block {
    pub fn new_with_entry<R: Read + Seek>(
        _reader: &mut R,
        entry: &crate::raw::Entry,
        record_n: u64,
    ) -> crate::Result<Self> {
        trace!("Creating Block from Entry");
        let mut blocks = vec![SectionPointer {
            block_type: BlockType::Entry,
            is_resident: true,
            attribute_id: None,
            offset: entry.offset,
            size: entry.header.total_entry_size as u64,
        }];

        // Create Attributes Blocks
        for attribute in &entry.attributes {
            use crate::raw::AttributeData;
            // Get offsets based on if the attribute is resident or not
            let data_offset = entry.offset
                + attribute.offset as u64
                + match attribute.data {
                    AttributeData::Resident {
                        data_offset: offset,
                        ..
                    } => offset as u64,
                    AttributeData::NonResident { .. } => 16,
                };
            let data_size = match attribute.data {
                AttributeData::Resident {
                    data_size: size, ..
                } => size as u64,
                AttributeData::NonResident {
                    data_size: size, ..
                } => size as u64,
            };
            let is_resident = match attribute.data {
                AttributeData::Resident { .. } => true,
                AttributeData::NonResident { .. } => false,
            };
            trace!(
                "Creating SectionPointer for record {} of type {:?}",
                record_n,
                BlockType::from_attribute_type_code(attribute.type_code)
            );
            blocks.push(SectionPointer {
                block_type: BlockType::from_attribute_type_code(attribute.type_code),
                is_resident,
                attribute_id: Some(attribute.instance),
                offset: data_offset,
                size: data_size,
            });
            // Zone Identifier checks
            if BlockType::from_attribute_type_code(attribute.type_code) == BlockType::Data {
                if let Some(name) = &attribute.name {
                    if name == "Zone.Identifier" {
                        trace!(
                            "Creating ZoneIdentifier SectionPointer for record {}",
                            record_n
                        );
                        blocks.push(SectionPointer {
                            block_type: BlockType::ZoneIdentifier,
                            is_resident,
                            attribute_id: None,
                            offset: data_offset,
                            size: data_size,
                        });
                    }
                }
            }
        }
        //
        Ok(Self {
            blocks,
            entry_id: record_n,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SectionPointer {
    pub block_type: BlockType,
    pub is_resident: bool,
    pub attribute_id: Option<u16>,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, PartialEq, Clone)]
pub enum BlockType {
    // Top Level
    Entry,
    // Attribute Types
    StandardInformation, // 0x10
    AttributeList,       // 0x20
    FileName,            // 0x30
    ObjectId,            // 0x40
    SecurityDescriptor,  // 0x50
    VolumeName,          // 0x60
    VolumeInformation,   // 0x70
    Data,                // 0x80
    IndexRoot,           // 0x90
    IndexAllocation,     // 0xA0
    Bitmap,              // 0xB0
    ReparsePoint,        // 0xC0
    EaInformation,       // 0xD0
    Ea,                  // 0xE0
    PropertySet,         // 0xF0
    LoggedUtilityStream, // 0x100
    End,                 // 0xFFFFFFFF
    UnknownAttribute,    // _
    // Zone Identifier tag, in Data Attribute when resident and name = "Zone.Identifier"
    ZoneIdentifier,
}

impl BlockType {
    pub fn from_attribute_type_code(type_code: u32) -> Self {
        match type_code {
            0x10 => BlockType::StandardInformation,
            0x20 => BlockType::AttributeList, // Resolve on block creation?
            0x30 => BlockType::FileName,
            0x40 => BlockType::ObjectId,
            0x50 => BlockType::SecurityDescriptor,
            0x60 => BlockType::VolumeName,
            0x70 => BlockType::VolumeInformation,
            0x80 => BlockType::Data,
            0x90 => BlockType::IndexRoot,
            0xA0 => BlockType::IndexAllocation,
            0xB0 => BlockType::Bitmap,
            0xC0 => BlockType::ReparsePoint,
            0xD0 => BlockType::EaInformation,
            0xE0 => BlockType::Ea,
            0xF0 => BlockType::PropertySet,
            0x100 => BlockType::LoggedUtilityStream,
            0xFFFFFFFF => BlockType::End,
            _ => BlockType::UnknownAttribute,
        }
    }
}
