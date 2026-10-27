use futures::StreamExt;
use tokio::{sync::mpsc::Sender, task::JoinHandle};
use zbus::Connection;

use crate::{
    dbus::status_notifier_item::StatusNotifierItemProxy, event::Event, menu::Menu, parse_address,
};

/// What the item's menu property says when it doesn't have one
const NO_MENU_PATH: &str = "/NO_DBUSMENU";

#[derive(Debug)]
pub struct Item {
    proxy: StatusNotifierItemProxy<'static>,
    handles: Vec<JoinHandle<()>>,
    menu: Option<Menu>,
}

impl Item {
    /// Connects to the item at `address` (see `parse_address`), and starts reporting its
    /// changes to `event_tx`.
    ///
    /// # Errors
    /// Errors when the item can't be reached
    pub async fn new(
        connection: Connection,
        address: &str,
        event_tx: Sender<Event>,
    ) -> crate::Result<Self> {
        let (destination, path) = parse_address(address);
        let proxy = StatusNotifierItemProxy::builder(&connection)
            .destination(destination.to_owned())?
            .path(path)?
            .build()
            .await?;

        // A menu is optional, items without one are still worth showing
        let menu = match proxy.menu().await {
            Ok(path) if path.as_str() != "/" && path.as_str() != NO_MENU_PATH => {
                Menu::new(
                    connection.clone(),
                    address.to_owned(),
                    destination,
                    path,
                    event_tx.clone(),
                )
                .await
                .ok()
            }
            _ => None,
        };

        if let Some(menu) = &menu {
            let _ = menu.watch_updates().await;
        }

        let handles = vec![Self::watch_signals(&proxy, address.to_string(), event_tx).await?];

        Ok(Self {
            proxy,
            handles,
            menu,
        })
    }

    /// Spawns the task that reports the changes the item announces
    async fn watch_signals(
        proxy: &StatusNotifierItemProxy<'static>,
        address: String,
        event_tx: Sender<Event>,
    ) -> crate::Result<JoinHandle<()>> {
        let mut new_icon = proxy.receive_new_icon().await?;
        let mut new_attention_icon = proxy.receive_new_attention_icon().await?;
        let mut new_overlay_icon = proxy.receive_new_overlay_icon().await?;
        let mut new_title = proxy.receive_new_title().await?;
        let mut new_status = proxy.receive_new_status().await?;
        let mut new_tool_tip = proxy.receive_new_tool_tip().await?;

        Ok(tokio::spawn(async move {
            loop {
                let event = tokio::select! {
                    Some(_) = new_icon.next() => Event::IconChanged(address.clone()),
                    Some(_) = new_attention_icon.next() => Event::IconChanged(address.clone()),
                    Some(_) = new_overlay_icon.next() => Event::IconChanged(address.clone()),
                    Some(_) = new_title.next() => Event::TitleChanged(address.clone()),
                    Some(_) = new_tool_tip.next() => Event::ToolTipChanged(address.clone()),
                    Some(signal) = new_status.next() => {
                        let Ok(args) = signal.args() else {
                            continue;
                        };

                        Event::StatusChanged(address.clone(), args.status.to_string())
                    }
                    else => break,
                };

                if event_tx.send(event).await.is_err() {
                    break;
                }
            }
        }))
    }

    /// Finds the icon of this item. First trying to get a system icon, then falling back to a
    /// pixmap if available. If both options fail, returns `None`.
    pub async fn icon(&self, size: i32) -> Option<Icon> {
        let system_icon = self.proxy.icon_name().await.ok();
        if let Some(icon) = system_icon
            && !icon.is_empty()
        {
            return Some(Icon::System(icon));
        }

        // System icon not found, trying pixmap
        let pixmap = self
            .proxy
            .icon_pixmap()
            .await
            .ok()
            .filter(|v| !v.is_empty())?;

        let heights: Vec<i32> = pixmap.iter().map(|(_, h, _)| *h).collect();
        let best_height = Icon::best_fit(&heights, size);
        let best_icon = pixmap.iter().find(|(_, h, _)| *h == best_height)?;

        Some(Icon::Pixmap {
            width: best_icon.0,
            height: best_icon.1,
            pixels: best_icon.2.clone(),
        })
    }

    /// Extra directory to look for system icons in, if the item has one.
    pub async fn icon_theme_path(&self) -> Option<String> {
        self.proxy
            .icon_theme_path()
            .await
            .ok()
            .filter(|path| !path.is_empty())
    }

    /// The right-click menu, for items that have one.
    pub const fn menu(&self) -> Option<&Menu> {
        self.menu.as_ref()
    }

    pub async fn title(&self) -> crate::Result<String> {
        let title = self.proxy.title().await?;
        Ok(title)
    }

    /// `Passive`, `Active` or `NeedsAttention`. Passive items are usually not shown.
    pub async fn status(&self) -> crate::Result<String> {
        let status = self.proxy.status().await?;
        Ok(status)
    }

    pub async fn tool_tip(&self) -> crate::Result<ToolTip> {
        let (icon_name, _, title, description) = self.proxy.tool_tip().await?;

        Ok(ToolTip {
            icon_name,
            title,
            description,
        })
    }

    /// Primary action, usually left click.
    pub async fn activate(&self) -> crate::Result<()> {
        self.proxy.activate(0, 0).await?;
        Ok(())
    }

    /// Secondary action, usually middle click.
    pub async fn secondary_activate(&self) -> crate::Result<()> {
        self.proxy.secondary_activate(0, 0).await?;
        Ok(())
    }

    /// Asks the item to show its own menu, for items that don't provide a `menu`.
    pub async fn context_menu(&self, x: i32, y: i32) -> crate::Result<()> {
        self.proxy.context_menu(x, y).await?;
        Ok(())
    }

    pub async fn scroll(&self, delta: i32, orientation: Orientation) -> crate::Result<()> {
        self.proxy
            .scroll(
                delta,
                match orientation {
                    Orientation::Vertical => "vertical",
                    Orientation::Horizontal => "horizontal",
                },
            )
            .await?;
        Ok(())
    }
}

impl Drop for Item {
    fn drop(&mut self) {
        for handle in &self.handles {
            handle.abort();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolTip {
    pub icon_name: String,
    pub title: String,
    pub description: String,
}

/// Represents an icon in the system tray, which can have two types:
/// * A System icon, which is a string representing an icon from the system's icon pack. e.g.
///   `"discord"` or `"vlc"`.
/// * A Pixmap icon, which is represented using a width, an height, and a `Vec` of bytes in ARGB
///   format.
///
/// When looking for the icon of an `Item`, the System icon takes priority.
#[derive(Debug)]
pub enum Icon {
    System(String),
    Pixmap {
        width: i32,
        height: i32,
        pixels: Vec<u8>,
    },
}

impl Icon {
    /// Find the closest number to `size` in a list of `options`.
    /// This is useful when trying to find the best size to display an icon when multiple sized
    /// pixmaps are available to use.
    #[must_use]
    pub fn best_fit(options: &[i32], size: i32) -> i32 {
        let mut best_fit = (0, u32::MAX);

        for option in options {
            let diff = size.abs_diff(*option);
            if diff < best_fit.1 {
                best_fit.0 = *option;
                best_fit.1 = diff;
            }
        }

        best_fit.0
    }
}
