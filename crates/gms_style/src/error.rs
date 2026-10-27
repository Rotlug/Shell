use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("The HOME environment variable is not set")]
    NoHome,

    #[error("Failed to read or write styles: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to run sass, is it installed? ({0})")]
    SassNotRun(std::io::Error),

    #[error("sass failed:\n{0}")]
    Sass(String),

    #[error("Failed to watch the styles: {0}")]
    Watch(#[from] notify::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
