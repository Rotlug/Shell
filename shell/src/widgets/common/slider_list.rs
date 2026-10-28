//! A list of things that are adjusted with a slider each: the streams of the audio tab, the
//! monitors of the brightness tab.
//!
//! The rows are styled by the `audio-stream-*` styles, anything that holds them needs to
//! have `audio-tab-root` among its classes (see the brightness tab).
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use gtk::prelude::*;

use crate::utils::gesture;
use super::dot_separator;

/// How much scrolling over a slider by one notch moves it, from 0.0 to 1.0
const SCROLL_STEP: f64 = 0.05;

/// How many accent colors the styles have (`accent-0` to `accent-7`: base08 to base0F)
const ACCENT_COUNT: usize = 8;

/// A row with a title (a description, a dot, a name), a slider, and what the slider is at.
pub struct SliderRow {
    pub root: gtk::Box,
    accent: Cell<Option<usize>>,
    description_label: gtk::Label,
    name_label: gtk::Label,
    value_label: gtk::Label,
    slider: gtk::Scale,
    is_dragging: Rc<RefCell<bool>>,
}

impl SliderRow {
    /// `value` is from 0.0 to 1.0. `on_change` is told about the values the user picks, in
    /// the same range. `button` is shown after the percentage, if there is one.
    pub fn new(
        description: &str,
        name: &str,
        value: f64,
        on_change: impl Fn(f64) + 'static,
        button: Option<&gtk::Button>,
    ) -> Self {
        let on_change = Rc::new(on_change);

        let description_label = gtk::Label::new(Some(description));
        description_label.set_css_classes(&["audio-stream-description"]);
        description_label.set_xalign(0.0);

        let name_label = gtk::Label::new(Some(name));
        name_label.set_css_classes(&["audio-stream-name"]);
        name_label.set_xalign(0.0);
        name_label.set_hexpand(true);
        name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

        let value_label = gtk::Label::new(Some(&format_percentage(value)));
        value_label.set_css_classes(&["audio-stream-volume"]);
        value_label.set_xalign(1.0);
        value_label.set_width_chars(4);
        value_label.set_halign(gtk::Align::End);

        let is_dragging = Rc::new(RefCell::new(false));
        let slider = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&gtk::Adjustment::new(0.0, 0.0, 1.0, SCROLL_STEP, 0.0, 0.0)));
        slider.set_css_classes(&["audio-stream-volume-slider"]);
        slider.set_draw_value(false);
        slider.set_hexpand(true);
        slider.set_value(value);
        // NOTE: don't attach a GestureDrag to send the value on drag - GtkRange/GtkScale
        // already drag the slider itself via its own internal gesture, and ours would
        // race it to claim the pointer sequence, which can leave the slider dead to
        // mouse input. `change-value` is GTK's own signal for user-driven changes
        // (drag, click-to-jump, keyboard, scroll) and doesn't need a competing gesture.
        slider.connect_change_value(clone!(
            #[strong] on_change,
            move |_, _, value| {
                on_change(value);
                glib::Propagation::Proceed
            }
        ));
        slider.connect_state_flags_changed(clone!(
            #[strong] is_dragging,
            move |slider, _| {
                *is_dragging.borrow_mut() = slider.state_flags().contains(gtk::StateFlags::ACTIVE);
            }
        ));
        slider.add_controller(gesture::on_vertical_scroll(clone!(
            #[weak] slider,
            move |dy| {
                on_change(dy.mul_add(-SCROLL_STEP, slider.value()).clamp(0.0, 1.0));
            }
        )));

        view! {
            controls = gtk::Box {
                append: &slider,
                append: &value_label,
            },

            root = gtk::Box {
                set_css_classes: &["audio-stream-root"],
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,

                gtk::Box {
                    append: &description_label,
                    append: &dot_separator::new(),
                    append: &name_label,
                },

                append: &controls,
            },
        }

        if let Some(button) = button {
            controls.append(button);
        }

        Self {
            root,
            accent: Cell::new(None),
            description_label,
            name_label,
            value_label,
            slider,
            is_dragging,
        }
    }

    /// Shows new values. The slider is left alone while the user is holding it.
    pub fn update(&self, description: &str, name: &str, value: f64) {
        self.description_label.set_label(description);
        self.name_label.set_label(name);
        self.set_value(value);
    }

    /// Gives the row the accent color the `index`th of the colors of the palette, going around
    /// when there are more rows than colors. The styles do the rest.
    pub fn set_accent(&self, index: usize) {
        let index = index % ACCENT_COUNT;

        if let Some(old) = self.accent.replace(Some(index)) {
            self.root.remove_css_class(&format!("accent-{old}"));
        }

        self.root.add_css_class(&format!("accent-{index}"));
    }

    pub fn set_value(&self, value: f64) {
        self.value_label.set_label(&format_percentage(value));

        if !*self.is_dragging.borrow() {
            self.slider.set_value(value);
        }
    }
}

fn format_percentage(value: f64) -> String {
    format!("{:.0}%", value * 100.0)
}

/// Where `SliderRow`s go: a scrolling column of them.
pub struct SliderList {
    pub root: gtk::ScrolledWindow,
    rows: gtk::Box,
    /// How many rows were added so far, which decides the accent color of the next one
    added: Cell<usize>,
}

impl Default for SliderList {
    fn default() -> Self {
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 4);
        rows.set_css_classes(&["audio-streams-root"]);

        let root = gtk::ScrolledWindow::new();
        root.set_child(Some(&rows));
        root.set_hscrollbar_policy(gtk::PolicyType::Never);
        root.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        root.set_vexpand(true);
        root.set_min_content_height(100);

        Self { root, rows, added: Cell::new(0) }
    }
}

impl SliderList {
    /// Rows get their accent color in the order they are added, and keep it for as long as
    /// they live: the ones that stay don't change color when others come and go.
    fn assign_accent(&self, row: &SliderRow) {
        row.set_accent(self.added.get());
        self.added.set(self.added.get() + 1);
    }

    pub fn append(&self, row: &SliderRow) {
        self.assign_accent(row);
        self.rows.append(&row.root);
    }

    pub fn prepend(&self, row: &SliderRow) {
        self.assign_accent(row);
        self.rows.prepend(&row.root);
    }

    pub fn remove(&self, row: &SliderRow) {
        self.rows.remove(&row.root);
    }
}
