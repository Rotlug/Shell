use gtk::prelude::*;

use crate::services::brightness::{self, Event, Monitor, MonitorId, MonitorKind};
use crate::widgets::common::slider_list::{SliderList, SliderRow};

struct MonitorRow {
    monitor: Monitor,
    row: SliderRow,
}

impl MonitorRow {
    fn new(monitor: Monitor) -> Self {
        let id = monitor.id.clone();
        let row = SliderRow::new(
            &description(&monitor),
            &monitor.name,
            monitor.brightness,
            move |value| brightness::set_brightness(&id, value),
            None,
        );

        Self { monitor, row }
    }

    fn update(&mut self, monitor: Monitor) {
        self.row.update(&description(&monitor), &monitor.name, monitor.brightness);
        self.monitor = monitor;
    }
}

/// What the monitor is connected to, or at least what sort it is
fn description(monitor: &Monitor) -> String {
    monitor.connector.clone().unwrap_or_else(|| match monitor.kind {
        MonitorKind::Internal => "Internal".to_owned(),
        MonitorKind::External => "External".to_owned(),
    })
}

struct Monitors {
    rows: Vec<MonitorRow>,
    list: SliderList,
    empty_label: gtk::Label,
}

impl Monitors {
    fn add(&mut self, monitor: Monitor) {
        if let Some(existing) = self.rows.iter_mut().find(|row| row.monitor.id == monitor.id) {
            existing.update(monitor);
            return;
        }

        let internal = monitor.kind == MonitorKind::Internal;
        let row = MonitorRow::new(monitor);

        // The panel of the machine goes on top
        if internal {
            self.list.prepend(&row.row);
        } else {
            self.list.append(&row.row);
        }

        self.rows.push(row);
        self.update_empty_label();
    }

    fn remove(&mut self, id: &MonitorId) {
        if let Some(index) = self.rows.iter().position(|row| row.monitor.id == *id) {
            let row = self.rows.remove(index);
            self.list.remove(&row.row);
            self.update_empty_label();
        }
    }

    fn set_brightness(&mut self, id: &MonitorId, brightness: f64) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.monitor.id == *id) {
            row.monitor.brightness = brightness;
            row.row.set_value(brightness);
        }
    }

    fn update_empty_label(&self) {
        self.empty_label.set_visible(self.rows.is_empty());
    }
}

pub fn new() -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // The rows are styled like the ones of the audio tab, because they are the same
    root.set_css_classes(&["brightness-tab-root", "audio-tab-root"]);

    let empty_label = gtk::Label::new(Some("No adjustable displays"));
    empty_label.set_css_classes(&["brightness-tab-empty-label"]);
    empty_label.set_margin_top(16);

    let list = SliderList::default();
    root.append(&empty_label);
    root.append(&list.root);

    let mut monitors = Monitors {
        rows: Vec::new(),
        list,
        empty_label,
    };
    monitors.update_empty_label();

    glib::spawn_future_local(async move {
        // Subscribe before looking at what's there already, so nothing falls in between
        let mut receiver = brightness::subscribe();

        for monitor in brightness::monitors() {
            monitors.add(monitor);
        }

        while let Ok(event) = receiver.recv().await {
            match event {
                Event::Added(monitor) => monitors.add(monitor),
                Event::Removed(id) => monitors.remove(&id),
                Event::Brightness(id, value) => monitors.set_brightness(&id, value),
            }
        }
    });

    root
}
