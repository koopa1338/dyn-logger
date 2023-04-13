use std::io;
use thiserror::Error;

pub(crate) const TARGET_PARSE_ERROR_MSG: &str = "Error parsing modules, this should never happen";

#[derive(Error, Debug)]
pub enum DynLogAPIErr {
    #[error("Failed to read file: {filename}")]
    FileReadError { filename: String, source: io::Error },
    #[error("Failed to create directory: {path}")]
    CreateLogDirError { path: String, source: io::Error },
    #[error("Failed to deserialize toml")]
    TomlDeserializeError(#[from] toml::de::Error),
    #[error("Error parsing file logger table, there were no entries found.")]
    InitializeFileloggerError,
}
