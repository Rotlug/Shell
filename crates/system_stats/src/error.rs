use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to start the thread that reads the stats")]
    Thread(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
