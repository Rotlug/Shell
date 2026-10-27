//! The theme of Gray Meadows, see `gms_style`, on a GTK application.

/// Keeps the theme up to date for as long as it is alive.
pub struct Style {
    _watcher: Option<gms_style::Watcher>,
}

/// Loads the theme (including the `GMS_*` environment variables) for all windows of the
/// application, and loads it again if the stylesheet is edited. Needs to be called from the
/// main thread, once GTK is initialized.
///
/// `entry` is the stylesheet to use, see `gms_style::compile`.
///
/// If the theme can't be compiled, the windows are left with GTK's own look.
#[must_use]
pub fn load(entry: &str) -> Style {
    let provider = gtk::CssProvider::new();

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    reload(&provider, entry);

    // The watcher calls back from its own thread, but a provider only likes to be used from
    // the main one
    let (sender, receiver) = std::sync::mpsc::channel::<()>();
    let entry = entry.to_owned();
    let watcher = gms_style::watch(move || {
        let _ = sender.send(());
    })
    .inspect_err(|error| tracing::warn!(%error, "Not watching the styles"))
    .ok();

    glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
        // Any number of edits is one reload
        if receiver.try_iter().count() > 0 {
            reload(&provider, &entry);
        }

        glib::ControlFlow::Continue
    });

    Style { _watcher: watcher }
}

fn reload(provider: &gtk::CssProvider, entry: &str) {
    match gms_style::compile(entry) {
        Ok(css) => provider.load_from_data(&css),
        Err(error) => tracing::error!(%error, "Failed to compile the styles"),
    }
}
