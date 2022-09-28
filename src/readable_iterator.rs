use chrono::{DateTime, Utc};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::block::{Block, BlockType};
use crate::MftParser;

// Record Structs

#[derive(Debug, Clone)]
pub struct SimpleRecord {
    pub path: PathBuf,
    pub is_file: bool,
    pub is_deleted: bool,
    pub filename: Option<String>,
    //
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub accessed: DateTime<Utc>,
    //
}

impl SimpleRecord {
    pub fn from_reader<R: Read + Seek>(
        file_reader: &mut R,
        target_block: &Block,
        _blocks: &[Block],
    ) -> crate::Result<Self> {
        use crate::attributes::StandardInformation;
        // Reader to pull data from MFT as needed
        // Block to target specific entry
        // Blocks todo lookup for full path on parent reference

        // Read whole entry and apply fixups.
        let entry_block = target_block
            .blocks
            .iter()
            .find(|b| b.block_type == BlockType::Entry)
            .ok_or(crate::Error::MissingBlock)?;
        let entry_bytes = crate::raw::Entry::get_entry_bytes(file_reader, entry_block.offset)?;

        // Define the entry reader - based on entry bytes (fixups applied)
        let mut reader = std::io::Cursor::new(entry_bytes);

        // Get Standard Info Attributes (from relative offset)
        let standard_info_block = target_block
            .blocks
            .iter()
            .find(|b| b.block_type == BlockType::StandardInformation)
            .ok_or(crate::Error::MissingBlock)?;
        reader.seek(SeekFrom::Start(
            standard_info_block.offset - entry_block.offset,
        ))?;
        let standard_info = StandardInformation::from_reader(&mut reader)?;
        println!("{:#?}", standard_info);

        let path = PathBuf::new();

        // Get path
        Ok(Self {
            path,
            is_file: false,
            is_deleted: false,
            filename: None,
            created: standard_info.creation_time,
            modified: standard_info.modification_time,
            accessed: standard_info.access_time,
        })
    }
}

pub struct MftIterator {
    pub inner: crate::MftParser,
    pub next_entry_id: u64,
    //
    // pub file_path_parts: HashMap<u64, String>, // K=Entry Id, V=Best Match Path Part
}

impl From<MftParser> for MftIterator {
    fn from(parser: MftParser) -> Self {
        Self {
            inner: parser,
            next_entry_id: 0,
        }
    }
}

impl Iterator for MftIterator {
    type Item = Block;

    fn next(&mut self) -> Option<Self::Item> {
        // Get next entry
        let entry: Option<Block> = self.inner.blocks.get(self.next_entry_id as usize).cloned();
        if entry.is_some() {
            self.next_entry_id += 1;
        }
        entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::BufReader;
    use std::str::FromStr;

    #[test]
    fn test_simple_record() {
        let path = PathBuf::from_str("./.test_data/mft.mft").unwrap();
        println!("{:#?}", path);
        let mut parser = MftParser::from_path(path).unwrap();
        let mut iterator = MftIterator::from(parser);
        println!("{:#?}", iterator.inner.records);
        for _ in 0..41740 {
            let _ = iterator.next();
        }
        let block = iterator.next().unwrap();
        let record =
            SimpleRecord::from_reader(&mut iterator.inner.reader, &block, &iterator.inner.blocks)
                .unwrap();
        println!("{:#?}", record);
    }
}
