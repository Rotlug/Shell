use std::sync::Arc;

use crate::{item::Item, menu::MenuItem};

#[derive(Debug)]
pub enum Event {
    ItemAdded(String, Arc<Item>),
    ItemRemoved(String),
    MenuChanged(String, Vec<MenuItem>),
    /// The icon, the attention icon or the overlay icon of an item changed
    IconChanged(String),
    TitleChanged(String),
    /// The item's new status: `Passive`, `Active` or `NeedsAttention`
    StatusChanged(String, String),
    ToolTipChanged(String),
}
