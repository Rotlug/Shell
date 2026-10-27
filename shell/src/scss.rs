//! Puts the styles of `gms_style` on the shell.
use crate::APP_LOCAL;

pub use gms_style::get_color;

/// Compiles the styles, and loads them.
pub fn bundle_apply_scss() {
    match gms_style::compile() {
        Ok(css) => glib::MainContext::default().invoke(move || {
            APP_LOCAL.with(|app| app.provider.load_from_data(&css));
        }),
        Err(error) => error!(%error, "Failed to compile the styles"),
    }
}

/// Loads the styles again whenever they are edited.
pub fn watch_scss() {
    match gms_style::watch(bundle_apply_scss) {
        // It watches for as long as it lives, and that's as long as the shell does
        Ok(watcher) => std::mem::forget(watcher),
        Err(error) => error!(%error, "Failed to watch the styles"),
    }
}
