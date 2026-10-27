use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("DBus Error")]
    DBusError(#[from] zbus::Error),

    #[error("DBUS FDO Error")]
    FDOError(#[from] zbus::fdo::Error),

    #[error("Unknown power profile: '{0}'")]
    ParseError(String),

    #[error("None of the power profiles can be set")]
    NoProfileAvailable,
}

pub type Result<T> = std::result::Result<T, Error>;
