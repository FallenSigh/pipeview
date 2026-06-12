use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Session error: {0}")]
    Session(String),
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
