use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("FDO Error")]
    FDOError(#[from] zbus::fdo::Error),

    #[error("ZBus Error")]
    ZbusError(#[from] zbus::Error),

    #[error("Failed to parse action list")]
    ActionParseError(Vec<String>),
}

pub type Result<T> = std::result::Result<T, Error>;
