pub mod base;
mod modules {
    pub mod workspaces;
    pub mod client;
    pub mod sysstats;
    pub mod mpris;
    pub mod clock;
    pub mod tray;
    pub mod volume;
    pub mod discord;
    pub mod network;
    pub mod power_profile;
}

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell as _};

use crate::utils::gesture;
use self::base::{BarModule, hide_all_expanded_modules};

static BAR_HEIGHT: i32 = 33;

/// A widget's allocation (as returned by `allocation()`) is relative to its
/// *direct* parent only. Walk every ancestor up to (but not including) the
/// top-level window, summing their offsets, to get the widget's true
/// position within the window - needed below to hit-test clicks against an
/// expanded module's *actual* on-screen bounds, not just where it sits
/// inside its immediate parent.
fn window_relative_allocation(widget: &impl IsA<gtk::Widget>) -> gdk::Rectangle {
    let widget = widget.as_ref();
    let local = widget.allocation();
    let (mut x, mut y) = (local.x(), local.y());

    let mut ancestor = widget.parent();
    while let Some(parent) = ancestor {
        if parent.downcast_ref::<gtk::Window>().is_some() {
            break;
        }

        let parent_allocation = parent.allocation();
        x += parent_allocation.x();
        y += parent_allocation.y();
        ancestor = parent.parent();
    }

    gdk::Rectangle::new(x, y, local.width(), local.height())
}

#[derive(glib::Downgrade)]
pub struct BarWindow {
    pub window: gtk::ApplicationWindow,
    pub monitor: gdk::Monitor,
    pub modules: Rc<RefCell<HashMap<String, BarModule>>>,
}

impl BarWindow {
    pub fn new(application: &libadwaita::Application, monitor: &gdk::Monitor) -> Self {
        let mpris_module = modules::mpris::new();
        let sysstats_module = modules::sysstats::new();
        let modules = Rc::new(RefCell::new(HashMap::new()));
        modules.borrow_mut().insert("mpris".to_owned(), mpris_module.clone());
        modules.borrow_mut().insert("sysstats".to_owned(), sysstats_module.clone());

        view! {
            left_box = gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 1,
                set_valign: gtk::Align::Start,

                append: &modules::workspaces::new(),
                append: &modules::client::new()
            },

            center_box = gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 1,
                set_valign: gtk::Align::Start,

                append: &sysstats_module,
                append: &mpris_module,
                append: &modules::clock::new()
            },

            right_box = gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 1,
                set_valign: gtk::Align::Start,

                append: &modules::discord::new(),
                append: &modules::tray::new(),
                append: &modules::power_profile::new(),
                append: &modules::network::new(),
                append: &modules::volume::new(),
            },

            window = gtk::ApplicationWindow {
                set_css_classes: &["bar-window"],
                set_application: Some(application),
                init_layer_shell: (),
                set_monitor: Some(monitor),
                set_default_height: BAR_HEIGHT,
                set_layer: Layer::Top,
                set_anchor: (Edge::Left, true),
                set_anchor: (Edge::Right, true),
                set_anchor: (Edge::Top, true),
                set_exclusive_zone: BAR_HEIGHT,
                set_namespace: Some("gms-bar"),

                gtk::CenterBox {
                    set_css_classes: &["bar"],
                    set_start_widget: Some(&left_box),
                    set_center_widget: Some(&center_box),
                    set_end_widget: Some(&right_box),
                }
            }
        }

        let me = BarWindow {
            window,
            monitor: monitor.clone(),
            modules,
        };

        // Collapse expanded modules when clicking elsewhere *within this
        // window* (e.g. on the clock while the MPRIS module is open).
        me.window.add_controller(gesture::on_primary_full_press(clone!(
            #[weak] me,
            move |_, press, release| {
                if me.should_hide_on_click(press, release) {
                    hide_all_expanded_modules();
                }

                me.set_keyboard_mode();
            }
        )));

        me.window.add_controller(gesture::on_secondary_up(clone!(
            #[weak] me,
            move |_, _, _| {
                // usually signifies that a module is being collapsed, but we should make sure that all are collapsed
                me.set_keyboard_mode();
            }
        )));

        // Collapse expanded modules when the bar loses focus entirely (e.g.
        // the user clicked a *different* application's window). A separate
        // always-on-top "steal window" used to be layered over the rest of
        // the screen to catch exactly this - but it's its own Wayland
        // surface, stacked independently of the bar window with no
        // guaranteed ordering between the two, and in practice ended up
        // swallowing clicks genuinely meant for the bar's own expanded
        // content (e.g. the MPRIS buttons) just as often as it caught real
        // outside clicks. The compositor's own notion of window focus has
        // no such stacking ambiguity.
        me.window.connect_is_active_notify(move |window| {
            if !window.is_active() {
                hide_all_expanded_modules();
            }
        });

        me
    }

    pub fn hide_all_expanded_modules(&self) {
        for module in self.modules.borrow().values() {
            module.set_expanded(false);
        }

        self.set_keyboard_mode();
    }

    /// Whether a click within this window, but outside every expanded
    /// module's own bounds (e.g. on the clock while the MPRIS module is
    /// open), should collapse them all.
    fn should_hide_on_click(&self, (px, py): (f64, f64), (rx, ry): (f64, f64)) -> bool {
        if py <= BAR_HEIGHT as f64 || ry <= BAR_HEIGHT as f64 {
            // Clicks on the bar's own row are handled by each module's own
            // click gesture (which collapses every other module when one
            // of them is opened) - not here.
            return false;
        }

        self.modules.borrow().values().filter(|module| module.expanded()).any(|module| {
            let allocation = window_relative_allocation(module);

            !allocation.contains_point(rx as i32, ry as i32)
                && !allocation.contains_point(px as i32, py as i32)
        })
    }

    /// While a module is expanded, promote the bar above other windows and
    /// let it take keyboard focus on demand - needed for connect_is_active_notify
    /// (see its call site) to actually notice focus moving to another
    /// window later.
    pub fn set_keyboard_mode(&self) {
        glib::idle_add_local_once(clone!(
            #[weak(rename_to = me)] self,
            move || {
                let any_expanded = me.modules.borrow().values().any(|module| module.expanded());
                if any_expanded {
                    me.window.set_layer(Layer::Overlay);
                    me.window.set_keyboard_mode(KeyboardMode::OnDemand);
                } else {
                    me.window.set_layer(Layer::Top);
                    me.window.set_keyboard_mode(KeyboardMode::None);
                }
            }
        ));
    }
}


