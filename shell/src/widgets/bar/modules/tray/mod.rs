mod icon;
mod menu;

use std::sync::Arc;
use gtk::prelude::*;
use systray::event::Event;
use systray::item::{Icon, Item};

use crate::utils::gesture;
use crate::services::tray;
use super::super::base::BarModule;

const ICON_SIZE: i32 = 28;

struct SystemTrayItem {
    address: String,
    widget: gtk::Image,
    popover_menu: gtk::PopoverMenu,
}

impl SystemTrayItem {
    fn new(address: String, item: &Arc<Item>) -> Self {
        let empty_model: gio::MenuModel = gio::Menu::new().into();
        let popover_menu = gtk::PopoverMenu::from_model(Some(&empty_model));

        popover_menu.set_css_classes(&["bar-tray-popover-menu"]);

        let default_activate = gesture::on_primary_down(clone!(
            #[strong] item,
            move |_, x, y| {
                tokio::spawn(clone!(
                    #[strong] item,
                    async move {
                        if let Err(e) = item.activate(x as i32, y as i32).await {
                            error!(%e, "Failed to activate tray item");
                        }
                    }
                ));
            }
        ));

        let menus_activate = gesture::on_secondary_down(clone!(
            #[strong] item,
            #[weak] popover_menu,
            move |_, x, y| {
                glib::spawn_future_local(clone!(
                    #[strong] item,
                    async move {
                        let loaded = tokio::spawn(clone!(
                            #[strong] item,
                            async move {
                                let Some(menu) = item.menu() else {
                                    // This item has no menu to give us, let it show its own
                                    let _ = item.context_menu(x as i32, y as i32).await;
                                    return None;
                                };

                                // Without this the menu may not have its latest contents
                                let _ = menu.update_menu().await;
                                menu.items().await.ok()
                            }
                        )).await;

                        if let Ok(Some(items)) = loaded {
                            let (model, actions) = menu::build_gio_dbus_menu_model(&item, &items);
                            popover_menu.set_menu_model(Some(&model));
                            popover_menu.insert_action_group("dbusmenu", Some(&actions));
                            popover_menu.popup();
                        }
                    }
                ));
            }
        ));

        view! {
            widget = gtk::Image {
                set_css_classes: &["bar-tray-item"],
                set_pixel_size: 14,
                add_controller: default_activate,
                add_controller: menus_activate
            }
        };

        popover_menu.set_parent(&widget);

        Self {
            address,
            widget,
            popover_menu,
        }
    }

    /// Fetches the item's icon in the background, and shows it once it's there.
    fn reload_icon(&self, item: &Arc<Item>) {
        glib::spawn_future_local(clone!(
            #[weak(rename_to = widget)] self.widget,
            #[strong] item,
            async move {
                let loaded = tokio::spawn(async move {
                    // A pixmap is what the item actually drew, so it wins over a theme icon
                    match item.icon_pixmap(ICON_SIZE).await {
                        Some(pixmap) => Some(pixmap),
                        None => item.icon_name().await.map(Icon::System),
                    }
                }).await;

                if let Ok(icon) = loaded {
                    widget.set_from_pixbuf(icon::make_icon_pixbuf(icon.as_ref()).as_ref());
                }
            }
        ));
    }

    fn reload_tooltip(&self, item: &Arc<Item>) {
        glib::spawn_future_local(clone!(
            #[weak(rename_to = widget)] self.widget,
            #[strong] item,
            async move {
                let loaded = tokio::spawn(async move {
                    match item.tool_tip().await {
                        Ok(tool_tip) if !tool_tip.title.is_empty() => Some(tool_tip.title),
                        _ => item.title().await.ok().filter(|title| !title.is_empty()),
                    }
                }).await;

                if let Ok(Some(tooltip)) = loaded {
                    widget.set_tooltip_text(Some(&tooltip));
                }
            }
        ));
    }
}

struct SystemTray {
    box_: gtk::Box,
    items: Vec<(SystemTrayItem, Arc<Item>)>,
}

impl SystemTray {
    fn new(box_: gtk::Box) -> Self {
        Self {
            box_,
            items: Vec::new(),
        }
    }

    fn update_visibility(&self) {
        self.box_.set_visible(!self.items.is_empty());
    }

    fn add_item(&mut self, address: String, item: Arc<Item>) {
        if self.items.iter().any(|(existing, _)| existing.address == address) {
            return;
        }

        let tray_item = SystemTrayItem::new(address, &item);
        tray_item.reload_icon(&item);
        tray_item.reload_tooltip(&item);

        self.box_.append(&tray_item.widget);
        self.items.push((tray_item, item));
        self.update_visibility();
    }

    fn remove_item(&mut self, address: &str) {
        if let Some(pos) = self.items.iter().position(|(item, _)| item.address == address) {
            let (item, _) = self.items.remove(pos);

            item.popover_menu.unparent();
            self.box_.remove(&item.widget);
            self.update_visibility();
        }
    }

    fn find(&self, address: &str) -> Option<&(SystemTrayItem, Arc<Item>)> {
        self.items.iter().find(|(item, _)| item.address == address)
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::ItemAdded(address, item) => self.add_item(address, item),
            Event::ItemRemoved(address) => self.remove_item(&address),

            Event::IconChanged(address) => if let Some((tray_item, item)) = self.find(&address) {
                tray_item.reload_icon(item);
            },

            Event::ToolTipChanged(address) | Event::TitleChanged(address) => if let Some((tray_item, item)) = self.find(&address) {
                tray_item.reload_tooltip(item);
            },

            // The menus are fetched whenever they're opened, and passive items are shown as well
            Event::MenuChanged(..) | Event::StatusChanged(..) => {},
        }
    }
}

pub fn new() -> BarModule {
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    box_.set_visible(false);
    box_.set_hexpand(false);

    let mut tray = SystemTray::new(box_.clone());

    glib::spawn_future_local(async move {
        // Subscribe before looking at what's there already, so nothing falls in between
        let mut receiver = tray::subscribe();

        let mut current: Vec<_> = tray::items().await.into_iter().collect();
        current.sort_by(|a, b| a.0.cmp(&b.0));

        for (address, item) in current {
            tray.add_item(address, item);
        }

        while let Ok(event) = receiver.recv().await {
            tray.handle_event(event);
        }
    });

    let module = BarModule::builder()
        .minimal_widget(&box_)
        .build();
    module.add_css_class("bar-tray");
    module
}
