use chrono::{DateTime, Utc};
use std::io::{Seek, SeekFrom};
use std::path::PathBuf;

use crate::attributes::StandardInformation;
use crate::block::{Block, BlockType};
use crate::Parser;

#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub entry_id: u64,
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
    pub fn from(parser: &mut Parser, block: &Block) -> crate::Result<Self> {
        //
        let path = parser.get_file_path(block.entry_id)?;
        let filename = path.file_name().map(|f| f.to_string_lossy().to_string());
        //
        let standard_info_block = block
            .blocks
            .iter()
            .find(|b| BlockType::StandardInformation == b.block_type)
            .ok_or_else(|| crate::Error::missing_block("StandardInfo", block.entry_id))?;
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
            .ok_or_else(|| crate::Error::missing_block("EntryBlock", block.entry_id))?;
        parser.reader.seek(SeekFrom::Start(entry_block.offset))?;
        let entry_header = crate::raw::Header::from_reader(&mut parser.reader)?;
        let is_deleted = entry_header.flags.to_le_bytes().contains(&0x02);
        //
        Ok(Self {
            entry_id: block.entry_id,
            path,
            is_file,
            is_deleted,
            filename,
            created,
            modified,
            accessed,
        })
    }
}

pub struct Iterator {
    pub inner: crate::Parser,
    pub next_entry_id: u64,
    output_type: OutputType,
}

enum OutputType {
    Csv,
    Json,
}

impl OutputType {
    pub fn as_type(&self, record: Record) -> String {
        match self {
            OutputType::Csv => {
                // Headers = "entry_id,path,is_file,is_deleted,filename,created,modified,accessed"
                let csv = format!(
                    "{},\"{}\",{},{},\"{}\",\"{}\",\"{}\",\"{}\"",
                    record.entry_id,
                    record.path.to_str().unwrap(),
                    record.is_file,
                    record.is_deleted,
                    record.filename.unwrap_or_default(),
                    record.created.to_rfc3339(),
                    record.modified.to_rfc3339(),
                    record.accessed.to_rfc3339(),
                );
                csv
            }
            OutputType::Json => {
                // todo!("Handle this unwrap gracefully");
                serde_json::to_string(&record).expect("An error occured whilst serializing record")
            }
        }
    }
}

impl From<Parser> for Iterator {
    fn from(parser: Parser) -> Self {
        Self {
            inner: parser,
            next_entry_id: 0,
            output_type: OutputType::Csv,
        }
    }
}

impl std::iter::Iterator for Iterator {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        // Loop to get blocks, allows for exclusion skips without next() recursion which causes stack overflows
        while let Some(block) = self.inner.blocks.get(self.next_entry_id as usize).cloned() {
            self.next_entry_id += 1;
            let record = Record::from(&mut self.inner, &block);
            let mut to_skip = false;
            match record {
                // Warn if record is Err and get next
                Err(e) => {
                    warn!(
                        "Record {} not generated from Block data with error: {}",
                        self.next_entry_id - 1,
                        e
                    );
                }
                // Exclude if exclusions regex match
                Ok(r) => {
                    if let Some(path_exclusion_regex) = &self.inner.settings.path_exclusion_regex {
                        to_skip =
                            r.path.to_str().map(|s| path_exclusion_regex.is_match(s)) == Some(true)
                    }
                    if let Some(filename_exclusion_regex) =
                        &self.inner.settings.filename_exclusion_regex
                    {
                        to_skip = r
                            .filename
                            .as_ref()
                            .map(|s| filename_exclusion_regex.is_match(s))
                            == Some(true)
                    }
                    if !to_skip {
                        return Some(self.output_type.as_type(r));
                    }
                }
            }
        }
        None
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
        let parser = Parser::from_path(path, None).unwrap();
        let mut iterator = Iterator::from(parser);
        println!("{:#?}", iterator.inner.records);
        for _ in 0..41740 {
            let record = iterator.next().unwrap();
            println!("{:#?}", record);
        }
    }
}
