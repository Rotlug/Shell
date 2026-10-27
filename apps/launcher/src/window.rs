use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell as _};

use gms_ui::gesture;
use crate::entry::Entry;
use crate::matching::{self, Score};

const ICON_SIZE: i32 = 24;
/// Rows that are shown before the list scrolls
const MAX_LIST_HEIGHT: i32 = 360;
/// How far PageUp and PageDown go
const PAGE: usize = 8;

/// Everything about what is shown, and what is picked.
struct Launcher {
    application: gtk::Application,
    /// Gets the line of what was picked
    picked: Rc<RefCell<Option<String>>>,
    entries: Vec<Entry>,
    /// The rows, by the index of their entry
    rows: Vec<gtk::ListBoxRow>,
    index_of: HashMap<gtk::ListBoxRow, usize>,
    list: gtk::ListBox,
    scroll: gtk::ScrolledWindow,
    no_results: gtk::Label,
    /// How well the entries match what is typed in, by the index of the entry
    scores: RefCell<Vec<Option<Score>>>,
    /// The indexes of the entries that match, best first
    order: RefCell<Vec<usize>>,
    /// Where in `order` the selected one is
    selected: Cell<Option<usize>>,
}

impl Launcher {
    fn update(&self, query: &str) {
        let scores: Vec<Option<Score>> = self.entries.iter().map(|entry| matching::score(&entry.title, query)).collect();

        let mut order: Vec<usize> = (0..scores.len()).filter(|&i| scores[i].is_some()).collect();
        order.sort_by_key(|&i| (scores[i], i));

        self.no_results.set_visible(order.is_empty());
        self.scroll.set_visible(!order.is_empty());

        *self.scores.borrow_mut() = scores;
        *self.order.borrow_mut() = order;

        self.list.invalidate_filter();
        self.list.invalidate_sort();
        self.select(0);
    }

    /// Selects the `position`th of the entries that match, or the nearest one that is there.
    fn select(&self, position: usize) {
        let order = self.order.borrow();
        let Some(&index) = order.get(position.min(order.len().saturating_sub(1))) else {
            self.selected.set(None);
            self.list.unselect_all();
            return;
        };

        let row = &self.rows[index];
        self.selected.set(Some(position.min(order.len() - 1)));
        self.list.select_row(Some(row));

        // The row may not have its place yet, if the list was only just changed
        glib::idle_add_local_once(clone!(
            #[weak] row,
            #[weak(rename_to = scroll)] self.scroll,
            move || scroll_into_view(&scroll, &row)
        ));
    }

    fn move_selection(&self, by: isize) {
        let current = self.selected.get().unwrap_or(0);
        self.select(current.saturating_add_signed(by));
    }

    fn pick(&self, index: usize) {
        *self.picked.borrow_mut() = Some(self.entries[index].line.clone());
        self.application.quit();
    }

    fn pick_selected(&self) {
        let order = self.order.borrow();

        if let Some(&index) = self.selected.get().and_then(|position| order.get(position)) {
            drop(order);
            self.pick(index);
        }
    }
}

fn scroll_into_view(scroll: &gtk::ScrolledWindow, row: &gtk::ListBoxRow) {
    let adjustment = scroll.vadjustment();
    let allocation = row.allocation();
    let (top, bottom) = (f64::from(allocation.y()), f64::from(allocation.y() + allocation.height()));

    if top < adjustment.value() {
        adjustment.set_value(top);
    } else if bottom > adjustment.value() + adjustment.page_size() {
        adjustment.set_value(bottom - adjustment.page_size());
    }
}

fn icon_for(icon: Option<&str>) -> gtk::Image {
    let image = gtk::Image::new();
    image.set_css_classes(&["launcher-item-icon"]);
    image.set_pixel_size(ICON_SIZE);
    // Entries without an icon still line up with the ones that have one
    image.set_size_request(ICON_SIZE, ICON_SIZE);

    match icon {
        Some(path) if path.starts_with('/') => {
            if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, ICON_SIZE, ICON_SIZE, true) {
                image.set_from_pixbuf(Some(&pixbuf));
            }
        }
        Some(name) => {
            let known = gdk::Display::default().is_some_and(|display| gtk::IconTheme::for_display(&display).has_icon(name));

            if known {
                image.set_icon_name(Some(name));
            }
        }
        None => {}
    }

    image
}

fn row_for(entry: &Entry, with_icons: bool) -> gtk::ListBoxRow {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.set_css_classes(&["launcher-item"]);

    if with_icons {
        content.append(&icon_for(entry.icon.as_deref()));
    }

    let title = gtk::Label::new(Some(&entry.title));
    title.set_css_classes(&["launcher-item-title"]);
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&title);

    let row = gtk::ListBoxRow::new();
    // The entry keeps the focus, so that typing always types
    row.set_focusable(false);
    row.set_child(Some(&content));
    row
}

/// Shows the entries. What is picked ends up in `picked`, and the application quits.
pub fn present(application: &gtk::Application, entries: Vec<Entry>, prompt: &str, picked: Rc<RefCell<Option<String>>>) {
    let window = gtk::ApplicationWindow::new(application);
    window.set_css_classes(&["launcher-window"]);
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_namespace(Some("gms-launcher"));
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, true);
    }

    let search_icon = gtk::Label::new(Some("search"));
    search_icon.set_css_classes(&["launcher-entry-icon"]);

    let search = gtk::Entry::new();
    search.set_css_classes(&["launcher-entry"]);
    search.set_placeholder_text(Some(prompt));
    search.set_hexpand(true);
    search.set_has_frame(false);

    let entry_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    entry_box.set_css_classes(&["launcher-entry-box"]);
    entry_box.append(&search_icon);
    entry_box.append(&search);

    let list = gtk::ListBox::new();
    list.set_css_classes(&["launcher-list"]);
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(true);
    list.set_focusable(false);

    let with_icons = entries.iter().any(|entry| entry.icon.is_some());
    let rows: Vec<gtk::ListBoxRow> = entries.iter().map(|entry| row_for(entry, with_icons)).collect();
    for row in &rows {
        list.append(row);
    }

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_vscrollbar_policy(gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(MAX_LIST_HEIGHT);
    scroll.set_focusable(false);

    let no_results = gtk::Label::new(Some("No results"));
    no_results.set_css_classes(&["launcher-no-results"]);
    no_results.set_visible(false);

    let results = gtk::Box::new(gtk::Orientation::Vertical, 0);
    results.set_css_classes(&["launcher-results"]);
    results.append(&scroll);
    results.append(&no_results);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_css_classes(&["launcher-box"]);
    column.set_halign(gtk::Align::Center);
    column.set_valign(gtk::Align::Start);
    column.append(&entry_box);
    column.append(&results);
    window.set_child(Some(&column));

    let launcher = Rc::new(Launcher {
        application: application.clone(),
        picked,
        index_of: rows.iter().cloned().enumerate().map(|(index, row)| (row, index)).collect(),
        scores: RefCell::new(vec![None; entries.len()]),
        order: RefCell::new(Vec::new()),
        selected: Cell::new(None),
        entries,
        rows,
        list: list.clone(),
        scroll,
        no_results,
    });

    list.set_sort_func(clone!(
        #[strong] launcher,
        move |a, b| {
            let key = |row: &gtk::ListBoxRow| {
                let index = launcher.index_of[row];
                (launcher.scores.borrow()[index].unwrap_or(Score::MAX), index)
            };

            key(a).cmp(&key(b)).into()
        }
    ));

    list.set_filter_func(clone!(
        #[strong] launcher,
        move |row| launcher.scores.borrow()[launcher.index_of[row]].is_some()
    ));

    list.connect_row_activated(clone!(
        #[strong] launcher,
        move |_, row| launcher.pick(launcher.index_of[row])
    ));

    search.connect_changed(clone!(
        #[strong] launcher,
        move |search| launcher.update(&search.text())
    ));

    search.connect_activate(clone!(
        #[strong] launcher,
        move |_| launcher.pick_selected()
    ));

    // Before the entry gets to see the keys, or it eats the arrows
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(clone!(
        #[strong] launcher,
        move |_, key, _, state| {
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let name = key.name();

            match (ctrl, name.as_deref()) {
                (_, Some("Escape")) => launcher.application.quit(),
                (false, Some("Down")) | (true, Some("n" | "j")) => launcher.move_selection(1),
                (false, Some("Up")) | (true, Some("p" | "k")) => launcher.move_selection(-1),
                (false, Some("Page_Down")) => launcher.move_selection(PAGE as isize),
                (false, Some("Page_Up")) => launcher.move_selection(-(PAGE as isize)),
                _ => return glib::Propagation::Proceed,
            }

            glib::Propagation::Stop
        }
    ));
    window.add_controller(keys);

    // A click that is not on the launcher closes it
    window.add_controller(gesture::on_primary_full_press(clone!(
        #[weak] application,
        #[weak] column,
        move |_, (px, py), (rx, ry)| {
            let allocation = column.allocation();

            if !allocation.contains_point(px as i32, py as i32) && !allocation.contains_point(rx as i32, ry as i32) {
                application.quit();
            }
        }
    )));

    // A bit down from the top, like where the eye goes
    window.connect_map(clone!(
        #[weak] column,
        move |window| {
            let monitor = window.surface().and_then(|surface| {
                gdk::Display::default()?.monitor_at_surface(&surface)
            });

            if let Some(monitor) = monitor {
                column.set_margin_top(monitor.geometry().height() / 5);
            }
        }
    ));

    launcher.update("");
    window.present();
    search.grab_focus();
}
