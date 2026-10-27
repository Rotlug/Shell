//! The system tray, backed by the `systray` crate.
//!
//! The crate serves the StatusNotifierWatcher and reports the tray items over a channel;
//! this owns it and fans its events out to whoever wants them.
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, OnceLock};
use async_broadcast::Receiver;
use systray::event::Event;
use systray::item::Item;
use systray::tray::Tray;

use crate::utils::broadcast::BroadcastChannel;

static CHANNEL: LazyLock<BroadcastChannel<Event>> = LazyLock::new(|| BroadcastChannel::new(32));
static TRAY: OnceLock<Tray> = OnceLock::new();

pub async fn activate() {
    let (tray, mut events) = match Tray::new().await {
        Ok(tray) => tray,
        Err(e) => return error!(%e, "Failed to start the system tray, tray items will not be available"),
    };

    if !tray.owns_watcher() {
        warn!("Another StatusNotifierWatcher is already running, using that one for the system tray");
    }

    if TRAY.set(tray).is_err() {
        return;
    }

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            CHANNEL.send(event).await;
        }
    });
}

/// Subscribes to tray events. Items that exist before subscribing are not announced,
/// get those from `items`.
pub fn subscribe() -> Receiver<Event> {
    CHANNEL.subscribe()
}

/// All items currently in the tray, by address.
pub async fn items() -> HashMap<String, Arc<Item>> {
    match TRAY.get() {
        Some(tray) => tray.items().await,
        None => HashMap::new(),
    }
}
