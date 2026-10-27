use futures_signals::signal::SignalExt as _;
use gtk::prelude::*;

use crate::services::networkmanager::status::{ConnectionStatus, CONNECTION_STATUS};
use super::super::base::BarModule;

const WIFI_ICON: &str = "\u{f05a9}";
const WIFI_OFF_ICON: &str = "\u{f05aa}";

fn status_icon(status: ConnectionStatus) -> &'static str {
    match status {
        ConnectionStatus::Disconnected => WIFI_OFF_ICON,
        ConnectionStatus::Connecting | ConnectionStatus::Limited | ConnectionStatus::Connected => WIFI_ICON,
    }
}

fn status_css_class(status: ConnectionStatus) -> &'static str {
    match status {
        ConnectionStatus::Connected => "connected",
        ConnectionStatus::Limited => "limited",
        ConnectionStatus::Connecting => "connecting",
        ConnectionStatus::Disconnected => "disconnected",
    }
}

fn status_tooltip(status: ConnectionStatus) -> &'static str {
    match status {
        ConnectionStatus::Connected => "Connected to the internet",
        ConnectionStatus::Limited => "Connected, but no internet access",
        ConnectionStatus::Connecting => "Connecting…",
        ConnectionStatus::Disconnected => "No network connection",
    }
}

pub fn new() -> BarModule {
    view! {
        network_icon_label = gtk::Label {
            set_label: status_icon(CONNECTION_STATUS.get()),
            set_css_classes: &["bar-network-icon", status_css_class(CONNECTION_STATUS.get())],
            set_tooltip_text: Some(status_tooltip(CONNECTION_STATUS.get())),
            set_xalign: 0.5,
            set_halign: gtk::Align::Center,
        },

        widget = gtk::Box {
            set_hexpand: false,

            append: &network_icon_label,
        }
    }

    glib::spawn_future_local(signal!(CONNECTION_STATUS, (status) {
        network_icon_label.set_label(status_icon(status));
        network_icon_label.set_css_classes(&["bar-network-icon", status_css_class(status)]);
        network_icon_label.set_tooltip_text(Some(status_tooltip(status)));
    }));

    BarModule::builder()
        .minimal_widget(&widget)
        .build()
}
