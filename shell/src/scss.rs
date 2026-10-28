//! Puts the styles of `gms_style` on the shell.
use crate::APP_LOCAL;

pub use gms_style::get_color;

/// The colors of the palette that are not shades (base08 to base0F), by the name of their
/// variable in the stylesheet.
const ACCENTS: [&str; 8] = [
    "accent-red",
    "accent-orange",
    "accent-yellow",
    "accent-green",
    "accent-cyan",
    "accent-blue",
    "accent-magenta",
    "accent-brown",
];

/// The accent color for the `index`th of something, going around when there are more of them
/// than accents. `None` when the palette has no accents (the default styles don't).
pub fn accent_color(index: usize) -> Option<color::models::Rgba> {
    get_color(ACCENTS[index % ACCENTS.len()])
}

/// Compiles the styles, and loads them.
pub fn bundle_apply_scss() {
    match gms_style::compile("main") {
        Ok(css) => glib::MainContext::default().invoke(move || {
            APP_LOCAL.with(|app| app.provider.load_from_data(&css));
        }),
        Err(error) => error!(%error, "Failed to compile the styles"),
    }
}
