use crate::notification::Notification;

#[derive(Debug)]
pub enum Event {
    /// A new notification arrived.
    Incoming(Notification),
    /// A notification that is still being shown was replaced by a new version of it (same id).
    Replaced(Notification),
    /// A notification was closed by the sender, through `CloseNotification`.
    Closed(u32),
}
