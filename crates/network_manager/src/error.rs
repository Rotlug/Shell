use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("DBus Error")]
    DBusError(#[from] zbus::Error),

    #[error("DBUS FDO Error")]
    FDOError(#[from] zbus::fdo::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
