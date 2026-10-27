pub mod notification;

use std::rc::Rc;
use std::cell::{Cell, RefCell};
use std::time::Duration;
use gtk::prelude::*;
use cairo::{Region, RectangleInt};
use gtk4_layer_shell::{Edge, Layer, LayerShell as _};

use crate::{APP, APP_LOCAL};
use crate::services::notifications::{self, Urgency};
use crate::widgets::notifications::notification::NotificationDismissAnimation;
use self::notification::NotificationWidget;

const NOTIF_DISPLAY_TIMEOUT: i32 = 2500; // ms
const CRITICAL_NOTIF_DISPLAY_TIMEOUT: i32 = 5000; // ms

#[derive(Clone)]
pub struct NotificationsWindow {
    pub widgets: Rc<RefCell<Vec<NotificationWidget>>>,
    pub window: gtk::ApplicationWindow,
    pub container: gtk::Box,
    /// Whether the input region is being kept up to date, see `start_input_region_updates`
    input_region_updates: Rc<Cell<bool>>,
}

/// Updates the input region every frame to match the size of the window's child, for as long as
/// there are notifications. A tick callback keeps the frame clock running, and every update
/// commits the surface of this window (which covers the whole monitor), so there must not be
/// one while there is nothing to follow.
fn start_input_region_updates(window: &gtk::ApplicationWindow, running: &Rc<Cell<bool>>) {
    if running.replace(true) {
        return;
    }

    window.add_tick_callback(clone!(
        #[strong] running,
        move |window, _| {
            let Some(child) = window.child() else {
                running.set(false);
                return glib::ControlFlow::Break;
            };

            let allocation = child.allocation();
            let region = Region::create_rectangle(&RectangleInt::new(
                allocation.x(),
                allocation.y(),
                allocation.width(),
                allocation.height(),
            ));

            if let Some(surface) = window.native().and_then(|n| n.surface()) {
                surface.set_input_region(&region);
            }

            if child.first_child().is_some() {
                glib::ControlFlow::Continue
            } else {
                running.set(false);
                glib::ControlFlow::Break
            }
        }
    ));
}

impl NotificationsWindow {
    pub fn new(application: &libadwaita::Application, monitor: &gdk::Monitor) -> Self {
        view! {
            container = gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Start,
                set_overflow: gtk::Overflow::Visible,
            },

            window = gtk::ApplicationWindow {
                set_css_classes: &["notifications-window"],
                set_application: Some(application),
                init_layer_shell: (),
                set_monitor: Some(monitor),
                set_layer: Layer::Overlay,
                set_anchor: (Edge::Top, true),
                set_anchor: (Edge::Bottom, true),
                set_anchor: (Edge::Left, true),
                set_anchor: (Edge::Right, true),
                set_namespace: Some("gms-notifications"),
                set_child: Some(&container),
            }
        }

        let input_region_updates = Rc::new(Cell::new(false));
        // Sets the (empty) input region of the window as it is now, so that it doesn't catch input
        start_input_region_updates(&window, &input_region_updates);

        NotificationsWindow {
            widgets: Rc::new(RefCell::new(Vec::new())),
            window,
            container,
            input_region_updates,
        }
    }

    pub fn add_widget(&self, widget: &NotificationWidget) {
        self.widgets.borrow_mut().push(widget.clone());
        start_input_region_updates(&self.window, &self.input_region_updates);

        // clamp to 300px
        let clamp = libadwaita::Clamp::new();
        clamp.set_width_request(300);
        clamp.set_maximum_size(300);
        clamp.set_unit(libadwaita::LengthUnit::Px);
        clamp.set_child(Some(&widget.root));

        self.container.prepend(&clamp);

        let timeout = if widget.notification.borrow().urgency == Urgency::Urgent {
            CRITICAL_NOTIF_DISPLAY_TIMEOUT
        } else {
            NOTIF_DISPLAY_TIMEOUT
        } as u64;

        glib::timeout_add_local_once(Duration::from_millis(timeout), clone!(
            #[weak] widget,
            move || {
                widget.queue_destroy(Some(NotificationDismissAnimation::Right));
            }
        ));
    }
}

pub fn listen_for_notifications() {
    use notifications::BusEvent;
    let mut receiver = notifications::subscribe();

    glib::spawn_future_local(async move {
        while let Ok(message) = receiver.recv().await {
            match message {
                BusEvent::NotificationAdded(notification) => if !APP.do_not_disturb.get() {
                    APP_LOCAL.with(move |app| {
                        for container in app.notification_containers.borrow().iter() {
                            let mut notif_widget = NotificationWidget::new(notification.clone());
                            notif_widget.set_notifications_ref(&container.widgets);
                            container.add_widget(&notif_widget);
                        }
                    });
                },

                BusEvent::NotificationClosed(id) => {
                    APP_LOCAL.with(move |app| {
                        for container in app.notification_containers.borrow().iter() {
                            let widgets = container.widgets.borrow().clone();
                            for widget in &widgets {
                                if widget.notification.borrow().id == id {
                                    widget.destroy(Some(notification::NotificationDismissAnimation::Right));
                                }
                            }
                        }
                    });
                },

                BusEvent::NotificationUpdated(id, notification) => {
                    APP_LOCAL.with(move |app| {
                        for container in app.notification_containers.borrow().iter() {
                            let widgets = container.widgets.borrow().clone();
                            for widget in &widgets {
                                if widget.notification.borrow().id == id {
                                    widget.update(&notification);
                                }
                            }
                        }
                    });
                },
            }
        }
    });
}