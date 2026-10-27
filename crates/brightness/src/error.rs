use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to start a thread that handles the monitors")]
    Thread(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
