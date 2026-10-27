//! The look of everything Gray Meadows, see the README.
use std::{
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex},
};

use color::{
    is_valid_hex_color,
    models::{ColorModel as _, Rgba},
};
use notify::{EventKind, Watcher as _, event::{AccessKind, AccessMode}};
use regex::Regex;

include!(concat!(env!("OUT_DIR"), "/default_styles.rs"));

mod environment;
mod error;
mod paths;

pub use error::{Error, Result};
pub use paths::styles_directory;

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

/// Compiles a stylesheet into CSS for GTK, getting everything ready for that first:
/// stylesheet that isn't in `~/.config/gray-meadows/styles` yet is put there, and `GMS_*`
/// environment variables are applied.
///
/// `entry` is the file that is compiled, without `.scss`: the shell has `main`, every app its
/// own. That way, they only carry the styles they use, and new ones don't depend on an old
/// `main.scss` that was copied before they existed.
///
/// This is quick when nothing changed since last time, the CSS is cached then.
///
/// # Errors
/// Errors when the styles can't be read or written, or when `sass` fails (or isn't there).
pub fn compile(entry: &str) -> Result<String> {
    let styles = styles_directory()?;
    environment::write_default_styles(&styles)?;

    refresh_colors(&fs::read_to_string(styles.join("_user.scss")).unwrap_or_default());

    let cache = paths::cache_directory()?.join(format!("style-{entry}-{:016x}.css", hash_sources(&styles)?));

    if let Ok(css) = fs::read_to_string(&cache) {
        return Ok(css);
    }

    let css = run_sass(&styles, entry)?;

    if let Err(error) = store_in_cache(&cache, entry, &css) {
        // Nothing wrong with the CSS, it's just not going to be quick the next time
        tracing::warn!(%error, "Failed to cache the compiled styles");
    }

    Ok(css)
}

fn run_sass(styles: &Path, entry: &str) -> Result<String> {
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

/// Every `.scss` file under `dir`, in a fixed order
fn scss_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();

        if path.is_dir() {
            scss_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "scss") {
            files.push(path);
        }
    }

    files.sort();
    Ok(())
}

/// Identifies what the stylesheet is made of: if it is the same as before, so is the CSS.
fn hash_sources(styles: &Path) -> Result<u64> {
    let mut files = Vec::new();
    scss_files(styles, &mut files)?;

    let mut hasher = DefaultHasher::new();
    for file in files {
        file.strip_prefix(styles).unwrap_or(&file).hash(&mut hasher);
        fs::read(&file)?.hash(&mut hasher);
    }

    Ok(hasher.finish())
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

/// Keeps watching the styles for as long as it is alive.
pub struct Watcher {
    _watcher: notify::RecommendedWatcher,
}

/// Calls `on_change` (from another thread) whenever a stylesheet is edited, so that the
/// styles can be compiled again.
///
/// # Errors
/// Errors when the styles can't be watched.
pub fn watch(on_change: impl Fn() + Send + 'static) -> Result<Watcher> {
    let styles = styles_directory()?;
    fs::create_dir_all(&styles)?;

    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
        // Access(Close(Write)) means that the file is done being written to
        Ok(event) => if event.paths.iter().any(|path| path.extension().is_some_and(|extension| extension == "scss"))
            && matches!(event.kind, EventKind::Access(AccessKind::Close(AccessMode::Write)))
        {
            on_change();
        },
        Err(error) => tracing::error!(%error, "Error watching the styles"),
    })?;

    watcher.watch(&styles, notify::RecursiveMode::Recursive)?;
    tracing::info!(styles = %styles.display(), "Watching the styles");

    Ok(Watcher { _watcher: watcher })
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
    fn the_hash_follows_the_content() {
        let dir = std::env::temp_dir().join(format!("gms_style_hash_{}", std::process::id()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("main.scss"), "a").unwrap();
        fs::write(dir.join("sub/_b.scss"), "b").unwrap();

        let before = hash_sources(&dir).unwrap();
        assert_eq!(hash_sources(&dir).unwrap(), before);

        fs::write(dir.join("sub/_b.scss"), "c").unwrap();
        assert_ne!(hash_sources(&dir).unwrap(), before);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_default_styles_are_bundled() {
        assert!(DEFAULT_STYLES.iter().any(|(path, _)| *path == "main.scss"));
        assert!(DEFAULT_STYLES.iter().any(|(path, _)| *path == "_user.scss"));
    }
}
