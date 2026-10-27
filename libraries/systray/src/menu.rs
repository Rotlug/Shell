use std::time::{SystemTime, UNIX_EPOCH};

use futures::{StreamExt, lock::Mutex};
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use zbus::{
    Connection,
    zvariant::{OwnedObjectPath, OwnedValue, Structure, Value},
};

use crate::{
    dbus::dbusmenu::{DBusMenuProxy, MenuLayout},
    event::Event,
};

/// `Menu` is used for displaying interacting with the right-click menu of a system tray item.
#[derive(Debug)]
pub struct Menu {
    proxy: DBusMenuProxy<'static>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    event_tx: Sender<Event>,
    owner_address: String,
}

impl Menu {
    pub async fn new(
        connection: Connection,
        address: String,
        destination: &str,
        path: OwnedObjectPath,
        event_tx: Sender<Event>,
    ) -> crate::Result<Self> {
        let proxy = DBusMenuProxy::builder(&connection)
            .destination(destination.to_owned())?
            .path(path)?
            .build()
            .await?;

        Ok(Self {
            proxy,
            handles: Mutex::new(Vec::new()),
            event_tx,
            owner_address: address,
        })
    }

    /// Spawn the task for updating `items`
    pub async fn watch_updates(&self) -> crate::Result<()> {
        let mut item_properties_updated = self.proxy.receive_items_properties_updated().await?;
        let mut layout_updated = self.proxy.receive_layout_updated().await?;

        let proxy = self.proxy.clone();

        let event_tx = self.event_tx.clone();
        let owner_address = self.owner_address.clone();
        let handle = tokio::spawn(async move {
            // Players tell us about changes of the menu through either of these signals
            while tokio::select! {
                Some(_) = item_properties_updated.next() => true,
                Some(_) = layout_updated.next() => true,
                else => false,
            } {
                if let Ok((_, layout)) = proxy.get_layout(0, -1, &[]).await
                    && let Ok(items) = Self::parse_layout(layout)
                {
                    let _ = event_tx
                        .send(Event::MenuChanged(owner_address.clone(), items))
                        .await;
                }
            }
        });

        self.handles.lock().await.push(handle);
        Ok(())
    }

    /// Call this method when you are about to open the menu gui. without calling this the menu
    /// will not update with new contents
    pub async fn update_menu(&self) -> crate::Result<()> {
        self.proxy.about_to_show(0).await?;
        Ok(())
    }

    /// Activate an entry by id, this should be called when clicking the item in the menu gui.
    pub async fn activate(&self, id: i32) -> crate::Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

        self.proxy
            .event(id, "clicked", &Value::I32(0), timestamp.as_secs() as u32)
            .await?;

        Ok(())
    }

    pub async fn items(&self) -> crate::Result<Vec<MenuItem>> {
        let (_, layout) = self.proxy.get_layout(0, -1, &[]).await?;

        Self::parse_layout(layout)
    }

    fn parse_layout(layout: MenuLayout) -> crate::Result<Vec<MenuItem>> {
        let mut items = vec![];
        for inner in layout.submenus {
            // A single entry we can't make sense of shouldn't take the whole menu down with it
            if let Ok(item) = MenuItem::try_from(&inner) {
                items.push(item);
            }
        }

        Ok(items)
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        // Abort all handles when the menu item is dropped
        for handle in self.handles.get_mut().iter() {
            handle.abort();
        }
    }
}

/// Represents an item in the `Menu`.
#[derive(Default, Debug, Clone)]
pub struct MenuItem {
    pub id: i32,
    pub contents: MenuItemContents,
    /// Disabled items are shown, but can't be activated
    pub enabled: bool,
    /// The items of the submenu that opens from this item, if it has one
    pub children: Vec<MenuItem>,
}

/// Menu labels use `_` to mark the mnemonic of the next letter, and `__` for an actual `_`
fn strip_mnemonics(label: &str) -> String {
    let mut stripped = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '_' {
            if chars.peek() == Some(&'_') {
                chars.next();
                stripped.push('_');
            }
        } else {
            stripped.push(c);
        }
    }

    stripped
}

impl MenuItem {
    /// Parses an `(ia{sv}av)` structure, and (recursively) its children. `None` for items that
    /// shouldn't be displayed.
    fn from_structure(structure: &Structure, original: &OwnedValue) -> Result<Option<Self>, crate::Error> {
        let mut fields = structure.fields().iter();
        let mut menu_item = Self {
            enabled: true,
            ..Self::default()
        };

        if let Some(Value::I32(id)) = fields.next() {
            menu_item.id = *id;
        }

        if let Some(Value::Dict(dict)) = fields.next() {
            let visible = dict
                .get::<&str, bool>(&"visible")
                .ok()
                .flatten()
                .unwrap_or(true);

            if !visible {
                return Ok(None);
            }

            menu_item.enabled = dict
                .get::<&str, bool>(&"enabled")
                .ok()
                .flatten()
                .unwrap_or(true);

            let menu_type = dict
                .get::<&str, &str>(&"type")
                .ok()
                .flatten()
                .unwrap_or_default();

            let label = dict
                .get::<&str, &str>(&"label")?
                .map(strip_mnemonics);

            let toggle_state = dict
                .get::<&str, i32>(&"toggle-state")
                .ok()
                .flatten()
                .map(ToggleState::from)
                .unwrap_or_default();

            let toggle_type = dict
                .get::<&str, &str>(&"toggle-type")
                .ok()
                .flatten()
                .unwrap_or_default();

            menu_item.contents = match (menu_type, toggle_type, label) {
                ("separator", _, _) => MenuItemContents::Separator,
                (_, "checkmark", Some(label)) => {
                    MenuItemContents::CheckboxButton(label, toggle_state)
                }
                (_, "radio", Some(label)) => MenuItemContents::RadioButton(label, toggle_state),
                (_, _, Some(label)) => MenuItemContents::Label(label),
                _ => return Err(crate::Error::MenuItemParse(original.clone())),
            };
        }

        // The children are an array of variants, each holding another one of these structures
        if let Some(Value::Array(children)) = fields.next() {
            for child in children.iter() {
                let mut child = child;
                while let Value::Value(inner) = child {
                    child = inner;
                }

                // Same as for the menu as a whole: one entry we can't read is just left out
                if let Value::Structure(child) = child
                    && let Ok(Some(child)) = Self::from_structure(child, original)
                {
                    menu_item.children.push(child);
                }
            }
        }

        Ok(Some(menu_item))
    }
}

// Prase the `OwnedValue` into a menu item
impl TryFrom<&OwnedValue> for MenuItem {
    type Error = crate::Error;

    fn try_from(value: &OwnedValue) -> Result<Self, Self::Error> {
        let structure = value.downcast_ref::<&Structure>()?;

        // An invisible item has nothing to show, so it doesn't produce an item
        Self::from_structure(structure, value)?
            .ok_or_else(|| crate::Error::MenuItemParse(value.clone()))
    }
}

/// Represents the current contents of a `MenuItem`. Possible contents are:
/// * Regular text
/// * Text with radio button
/// * Text with checkbox
/// * Visual Separator
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItemContents {
    Label(String),
    RadioButton(String, ToggleState),
    CheckboxButton(String, ToggleState),
    Separator,
}

impl Default for MenuItemContents {
    fn default() -> Self {
        Self::Label(String::new())
    }
}

/// Represents the current state of a radio button or checkbox
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToggleState {
    #[default]
    On,
    Off,
    NonInteractive,
}

impl From<i32> for ToggleState {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::On,
            _ => Self::NonInteractive,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use zbus::zvariant::{Array, Dict, StructureBuilder};

    use super::*;

    fn item(id: i32, props: &[(&str, Value<'static>)], children: Vec<Value<'static>>) -> Value<'static> {
        let props: BTreeMap<String, Value> = props
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();

        let children: Vec<Value> = children
            .into_iter()
            .map(|child| Value::Value(Box::new(child)))
            .collect();

        Value::Structure(
            StructureBuilder::new()
                .add_field(id)
                .append_field(Value::Dict(Dict::from(props)))
                .append_field(Value::Array(Array::from(children)))
                .build()
                .unwrap(),
        )
    }

    fn parse(value: Value<'static>) -> crate::Result<MenuItem> {
        MenuItem::try_from(&OwnedValue::try_from(value).unwrap())
    }

    #[test]
    fn parses_nested_children() {
        let menu = parse(item(
            1,
            &[("label", Value::from("_Recent"))],
            vec![
                item(2, &[("label", Value::from("one"))], vec![]),
                item(3, &[("type", Value::from("separator"))], vec![]),
                item(4, &[("label", Value::from("two")), ("enabled", Value::from(false))], vec![]),
            ],
        ))
        .unwrap();

        assert_eq!(menu.contents, MenuItemContents::Label("Recent".into()));
        assert_eq!(menu.children.len(), 3);
        assert_eq!(menu.children[0].id, 2);
        assert_eq!(menu.children[1].contents, MenuItemContents::Separator);
        assert!(!menu.children[2].enabled);
    }

    #[test]
    fn leaves_out_invisible_and_unreadable_children() {
        let menu = parse(item(
            1,
            &[("label", Value::from("Root"))],
            vec![
                item(2, &[("label", Value::from("hidden")), ("visible", Value::from(false))], vec![]),
                item(3, &[], vec![]),
                item(4, &[("label", Value::from("shown"))], vec![]),
            ],
        ))
        .unwrap();

        assert_eq!(menu.children.len(), 1);
        assert_eq!(menu.children[0].id, 4);
    }

    #[test]
    fn mnemonics() {
        assert_eq!(strip_mnemonics("_File"), "File");
        assert_eq!(strip_mnemonics("snake__case"), "snake_case");
    }
}
