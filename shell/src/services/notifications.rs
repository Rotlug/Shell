//! The notification daemon, backed by the `notif_daemon` crate.
//!
//! The crate serves `org.freedesktop.Notifications` and reports what happens to the
//! notifications over a channel; this keeps track of the ones that are still around and fans
//! the events out to whoever wants them.
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, OnceLock, RwLock};
use async_broadcast::Receiver;
use notif_daemon::event::Event;
use notif_daemon::manager::Manager;
pub use notif_daemon::notification::{ClosingReason, Notification, Urgency};

use crate::utils::broadcast::BroadcastChannel;

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum BusEvent {
    NotificationAdded(Notification),
    NotificationUpdated(u32, Notification),
    NotificationClosed(u32),
}

static CHANNEL: LazyLock<BroadcastChannel<BusEvent>> = LazyLock::new(|| BroadcastChannel::new(10));

/// The notifications that haven't been closed yet, readable without awaiting anything.
static NOTIFICATIONS: LazyLock<RwLock<HashMap<u32, Notification>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

static MANAGER: OnceLock<Arc<Manager>> = OnceLock::new();

fn remember(notification: &Notification) {
    if let Ok(mut notifications) = NOTIFICATIONS.write() {
        notifications.insert(notification.id, notification.clone());
    }
}

/// Returns whether the notification was still around.
fn forget(id: u32) -> bool {
    NOTIFICATIONS.write().is_ok_and(|mut notifications| notifications.remove(&id).is_some())
}

pub async fn activate() {
    let (manager, mut events) = match Manager::new().await {
        Ok(manager) => manager,
        Err(e) => return error!(%e, "Failed to start the notification daemon, is another one running? Notifications will not be shown"),
    };

    if MANAGER.set(Arc::new(manager)).is_err() {
        return;
    }

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let event = match event {
                Event::Incoming(notification) => {
                    remember(&notification);
                    BusEvent::NotificationAdded(notification)
                },

                Event::Replaced(notification) => {
                    remember(&notification);
                    BusEvent::NotificationUpdated(notification.id, notification)
                },

                Event::Closed(id) => {
                    forget(id);
                    BusEvent::NotificationClosed(id)
                },
            };

            CHANNEL.send(event).await;
        }
    });
}

pub fn subscribe() -> Receiver<BusEvent> {
    CHANNEL.subscribe()
}

/// How many notifications are around.
pub fn count() -> usize {
    NOTIFICATIONS.read().map_or(0, |notifications| notifications.len())
}

/// Closes the notifications that are around, one after the other.
pub fn clear_notifications() {
    let mut ids: Vec<u32> = NOTIFICATIONS.read()
        .map(|notifications| notifications.keys().copied().collect())
        .unwrap_or_default();

    // Newest first
    ids.sort_unstable_by(|a, b| b.cmp(a));

    for (i, id) in ids.into_iter().enumerate() {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10 * i as u64)).await;
            close_notification_by_id(id, ClosingReason::Expired).await;
        });
    }
}

/// Closes a notification, telling the application that sent it and everyone listening here.
pub async fn close_notification_by_id(id: u32, reason: ClosingReason) {
    let Some(manager) = MANAGER.get() else {
        return;
    };

    match manager.close_notification(id, reason).await {
        Ok(Some(_)) => {
            forget(id);
            CHANNEL.send(BusEvent::NotificationClosed(id)).await;
        },
        Ok(None) => {},
        Err(e) => error!(%e, id, "Failed to close notification"),
    }
}

/// Tells the application that sent a notification that one of its actions was invoked.
pub async fn invoke_notification_action(id: u32, action_key: String) {
    let Some(manager) = MANAGER.get() else {
        return;
    };

    if let Err(e) = manager.invoke_action(id, action_key).await {
        error!(%e, id, "Failed to invoke notification action");
    }
}
