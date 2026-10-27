use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use notify::{EventKind, Watcher as _};
use notify::event::{AccessKind, AccessMode};
use regex::Regex;

use crate::APP_LOCAL;
use crate::utils::filesystem;
use crate::color::is_valid_hex_color;
use crate::color::models::{Rgba, ColorModel as _};

include!(concat!(env!("OUT_DIR"), "/default_styles.rs"));

const VAR_REGEX: &str = r#"^\$([a-zA-Z0-9_-]+):\s*(?:"?([^"]+)"?|([a-zA-Z0-9#() ,.-])+);$"#;

pub static SCSS_VARS: LazyLock<Mutex<ScssVars>> = LazyLock::new(|| Mutex::new(ScssVars::new()));

pub struct ScssVars {
    string_vars: HashMap<String, String>,
    color_vars: HashMap<String, Rgba>,
}

impl ScssVars {
    pub fn new() -> Self {
        Self {
            string_vars: HashMap::new(),
            color_vars: HashMap::new(),
        }
    }

    pub fn add_string(&mut self, name: String, value: String) {
        self.string_vars.insert(name, value);
    }

    pub fn add_color(&mut self, name: String, value: Rgba) {
        self.color_vars.insert(name, value);
    }

    #[allow(dead_code)]
    pub fn get_string(&self, name: &str) -> Option<&String> {
        self.string_vars.get(name)
    }

    pub fn get_color(&self, name: &str) -> Option<&Rgba> {
        self.color_vars.get(name)
    }
}

pub fn refresh_variables() {
    let mut vars = ScssVars::new();
    let regex = Regex::new(VAR_REGEX).unwrap();

    let styles_dir = filesystem::get_styles_directory();
    let user_styles_path = format!("{}/_user.scss", styles_dir);

    if let Ok(content) = std::fs::read_to_string(user_styles_path) {
        for line in content.lines() {
            if let Some(caps) = regex.captures(line) {
                let name = caps[1].to_string();
                let value = caps[2].to_string();

                if is_valid_hex_color(&value) {
                    vars.add_color(name, Rgba::from_hex(&value));
                } else {
                    vars.add_string(name, value);
                }
            }
        }
    }

    let mut scss_vars = SCSS_VARS.lock().unwrap();
    *scss_vars = vars;
}

pub fn escape_html(input: char) -> String {
    match input {
        '&' => "&amp;".to_owned(),
        '<' => "&lt;".to_owned(),
        '>' => "&gt;".to_owned(),
        '"' => "&quot;".to_owned(),
        '\'' => "&#39;".to_owned(),
        _ => input.to_string(),
    }
}

pub fn get_color(name: &str) -> Option<Rgba> {
    let scss_vars = SCSS_VARS.lock().unwrap();
    scss_vars.get_color(name).copied()
}

#[allow(dead_code)]
pub fn get_string(name: &str) -> Option<String> {
    let scss_vars = SCSS_VARS.lock().unwrap();
    scss_vars.get_string(name).cloned()
}

/// Base16 slot names (`base00`..`base0F`), in the same order the nix flake's
/// `style.nix` defines them.
const BASE16_SLOTS: [&str; 16] = [
    "00", "01", "02", "03", "04", "05", "06", "07",
    "08", "09", "0A", "0B", "0C", "0D", "0E", "0F",
];

/// Reads a full base16 palette from `GMS_BASE16_00`..`GMS_BASE16_0F`
/// environment variables (plain hex, no leading `#`, matching how
/// `self.lib.customDesktopColors` stores them on the nix side). Returns
/// `None` unless every one of the 16 is set, so a partial/missing
/// configuration falls back to this shell's own bundled defaults below
/// instead of producing a half-themed result.
fn base16_from_env() -> Option<[String; 16]> {
    let mut colors = Vec::with_capacity(16);

    for slot in BASE16_SLOTS {
        colors.push(std::env::var(format!("GMS_BASE16_{slot}")).ok()?);
    }

    colors.try_into().ok()
}

/// Renders a `_user.scss` that maps the shell's existing (grayscale-named)
/// variables onto a base16 palette: base00-07 cover the
/// background/foreground scale these variables were already designed
/// around, and base08-0F are additionally exposed as `$accent-*` variables
/// for any styling that wants an actual hue rather than a shade of gray.
fn generate_base16_user_scss(base16: &[String; 16]) -> String {
    let hex = |i: usize| format!("#{}", base16[i]);
    let (bg0, bg1, bg2, bg3) = (hex(0), hex(1), hex(2), hex(3));
    let (fg4, fg5, fg6, fg7) = (hex(4), hex(5), hex(6), hex(7));

    format!(
        "// Generated from GMS_BASE16_* environment variables (see the nix flake's style.nix).\n\
         // Edits here are overwritten on every start; customize style.nix instead.\n\
         $background-black-semi-trans: {bg0}88;\n\
         $background-color-primary: {bg0};\n\
         $background-color-primary-semitrans: {bg0}64;\n\
         $background-color-primary-semitrans-hover: {bg0}aa;\n\
         $background-color-primary-semitrans-active: {bg0}dd;\n\
         $background-color-secondary: {bg1};\n\
         $background-color-tertiary: {bg1};\n\
         $background-color-quaternary: {bg2};\n\
         $background-color-quinary: {bg3};\n\
         $border-color-primary: {bg2};\n\
         $foreground-color-primary: {fg5};\n\
         $foreground-color-primary-focus: {fg6};\n\
         $foreground-color-primary-active: {fg7};\n\
         $foreground-color-select: {fg4};\n\
         $foreground-color-active: {fg4};\n\
         $foreground-color-secondary: {fg4};\n\
         $foreground-color-tertiary: {bg3};\n\
         $foreground-color-quaternary: {bg3};\n\
         $foreground-color-quinary: {bg2};\n\
         $accent-red: {red};\n\
         $accent-orange: {orange};\n\
         $accent-yellow: {yellow};\n\
         $accent-green: {green};\n\
         $accent-cyan: {cyan};\n\
         $accent-blue: {blue};\n\
         $accent-magenta: {magenta};\n\
         $accent-brown: {brown};\n\
         $font-family-normal: \"Cozette\";\n\
         $font-family-small: \"GohuFont uni11 Nerd Font\";\n\
         $font-family-big: \"GohuFont uni14 Nerd Font\";\n\
         $font-family-variable: \"Inconsolata Nerd Font\";\n",
        bg0 = bg0, bg1 = bg1, bg2 = bg2, bg3 = bg3,
        fg4 = fg4, fg5 = fg5, fg6 = fg6, fg7 = fg7,
        red = hex(8), orange = hex(9), yellow = hex(10), green = hex(11),
        cyan = hex(12), blue = hex(13), magenta = hex(14), brown = hex(15),
    )
}

/// Reads the font family from the `GMS_FONT` environment variable, if set
/// to something non-empty.
fn font_from_env() -> Option<String> {
    std::env::var("GMS_FONT").ok().map(|font| font.trim().to_owned()).filter(|font| !font.is_empty())
}

/// Replaces every `$font-family-*` variable in `user_scss` with `font`, so a
/// single `GMS_FONT` applies to all of the shell's text sizes.
fn override_fonts(user_scss: &str, font: &str) -> String {
    let font = font.replace('\\', "\\\\").replace('"', "\\\"");

    user_scss.lines().map(|line| {
        line.strip_prefix("$font-family-")
            .and_then(|rest| rest.split_once(':'))
            .map_or_else(|| line.to_owned(), |(name, _)| format!("$font-family-{name}: \"{font}\";"))
    }).collect::<Vec<_>>().join("\n") + "\n"
}

/// Overwrites `_user.scss` with colors derived from `GMS_BASE16_*`
/// environment variables (if a full palette is set) and/or the font from
/// `GMS_FONT`. When only the font is set, the rest comes from the bundled
/// default. Returns `true` when it wrote the file, so the caller can skip
/// writing the bundled default in that case.
fn write_env_user_styles(styles_path: &Path) -> bool {
    let base16 = base16_from_env();
    let font = font_from_env();

    if base16.is_none() && font.is_none() {
        return false;
    }

    let mut user_scss = base16.as_ref().map_or_else(
        || DEFAULT_STYLES.iter()
            .find(|(rel_path, _)| *rel_path == "_user.scss")
            .map_or_else(String::new, |(_, content)| (*content).to_owned()),
        generate_base16_user_scss,
    );

    if let Some(font) = &font {
        user_scss = override_fonts(&user_scss, font);
    }

    let abs_path = styles_path.join("_user.scss");

    if let Err(error) = std::fs::write(&abs_path, user_scss) {
        error!(%error, path = %abs_path.display(), "Failed to write environment-derived user styles");
    }

    true
}

pub fn write_default_styles() {
    let styles_dir = filesystem::get_styles_directory();
    let styles_path = Path::new(&styles_dir);
    let _ = std::fs::create_dir_all(styles_path);

    let wrote_env_user_styles = write_env_user_styles(styles_path);

    for (rel_path, content) in DEFAULT_STYLES {
        if wrote_env_user_styles && *rel_path == "_user.scss" {
            continue;
        }

        let abs_path = styles_path.join(rel_path);

        if !abs_path.exists() {
            if let Some(parent) = abs_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            if let Err(error) = std::fs::write(&abs_path, content) {
                error!(%error, path = %abs_path.display(), "Failed to write default style");
            }
        }
    }
}

pub fn bundle_apply_scss() {
    write_default_styles();
    
    let styles_directory = filesystem::get_styles_directory();
    let output = std::process::Command::new("sass")
        .arg(format!("-I {}", styles_directory))
        .arg(format!("{}/main.scss", styles_directory))
        .output()
        .expect("Failed to run sass command");
    
    if output.status.success() {
        let css = String::from_utf8_lossy(&output.stdout).to_string();
        
        glib::MainContext::default().invoke(move || {
            APP_LOCAL.with(|app| app.provider.load_from_data(&css));
        });
        
        refresh_variables();
    } else {
        error!(stderr = %String::from_utf8_lossy(&output.stderr), "Error running sass");
    }
}

pub fn watch_scss() {
    tokio::spawn(async move {
        let (tx, rx) = std::sync::mpsc::channel();
        let styles_path = filesystem::get_styles_directory();

        let mut watcher = notify::recommended_watcher(tx).unwrap();
        let result = watcher.watch(
            Path::new(&styles_path),
            notify::RecursiveMode::Recursive,
        );

        if let Err(error) = result {
            error!(%error, "Failed to watch styles directory");
        } else {
            info!(%styles_path, "Watching styles directory");

            for res in rx {
                match res {
                    // If the event kind is Access(Close(Write)), it means the file is done being written to
                    Ok(event) => if event.paths.iter().any(|p| p.extension() == Some("scss".as_ref())
                        && matches!(event.kind, EventKind::Access(AccessKind::Close(AccessMode::Write))))
                    {
                        debug!(paths = ?event.paths, "Styles changed");
                        bundle_apply_scss();
                    },

                    Err(e) => {
                        error!(%e, "Error watching styles directory");
                    }
                }
            }
        }
    });
}