use std::io;
use thiserror::Error;
use tracing_subscriber::filter::ParseError;

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
    #[error("Error parsing targets, there may be an issue with the declared modules.")]
    TargetParseError(#[from] ParseError),
}
