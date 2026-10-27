#[macro_use(clone)]
extern crate glib;

mod action;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell as _};

use gms_ui::gesture;
use self::action::Action;

fn session_button(application: &gtk::Application, window: &gtk::ApplicationWindow, icon: &str, action: Action) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_valign(gtk::Align::Center);
    button.set_css_classes(&["session-button"]);
    button.connect_clicked(clone!(
        #[weak] application,
        #[weak] window,
        move |_| {
            // Out of the way first, some of these take a moment
            window.set_visible(false);

            if let Err(error) = action.run() {
                eprintln!("gms-session: {error}");
            }

            application.quit();
        }
    ));

    let label = gtk::Label::new(Some(icon));
    label.set_css_classes(&["session-button-icon"]);
    button.set_child(Some(&label));

    button
}

fn row(class: &str, buttons: &[&gtk::Button]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_css_classes(&[class]);

    for button in buttons {
        row.append(*button);
    }

    row
}

fn activate(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);
    window.set_css_classes(&["session-window"]);
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_namespace(Some("gms-session"));
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, true);
    }

    let lock = session_button(application, &window, "lock", Action::Lock);
    let logout = session_button(application, &window, "logout", Action::Logout);
    let suspend = session_button(application, &window, "remove_circle_outline", Action::Suspend);
    let hibernate = session_button(application, &window, "mode_standby", Action::Hibernate);
    let reboot = session_button(application, &window, "restart_alt", Action::Reboot);
    let shutdown = session_button(application, &window, "power_settings_new", Action::Shutdown);

    let buttons = gtk::Box::new(gtk::Orientation::Vertical, 12);
    buttons.set_halign(gtk::Align::Center);
    buttons.set_valign(gtk::Align::Center);
    buttons.set_hexpand(true);
    buttons.append(&row("session-box-row1", &[&lock, &logout, &suspend]));
    buttons.append(&row("session-box-row2", &[&hibernate, &reboot, &shutdown]));
    window.set_child(Some(&buttons));

    // Escape, or a click that is not on the buttons, closes it
    window.add_controller(gesture::on_key_press(clone!(
        #[weak] application,
        move |key, _| if key.name() == Some("Escape".into()) {
            application.quit();
        }
    )));

    window.add_controller(gesture::on_primary_full_press(clone!(
        #[weak] application,
        #[weak] buttons,
        move |_, (px, py), (rx, ry)| {
            let allocation = buttons.allocation();

            if !allocation.contains_point(px as i32, py as i32) && !allocation.contains_point(rx as i32, ry as i32) {
                application.quit();
            }
        }
    )));

    window.present();
}

fn main() -> glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id("sn.inpr.gms_session")
        // It's not the kind of thing to be started twice, and a second one shouldn't be
        // swallowed by the first
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    // The theme needs GTK to be ready, which it is once the application starts
    application.connect_startup(|_| {
        // Lives as long as the application does
        std::mem::forget(gms_ui::style::load("session"));
    });
    application.connect_activate(activate);

    application.run()
}
