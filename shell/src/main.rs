#[macro_use(clone)]
extern crate glib;
#[macro_use(view)]
extern crate relm4_macros;
#[macro_use(warn, error)]
extern crate tracing;
#[macro_use]
mod macros;

mod services;
mod widgets;
mod utils;
mod scss;
mod pixbuf;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::LazyLock;
use futures_signals::signal::Mutable;
use gtk::prelude::*;
use libadwaita::Application;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::time::ChronoLocal;

use self::widgets::{windows::GmsWindow, notifications::NotificationsWindow};
use self::utils::display;


pub struct GrayMeadowsLocal {
    provider: gtk::CssProvider,
    icon_theme: gtk::IconTheme,
    pub bars: RefCell<Vec<widgets::bar::BarWindow>>,
    pub osd_containers: RefCell<Vec<widgets::osd::OsdWindow>>,
    pub notification_containers: RefCell<Vec<NotificationsWindow>>,
    pub windows: RefCell<HashMap<String, Box<dyn GmsWindow>>>,
}

thread_local! {
    pub static APP_LOCAL: GrayMeadowsLocal = GrayMeadowsLocal {
        provider: gtk::CssProvider::new(),
        icon_theme: gtk::IconTheme::default(),
        bars: RefCell::new(Vec::new()),
        osd_containers: RefCell::new(Vec::new()),
        notification_containers: RefCell::new(Vec::new()),
        windows: RefCell::new(HashMap::new()),
    };
}

#[derive(Debug, Clone)]
pub struct GrayMeadowsGlobal {
    game_mode: Mutable<bool>,
    do_not_disturb: Mutable<bool>,
}

pub static APP: LazyLock<GrayMeadowsGlobal> = LazyLock::new(|| GrayMeadowsGlobal {
    game_mode: Mutable::new(false),
    do_not_disturb: Mutable::new(false),
});

pub static USERNAME: LazyLock<String> = LazyLock::new(|| {
    std::env::var("USER").unwrap_or_else(|_| "unknown".to_owned())
});

fn activate(application: &Application) {
    let volume_osd = widgets::osd::imp::volume::VolumeOsd::default();

    for monitor in display::get_all_monitors(&gdk::Display::default().expect("Failed to get default display")) {
        let bar = widgets::bar::BarWindow::new(application, &monitor);
        let osd = widgets::osd::OsdWindow::new(application, &monitor);
        let notifications_window = NotificationsWindow::new(application, &monitor);

        osd.add_osd(&volume_osd);

        bar.window.show();
        osd.window.show();
        notifications_window.window.show();

        APP_LOCAL.with(|app| {
            app.bars.borrow_mut().push(bar);
            app.osd_containers.borrow_mut().push(osd);
            app.notification_containers.borrow_mut().push(notifications_window);
        });
    }

    widgets::notifications::listen_for_notifications();

    APP_LOCAL.with(|app| {
        let mut windows = app.windows.borrow_mut();
        windows.insert("right_sidebar".into(), Box::new(widgets::windows::sidebar_right::new(application)));
    });
}

/// Sets everything up. Returns the application, which `main` runs: see there for why not here.
async fn start_shell() -> Application {
    tracing_subscriber::fmt()
        .with_line_number(true)
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive(LevelFilter::INFO.into()))
        .with_timer(ChronoLocal::new("%H:%M:%S%.3f".to_owned()))
        .init();

    let _ = gtk::init();

    let application = Application::new(
        Some("sn.inpr.gray_meadows_shell"),
        Default::default(),
    );

    // Ensure that another instance of gray-meadows-shell is not running: the name of the
    // application can only be owned by one of them
    if let Err(e) = application.register(None::<&gio::Cancellable>) {
        error!(%e, "Failed to register the application");
        std::process::exit(1);
    }

    if application.is_remote() {
        error!("Looks like another instance of gray-meadows-shell is already running.");
        std::process::exit(1);
    }

    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("Failed to get default display"),
        &APP_LOCAL.with(|app| app.provider.clone()),
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    if let Some(settings) = gtk::Settings::default() {
        let current_icon_theme = settings.property::<String>("gtk-icon-theme-name");
        APP_LOCAL.with(|app| {
            app.icon_theme.set_theme_name(Some(&current_icon_theme));
        });
    }

    scss::bundle_apply_scss();

    services::activate_all().await;

    application.connect_activate(activate);
    application
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("Failed to start the tokio runtime");
    let application = runtime.block_on(start_shell());

    // The main loop must not run inside `block_on`: everything in there shares one cooperative
    // budget of tokio operations (join handles, channels...) that is never replenished while
    // `block_on` doesn't return. Once it ran out, every tokio future polled by the UI thread
    // returns pending and wakes itself up again, so the UI thread spins and nothing completes.
    // Entering the runtime still lets the UI thread spawn tasks.
    let _guard = runtime.enter();
    application.run();
}