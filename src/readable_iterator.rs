use chrono::{DateTime, Utc};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::PathBuf;

use crate::attributes::StandardInformation;
use crate::block::{Block, BlockType};
use crate::MftParser;

#[derive(Debug, Clone, Serialize)]
pub struct Record {
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

impl Record {
    pub fn from(parser: &mut MftParser, block: &Block) -> crate::Result<Self> {
        //
        let path = parser.get_file_path(block.entry_id)?;
        let filename = path.file_name().map(|f| f.to_string_lossy().to_string());
        //
        let standard_info_block = block
            .blocks
            .iter()
            .find(|b| BlockType::StandardInformation == b.block_type)
            .ok_or(crate::Error::MissingBlock)?;
        parser
            .reader
            .seek(SeekFrom::Start(standard_info_block.offset))?;
        let standard_info = StandardInformation::from_reader(&mut parser.reader)?;
        let is_file = standard_info.file_attributes != 0x00000010;
        let created = standard_info.creation_time;
        let modified = standard_info.modification_time;
        let accessed = standard_info.access_time;
        //
        let entry_block = block
            .blocks
            .iter()
            .find(|b| BlockType::Entry == b.block_type)
            .ok_or(crate::Error::MissingBlock)?;
        parser.reader.seek(SeekFrom::Start(entry_block.offset))?;
        let entry_header = crate::raw::Header::from_reader(&mut parser.reader)?;
        let is_deleted = entry_header.flags.to_le_bytes().contains(&0x02);
        //
        Ok(Self {
            path,
            is_file,
            is_deleted,
            filename,
            created,
            modified,
            accessed,
        })
    }

    pub fn as_csv(&self, stdout: &std::io::Stdout) -> std::io::Result<()> {
        //
        unimplemented!();
        Ok(())
    }
}

pub struct Iterator {
    pub inner: crate::MftParser,
    pub next_entry_id: u64,
}

impl From<MftParser> for Iterator {
    fn from(parser: MftParser) -> Self {
        Self {
            inner: parser,
            next_entry_id: 0,
        }
    }
}

impl std::iter::Iterator for Iterator {
    type Item = crate::Result<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        // Get next entry
        if let Some(block) = self.inner.blocks.get(self.next_entry_id as usize).cloned() {
            self.next_entry_id += 1;
            Some(Record::from(&mut self.inner, &block))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_simple_record() {
        let path = PathBuf::from_str("./.test_data/mft.mft").unwrap();
        println!("{:#?}", path);
        let parser = MftParser::from_path(path).unwrap();
        let mut iterator = Iterator::from(parser);
        println!("{:#?}", iterator.inner.records);
        for _ in 0..41740 {
            let record = iterator.next().unwrap();
            println!("{:#?}", record);
        }
        // let record =
        //     Record::from_reader(&mut iterator.inner.reader, &block, &iterator.inner.blocks)
        //         .unwrap();
        // println!("{:#?}", record);
    }
}
