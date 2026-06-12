use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serial port error: {0}")]
    Serial(#[from] serialport::Error),

    #[error("Transport not connected")]
    NotConnected,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Script error: {0}")]
    Script(String),
}

pub type Result<T> = std::result::Result<T, Error>;
