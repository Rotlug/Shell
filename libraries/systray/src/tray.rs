use std::{collections::HashMap, sync::Arc};

use futures::StreamExt;
use tokio::sync::{
    Mutex,
    mpsc::{Receiver, Sender, channel},
};
use zbus::Connection;

use crate::{
    dbus::status_notifier_watcher::{
        StatusNotifierItemRegistered, StatusNotifierItemUnregistered, StatusNotifierWatcherProxy,
    },
    event::Event,
    item::Item,
    watcher::{WATCHER_PATH, Watcher},
};

type Items = Arc<Mutex<HashMap<String, Arc<Item>>>>;

/// The system tray host. Applications register their tray icons with the
/// `org.kde.StatusNotifierWatcher` service, which `Tray` provides itself unless another watcher
/// is already running, and `Tray` reports them as they come and go.
pub struct Tray {
    _connection: Connection,
    items: Items,
    owns_watcher: bool,
}

impl Tray {
    /// Starts the tray: serves the watcher service (if nothing else does), and registers as a
    /// host. Returns a `Receiver` which can be used to listen to the items and their updates.
    ///
    /// # Errors
    /// Errors when the connection to dbus fails
    pub async fn new() -> crate::Result<(Self, Receiver<Event>)> {
        let connection = zbus::Connection::session().await?;
        let items = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, event_rx) = channel(16);

        let owns_watcher = Watcher::serve(&connection).await?;

        // Signals
        let watcher_proxy = StatusNotifierWatcherProxy::builder(&connection)
            .path(WATCHER_PATH)?
            .build()
            .await?;

        let mut item_registered = watcher_proxy
            .receive_status_notifier_item_registered()
            .await?;
        let mut item_unregistered = watcher_proxy
            .receive_status_notifier_item_unregistered()
            .await?;

        let name = connection
            .unique_name()
            .map(|n| n.as_str())
            .unwrap_or_default();

        // Register myself as the host
        watcher_proxy.register_status_notifier_host(name).await?;

        let conn_task = connection.clone();
        let items_task = items.clone();
        let event_tx_task = event_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(event) = item_registered.next() => {
                        handle_item_registered(
                            event,
                            items_task.clone(),
                            conn_task.clone(),
                            event_tx_task.clone(),
                        ).await;
                    }
                    Some(event) = item_unregistered.next() => {
                        handle_item_unregistered(event, items_task.clone(), event_tx_task.clone()).await;
                    }
                    else => break,
                }
            }
        });

        // Get all system tray items that were present before this method was called, so we
        // can be in sync.
        let addresses = watcher_proxy.registered_status_notifier_items().await?;
        for address in addresses {
            add_item(&address, &items, &connection, &event_tx).await;
        }

        Ok((
            Self {
                _connection: connection,
                items,
                owns_watcher,
            },
            event_rx,
        ))
    }

    /// Whether this `Tray` provides the watcher service, as opposed to using one that was
    /// already running.
    #[must_use]
    pub const fn owns_watcher(&self) -> bool {
        self.owns_watcher
    }

    /// All items currently in the tray, by address.
    pub async fn items(&self) -> HashMap<String, Arc<Item>> {
        self.items.lock().await.clone()
    }
}

/// Connects to the item at `address` and announces it. Items that fail to load (they may be
/// buggy, or gone by now) are skipped.
async fn add_item(
    address: &str,
    items: &Items,
    connection: &Connection,
    event_tx: &Sender<Event>,
) {
    if items.lock().await.contains_key(address) {
        return;
    }

    let Ok(item) = Item::new(connection.clone(), address, event_tx.clone()).await else {
        return;
    };

    let item = Arc::new(item);

    items.lock().await.insert(address.to_string(), item.clone());
    let _ = event_tx
        .send(Event::ItemAdded(address.to_string(), item))
        .await;
}

async fn handle_item_registered(
    event: StatusNotifierItemRegistered,
    items: Items,
    connection: Connection,
    event_tx: Sender<Event>,
) {
    if let Ok(args) = event.args() {
        add_item(args.arg_1, &items, &connection, &event_tx).await;
    }
}

async fn handle_item_unregistered(
    event: StatusNotifierItemUnregistered,
    items: Items,
    event_tx: Sender<Event>,
) {
    let address = event.args().map(|args| args.arg_1);

    if let Ok(address) = address
        && items.lock().await.remove(address).is_some()
    {
        let _ = event_tx.send(Event::ItemRemoved(address.to_string())).await;
    }
}
