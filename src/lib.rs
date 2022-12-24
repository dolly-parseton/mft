mod block;
mod error;
#[macro_use]
mod raw;
mod attributes;
mod iter;

#[macro_use]
extern crate serde;
#[macro_use]
extern crate log;

use block::BlockType;

use crate::block::{Block, SectionPointer};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, error::Error>;
pub use crate::error::Error;
pub use iter::Iterator;

pub const MFT_RECORD_SIZE: u64 = 1024;

#[derive(Debug)]
// Iterates over the MFT file and returns sizes and offsets for useful data by entry
pub struct Parser {
    pub reader: BufReader<File>,
    pub size: u64,
    pub records: u64,
    pub blocks: Vec<Block>,
    pub path_parts: HashMap<u64, Option<(String, u64)>>, // Entry ID and (Path Part, Entry)
    //
    pub settings: ParserSettings,
}

impl Parser {
    pub fn new<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        Self::with_settings(path, ParserSettings::default())
    }

    pub fn with_settings<P: AsRef<Path>>(path: P, settings: ParserSettings) -> crate::Result<Self> {
        trace!(
            "Creating MftParser struct from {} ({} drive)",
            path.as_ref().display(),
            settings
                .drive_char
                .map(|c| c.to_string())
                .unwrap_or_else(|| String::from("Unknown"))
        );
        // Get reader
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        // Get size
        let size = reader.get_ref().metadata()?.len();
        // Get records
        let records = size / MFT_RECORD_SIZE;
        // Get Blocks
        let blocks = Self::get_blocks(&mut reader, records)?;
        // Return
        trace!("Returning MftParser parser struct");
        Ok(Self {
            reader,
            size,
            records,
            blocks,
            path_parts: HashMap::new(),
            settings,
        })
    }

    fn get_blocks<R: Read + Seek>(reader: &mut R, records: u64) -> crate::Result<Vec<Block>> {
        trace!("Getting blocks from MFT file ({} records)", records);
        let mut prev = None;
        let mut blocks = Vec::new();
        for record_n in 0..(records) {
            let entry: raw::Entry = raw::Entry::from_reader(reader, prev)?;
            let block = block::Block::new_with_entry(reader, &entry, record_n)?;
            blocks.push(block);
            prev = Some(entry);
        }
        Ok(blocks)
    }

    // Debug function for caching all path parts
    pub fn fill_path_parts_cache(&mut self) -> crate::Result<()> {
        for i in 0..self.blocks.len() {
            let entry_id = self.blocks.get(i).map(|b| b.entry_id);
            if let Some(id) = entry_id {
                let filename_attribute = match self.get_best_path_part(id) {
                    Ok(f) => Some(f),
                    Err(Error::MissingFileNameAttribute) => None,
                    Err(e) => return Err(e),
                };
                self.path_parts.insert(
                    id,
                    filename_attribute.map(|f| (f.name, f.parent_file_reference.entry)),
                );
            }
        }
        Ok(())
    }

    pub fn get_file_path(&mut self, entry_id: u64) -> crate::Result<PathBuf> {
        trace!("Getting path for entry {}", entry_id);
        let mut parts = Vec::new();
        let mut current_id = entry_id;
        loop {
            match self.path_parts.get(&current_id) {
                // 5 is a reserved reference for the root of the filesystem
                Some(Some((name, 5))) => {
                    parts.push(name.clone());
                    if let Some(drive) = self.settings.drive_char {
                        parts.push(format!("{}:", drive));
                    } else {
                        parts.push("{Root}".to_string());
                    }
                    break;
                }
                Some(Some((name, parent_id))) => {
                    parts.push(name.clone());
                    if current_id == *parent_id || *parent_id == 0 {
                        parts.push("{Orphaned}".to_string());
                        break;
                    }
                    current_id = *parent_id;
                }
                // If part of the path for this entry has not yet been resolved and cached, get the best one
                _ => match self.get_best_path_part(current_id) {
                    Ok(f) => {
                        self.path_parts.insert(
                            current_id,
                            Some((f.name.clone(), f.parent_file_reference.entry)),
                        );
                    }
                    Err(Error::MissingFileNameAttribute) => {
                        break;
                    }
                    Err(e) => return Err(e),
                },
            }
        }
        Ok(PathBuf::from(
            parts.into_iter().rev().collect::<Vec<String>>().join("/"),
        ))
    }

    pub fn get_best_path_part(&mut self, entry_id: u64) -> crate::Result<attributes::FileName> {
        fn recurse_attributes<R: Read + Seek>(
            file_reader: &mut R,
            target_block: &Block,
            target_attribute: Option<SectionPointer>,
            blocks: &[Block],
        ) -> crate::Result<attributes::FileName> {
            // Grab entry block, there will be only one per block
            let entry_block = target_block
                .blocks
                .iter()
                .find(|block| block.block_type == BlockType::Entry)
                .ok_or_else(|| crate::Error::missing_block("EntryBlock", target_block.entry_id))?;
            let entry_bytes = crate::raw::Entry::get_entry_bytes(file_reader, entry_block.offset)?;
            let mut block_reader = std::io::Cursor::new(entry_bytes);
            // Get all relevant attribute blocks (FileName and AttributeList)
            let attribute_blocks = target_block
                .blocks
                .iter()
                .filter(|b| match target_attribute {
                    Some(ref a) => {
                        b.attribute_id == a.attribute_id
                            && (b.block_type == BlockType::FileName
                                || b.block_type == BlockType::AttributeList)
                    }
                    None => {
                        b.block_type == BlockType::FileName
                            || b.block_type == BlockType::AttributeList
                    }
                });
            // Iterate over attribute blocks and find the best filename
            let mut filename_opt = None;
            'outer: for block in attribute_blocks {
                block_reader.seek(SeekFrom::Start(block.offset - entry_block.offset))?;
                match block.block_type {
                    BlockType::FileName => {
                        // Seek relative offset
                        let filename_attribute =
                            attributes::FileName::from_reader(&mut block_reader)?;
                        if filename_attribute.name_space != 2 {
                            filename_opt = Some(filename_attribute);
                            break 'outer;
                        }
                    }
                    BlockType::AttributeList => {
                        for (resolved_entry_id, resolved_attribute) in
                            attributes::AttributeList::from_reader(&mut block_reader, block.size)?
                                .resolve_to_blocks(blocks)
                        {
                            let resolved_entry = blocks
                                .iter()
                                .find(|b| b.entry_id == resolved_entry_id)
                                .ok_or_else(|| {
                                    crate::Error::missing_block(
                                        "AttributePointer",
                                        target_block.entry_id,
                                    )
                                })?;
                            // Recurse
                            let attribute_opt = recurse_attributes(
                                file_reader,
                                resolved_entry,
                                Some(resolved_attribute),
                                blocks,
                            )
                            .ok();
                            if attribute_opt.is_some() {
                                filename_opt = attribute_opt;
                                break 'outer;
                            }
                        }
                    }
                    _ => (),
                };
            }
            match filename_opt {
                Some(filename) => Ok(filename),
                None => Err(crate::Error::MissingFileNameAttribute),
            }
        }
        trace!("Getting best path part for entry {}", entry_id);
        let target_block = self
            .blocks
            .iter()
            .find(|b| b.entry_id == entry_id)
            .ok_or_else(|| crate::Error::missing_block("Block", entry_id))?;
        recurse_attributes(&mut self.reader, target_block, None, &self.blocks)
    }
}

#[derive(Debug, Default)]
pub struct ParserSettings {
    pub drive_char: Option<char>,
    pub path_exclusion_regex: Option<regex::Regex>,
    pub filename_exclusion_regex: Option<regex::Regex>,
}

impl ParserSettings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn drive_char(mut self, drive_char: char) -> Self {
        self.drive_char = Some(drive_char);
        self
    }

    pub fn path_exclusion_regex(mut self, regex: &str) -> Self {
        self.path_exclusion_regex = Some(
            regex::Regex::new(regex).expect("Unable to parse regex provided for path exclusions"),
        );
        self
    }

    pub fn filename_exclusion_regex(mut self, regex: &str) -> Self {
        self.filename_exclusion_regex = Some(
            regex::Regex::new(regex)
                .expect("Unable to parse regex provided for filename exclusions"),
        );
        self
    }
}

#[cfg(test)]
mod iterator_tests {
    use super::Parser;
    use std::{path::PathBuf, str::FromStr};
    #[test]
    fn create_iterator() {
        let path = PathBuf::from_str("./.test_data/mft.mft").unwrap();
        println!("{:#?}", path);
        let mut parser = Parser::from_path(path, Some('C')).unwrap();
        for i in 0..parser.records {
            let _ = parser.get_file_path(i).unwrap();
        }
    }
}
