//! The theme of Gray Meadows, see `gms_style`, on a GTK application.

/// Loads the theme (including the `GMS_*` environment variables) for all windows of the
/// application. Needs to be called from the main thread, once GTK is initialized.
///
/// `entry` is the stylesheet to use, see `gms_style::compile`.
///
/// If the theme can't be compiled, the windows are left with GTK's own look.
pub fn load(entry: &str) {
    let provider = gtk::CssProvider::new();

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    match gms_style::compile(entry) {
        Ok(css) => provider.load_from_data(&css),
        Err(error) => tracing::error!(%error, "Failed to compile the styles"),
    }
}
