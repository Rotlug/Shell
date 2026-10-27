use std::path::PathBuf;

use crate::{Error, Result};

const NAME: &str = "gray-meadows";

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or(Error::NoHome)
}

/// Where the stylesheet lives, and where it can be edited: `~/.config/gray-meadows/styles`
///
/// # Errors
/// Errors when `HOME` isn't set
pub fn styles_directory() -> Result<PathBuf> {
    Ok(home()?.join(".config").join(NAME).join("styles"))
}

/// Where compiled stylesheets are kept: `~/.cache/gray-meadows`
pub fn cache_directory() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home()?.join(".cache"),
    };

    Ok(base.join(NAME))
}
