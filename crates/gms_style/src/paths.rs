use std::path::PathBuf;

use crate::{Error, Result};

const NAME: &str = "gray-meadows";

/// Where compiled stylesheets are kept: `~/.cache/gray-meadows`
pub fn cache_directory() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => std::env::var_os("HOME").map(PathBuf::from).ok_or(Error::NoHome)?.join(".cache"),
    };

    Ok(base.join(NAME))
}
