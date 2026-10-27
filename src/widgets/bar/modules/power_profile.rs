use futures_signals::signal::SignalExt as _;
use gtk::prelude::*;

use crate::utils::gesture;
use crate::services::power_profiles::{self, PowerProfile, ACTIVE_PROFILE_STATE};
use super::super::base::BarModule;

const POWER_SAVER_ICON: &str = "\u{f032a}";
const BALANCED_ICON: &str = "\u{f0f85}";
const PERFORMANCE_ICON: &str = "\u{f04c5}";

fn profile_icon(profile: PowerProfile) -> &'static str {
    match profile {
        PowerProfile::PowerSaver => POWER_SAVER_ICON,
        PowerProfile::Balanced => BALANCED_ICON,
        PowerProfile::Performance => PERFORMANCE_ICON,
    }
}

fn profile_css_class(profile: Option<PowerProfile>) -> &'static str {
    match profile {
        Some(profile) => profile.name(),
        None => "unavailable",
    }
}

fn profile_tooltip(profile: Option<PowerProfile>) -> &'static str {
    match profile {
        Some(PowerProfile::PowerSaver) => "Power saver (click to switch)",
        Some(PowerProfile::Balanced) => "Balanced (click to switch)",
        Some(PowerProfile::Performance) => "Performance (click to switch)",
        None => "Power profiles are unavailable",
    }
}

fn apply(label: &gtk::Label, profile: Option<PowerProfile>) {
    label.set_label(profile.map_or(BALANCED_ICON, profile_icon));
    label.set_css_classes(&["bar-power-profile-icon", profile_css_class(profile)]);
    label.set_tooltip_text(Some(profile_tooltip(profile)));
}

pub fn new() -> BarModule {
    view! {
        click_gesture = gesture::on_primary_down(|_, _, _| {
            power_profiles::cycle_profile();
        }),

        icon_label = gtk::Label {
            set_xalign: 0.5,
            set_halign: gtk::Align::Center,
        },

        widget = gtk::Box {
            set_hexpand: false,

            append: &icon_label,
        }
    }

    apply(&icon_label, ACTIVE_PROFILE_STATE.get());

    glib::spawn_future_local(signal!(ACTIVE_PROFILE_STATE, (profile) {
        apply(&icon_label, profile);
    }));

    let module = BarModule::builder()
        .minimal_widget(&widget)
        .build();
    module.add_controller(click_gesture);
    module
}
