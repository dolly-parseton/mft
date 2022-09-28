use std::error::Error as ErrTrait;
use std::fmt;
use std::io::Error as IoError;

#[derive(Debug)]
pub enum Error {
    Reader(IoError),
    ValueRead(String),
    BufferFill(String),
    MissingBlock,
    MissingFileNameAttribute,
}

impl Error {
    pub fn into_value_read_error(self, value_name: &str, value_type: &str) -> Self {
        match self {
            Error::Reader(e) => Error::ValueRead(format!(
                "Reader error occured trying to get value {} as {}. {}",
                value_name, value_type, e
            )),
            _ => unreachable!(),
        }
    }
    pub fn into_buffer_fill_error(self, offset: u64, size: u64) -> Self {
        match self {
            Error::Reader(e) => Error::BufferFill(format!(
                "Buffer could not be filled with {} at offset {}. {}",
                size, offset, e
            )),
            _ => unreachable!(),
        }
    }
}

impl ErrTrait for Error {}

impl From<IoError> for Error {
    fn from(error: IoError) -> Self {
        Error::Reader(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Reader(error) => write!(f, "Reader error: {}", error),
            Error::ValueRead(error) => write!(f, "Value read error: {}", error),
            Error::BufferFill(error) => write!(f, "Buffer fill error: {}", error),
            Error::MissingBlock => write!(f, "Missing block"),
            Error::MissingFileNameAttribute => write!(f, "Missing file name attribute"),
        }
    }
}
