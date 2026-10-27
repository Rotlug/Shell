//! Looks for the `GMS_*` environment variables, which set the colors and font.
use std::{fs, path::Path};

use crate::{DEFAULT_STYLES, Result};

/// Base16 slot names (`base00`..`base0F`), in the same order the nix flake's
/// `style.nix` defines them.
const BASE16_SLOTS: [&str; 16] = [
    "00", "01", "02", "03", "04", "05", "06", "07",
    "08", "09", "0A", "0B", "0C", "0D", "0E", "0F",
];

const USER_STYLES: &str = "_user.scss";

/// Reads a full base16 palette from `GMS_BASE16_00`..`GMS_BASE16_0F`
/// environment variables (plain hex, no leading `#`, matching how
/// `self.lib.customDesktopColors` stores them on the nix side). Returns
/// `None` unless every one of the 16 is set, so a partial/missing
/// configuration falls back to the bundled defaults instead of producing a
/// half-themed result.
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

/// What `_user.scss` should be according to the environment, if the environment has an
/// opinion: a palette and/or a font. When only the font is set, the rest comes from the bundled
/// default.
fn user_styles_from_env() -> Option<String> {
    let base16 = base16_from_env();
    let font = font_from_env();

    if base16.is_none() && font.is_none() {
        return None;
    }

    let mut user_scss = base16.as_ref().map_or_else(
        || DEFAULT_STYLES.iter()
            .find(|(rel_path, _)| *rel_path == USER_STYLES)
            .map_or_else(String::new, |(_, content)| (*content).to_owned()),
        generate_base16_user_scss,
    );

    if let Some(font) = &font {
        user_scss = override_fonts(&user_scss, font);
    }

    Some(user_scss)
}

/// Writes `content` to `path`, unless it has that content already. Anything watching the file
/// doesn't need to hear about it every time an app starts.
fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == content) {
        return Ok(());
    }

    // Written elsewhere first, so that nobody reads half of it
    let temporary = path.with_extension("scss.tmp");
    fs::write(&temporary, content)?;
    fs::rename(&temporary, path)?;

    Ok(())
}

/// Writes the stylesheet that isn't there yet to `styles`, and `_user.scss` according to the
/// environment.
pub fn write_default_styles(styles: &Path) -> Result<()> {
    fs::create_dir_all(styles)?;

    let user_styles = user_styles_from_env();
    if let Some(user_styles) = &user_styles {
        write_if_changed(&styles.join(USER_STYLES), user_styles)?;
    }

    for (rel_path, content) in DEFAULT_STYLES {
        if user_styles.is_some() && *rel_path == USER_STYLES {
            continue;
        }

        let path = styles.join(rel_path);

        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::write(&path, content)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_all_fonts() {
        let scss = "$font-family-normal: \"A\";\n$background-color-primary: #000;\n$font-family-big: \"B\";\n";
        let overridden = override_fonts(scss, "My \"Font\"");

        assert_eq!(
            overridden,
            "$font-family-normal: \"My \\\"Font\\\"\";\n$background-color-primary: #000;\n$font-family-big: \"My \\\"Font\\\"\";\n"
        );
    }

    #[test]
    fn base16_needs_every_slot() {
        let palette: [String; 16] = std::array::from_fn(|i| format!("{i:06x}"));
        let scss = generate_base16_user_scss(&palette);

        assert!(scss.contains("$background-color-primary: #000000;"));
        assert!(scss.contains("$foreground-color-primary: #000005;"));
        assert!(scss.contains("$accent-brown: #00000f;"));
    }

    #[test]
    fn only_writes_what_changed() {
        let dir = std::env::temp_dir().join(format!("gms_style_write_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("_user.scss");

        write_if_changed(&path, "a").unwrap();
        let first = fs::metadata(&path).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        write_if_changed(&path, "a").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), first);

        write_if_changed(&path, "b").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "b");

        fs::remove_dir_all(dir).unwrap();
    }
}
