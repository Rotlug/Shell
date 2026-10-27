use std::sync::Arc;
use gio::prelude::{ActionMapExt as _, ActionMapExtManual as _, Cast as _};
use systray::item::Item;
use systray::menu::{MenuItem, MenuItemContents};

fn label_of(item: &MenuItem) -> Option<&str> {
    match &item.contents {
        MenuItemContents::Label(label)
        | MenuItemContents::RadioButton(label, _)
        | MenuItemContents::CheckboxButton(label, _) => Some(label),
        MenuItemContents::Separator => None,
    }
}

fn action_name(item: &MenuItem) -> String {
    format!("item{}", item.id)
}

fn build_gio_menu(
    tray_item: &Arc<Item>,
    items: &[MenuItem],
    action_entries: &mut Vec<gio::ActionEntry<gio::SimpleActionGroup>>,
    disabled_actions: &mut Vec<String>,
) -> gio::Menu {
    let menu = gio::Menu::new();

    for item in items {
        let Some(label) = label_of(item) else {
            continue;
        };

        if !item.children.is_empty() {
            let submenu = build_gio_menu(tray_item, &item.children, action_entries, disabled_actions);
            menu.append_submenu(Some(label), &submenu);
            continue;
        }

        let name = action_name(item);
        let id = item.id;

        menu.append(Some(label), Some(&format!("dbusmenu.{name}")));

        if !item.enabled {
            disabled_actions.push(name.clone());
        }

        action_entries.push(gio::ActionEntry::builder(&name)
            .activate(clone!(
                #[strong] tray_item,
                move |_, _, _| {
                    tokio::spawn(clone!(
                        #[strong] tray_item,
                        async move {
                            if let Some(menu) = tray_item.menu()
                                && let Err(e) = menu.activate(id).await
                            {
                                error!(%e, id, "Failed to activate menu item");
                            }
                        }
                    ));
                }
            ))
            .build());
    }

    menu
}

/// Builds a menu model, and the actions that back it (to be inserted as `dbusmenu`), out of
/// a tray item's menu.
pub fn build_gio_dbus_menu_model(tray_item: &Arc<Item>, items: &[MenuItem]) -> (gio::MenuModel, gio::SimpleActionGroup) {
    let mut action_entries = vec![];
    let mut disabled_actions = vec![];
    let menu = build_gio_menu(tray_item, items, &mut action_entries, &mut disabled_actions);

    let action_group = gio::SimpleActionGroup::new();
    action_group.add_action_entries(action_entries);

    for name in disabled_actions {
        if let Some(action) = action_group.lookup_action(&name)
            && let Ok(action) = action.downcast::<gio::SimpleAction>()
        {
            action.set_enabled(false);
        }
    }

    (menu.into(), action_group)
}
