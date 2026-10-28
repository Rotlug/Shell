//! The look of everything Gray Meadows, see the README.
use std::{
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    path::Path,
    process::Command,
    sync::{LazyLock, Mutex},
};

use color::{
    is_valid_hex_color,
    models::{ColorModel as _, Rgba},
};
use regex::Regex;

include!(concat!(env!("OUT_DIR"), "/default_styles.rs"));

mod environment;
mod error;
mod paths;

pub use error::{Error, Result};

/// The stylesheet that holds the colors and fonts, which the environment replaces
const USER_STYLES: &str = "_user.scss";

const VAR_REGEX: &str = r#"^\$([a-zA-Z0-9_-]+):\s*(?:"?([^"]+)"?|([a-zA-Z0-9#() ,.-])+);$"#;

/// The colors defined in `_user.scss`, by name
static COLORS: LazyLock<Mutex<HashMap<String, Rgba>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A color of the stylesheet (e.g. `foreground-color-primary`), as of the last `compile`.
///
/// # Panics
/// Panics if another thread panicked while using the colors.
#[must_use]
pub fn get_color(name: &str) -> Option<Rgba> {
    COLORS.lock().unwrap().get(name).copied()
}

fn refresh_colors(user_styles: &str) {
    let regex = Regex::new(VAR_REGEX).unwrap();
    let mut colors = HashMap::new();

    for line in user_styles.lines() {
        if let Some(caps) = regex.captures(line)
            && is_valid_hex_color(&caps[2])
        {
            colors.insert(caps[1].to_string(), Rgba::from_hex(&caps[2]));
        }
    }

    *COLORS.lock().unwrap() = colors;
}

/// Compiles a stylesheet into CSS for GTK.
///
/// The stylesheet is part of the program, the only things that change it are the `GMS_*`
/// environment variables (see the README).
///
/// `entry` is the file that is compiled, without `.scss`: the shell has `main`, every app its
/// own. That way, they only carry the styles they use.
///
/// This is quick when nothing changed since last time, the CSS is cached then.
///
/// # Errors
/// Errors when the CSS can't be built, or when `sass` fails (or isn't there).
pub fn compile(entry: &str) -> Result<String> {
    let user_styles = environment::user_styles();
    refresh_colors(&user_styles);

    let cache = paths::cache_directory()?.join(format!("style-{entry}-{:016x}.css", hash_sources(&user_styles)));

    if let Ok(css) = fs::read_to_string(&cache) {
        return Ok(css);
    }

    let css = run_sass(&user_styles, entry)?;

    if let Err(error) = store_in_cache(&cache, entry, &css) {
        // Nothing wrong with the CSS, it's just not going to be quick the next time
        tracing::warn!(%error, "Failed to cache the compiled styles");
    }

    Ok(css)
}

/// `sass` reads files, so the stylesheet is put in a directory of its own for it for as long
/// as it takes.
fn run_sass(user_styles: &str, entry: &str) -> Result<String> {
    let dir = paths::cache_directory()?.join(format!("build-{}", std::process::id()));
    let result = write_sources(&dir, user_styles).and_then(|()| sass(&dir, entry));

    // Nothing in there is of any use afterwards
    let _ = fs::remove_dir_all(&dir);

    result
}

fn write_sources(dir: &Path, user_styles: &str) -> Result<()> {
    for (rel_path, content) in DEFAULT_STYLES {
        let path = dir.join(rel_path);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&path, if *rel_path == USER_STYLES { user_styles } else { content })?;
    }

    Ok(())
}

fn sass(styles: &Path, entry: &str) -> Result<String> {
    let output = Command::new("sass")
        .arg("-I")
        .arg(styles)
        .arg(styles.join(format!("{entry}.scss")))
        .output()
        .map_err(Error::SassNotRun)?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(Error::Sass(String::from_utf8_lossy(&output.stderr).into_owned()))
    }
}

/// Identifies what the stylesheet is made of: if it is the same as before, so is the CSS.
fn hash_sources(user_styles: &str) -> u64 {
    let mut hasher = DefaultHasher::new();

    for (rel_path, content) in DEFAULT_STYLES {
        rel_path.hash(&mut hasher);
        if *rel_path != USER_STYLES {
            content.hash(&mut hasher);
        }
    }

    user_styles.hash(&mut hasher);
    hasher.finish()
}

fn store_in_cache(cache: &Path, entry: &str, css: &str) -> std::io::Result<()> {
    let dir = cache.parent().expect("a cache file is in a directory");
    fs::create_dir_all(dir)?;

    // Written elsewhere first, so that nobody reads half of it
    let temporary = cache.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, css)?;
    fs::rename(&temporary, cache)?;

    // Whatever this stylesheet was before is of no use anymore
    let prefix = format!("style-{entry}-");
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let is_old_style = path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
            name.starts_with(&prefix) && name.ends_with(".css")
        });

        if is_old_style && path != cache {
            let _ = fs::remove_file(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_colors() {
        refresh_colors("$a: #ff0000;\n$b: \"Some Font\";\n$c: #00ff00;\n");

        assert!(get_color("a").is_some());
        assert!(get_color("c").is_some());
        assert!(get_color("b").is_none());
        assert!(get_color("missing").is_none());
    }

    #[test]
    fn the_hash_follows_the_user_styles() {
        assert_eq!(hash_sources("a"), hash_sources("a"));
        assert_ne!(hash_sources("a"), hash_sources("b"));
    }

    #[test]
    fn the_default_styles_are_bundled() {
        assert!(DEFAULT_STYLES.iter().any(|(path, _)| *path == "main.scss"));
        assert!(DEFAULT_STYLES.iter().any(|(path, _)| *path == USER_STYLES));
    }

    #[test]
    fn the_default_styles_compile() {
        // Needs sass, like everything else that shows the styles
        let dir = std::env::temp_dir().join(format!("gms_style_compile_{}", std::process::id()));

        write_sources(&dir, &environment::user_styles()).unwrap();
        for entry in ["main", "launcher", "session"] {
            let css = sass(&dir, entry);
            assert!(css.is_ok(), "{entry}: {css:?}");
        }

        fs::remove_dir_all(dir).unwrap();
    }
}
