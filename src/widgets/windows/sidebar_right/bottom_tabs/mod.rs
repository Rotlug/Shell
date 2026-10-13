pub mod calendar;

use crate::widgets::common::tabs::{Tabs, TabSize};

pub fn new() -> Tabs {
    let tabs = Tabs::new(TabSize::Tiny, true, Some("sidebar-right-bottom-tabs-content"));
    tabs.add_tab("calendar", "calendar", Some("calendar_today"), &calendar::new());
    tabs.current_tab.set(Some("calendar".to_owned()));
    tabs
}