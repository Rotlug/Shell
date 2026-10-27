use std::{collections::HashMap, sync::Arc};

use tokio::sync::{
    Mutex,
    mpsc::{Receiver, Sender, channel},
};
use zbus::{
    Connection, fdo::RequestNameFlags, object_server::SignalEmitter,
};

use crate::{
    daemon::{Daemon, DaemonSignals},
    event::Event,
    notification::{ClosingReason, Notification},
};

#[derive(Debug)]
pub struct Manager {
    notifications: Arc<Mutex<HashMap<u32, Notification>>>,
    signal_emitter: SignalEmitter<'static>,
    _connection: Connection,
}

impl Manager {
    /// Registers the notification manager and returns a `Receiver` which can be used to listen
    /// to incoming notifications and other events.
    ///
    /// # Errors
    /// Errors when the connection to dbus fails - Which may happen if a different notification
    /// daemon is in use.
    pub async fn new() -> crate::Result<(Self, Receiver<Event>)> {
        // Communication between the `Daemon` and the `Manager`
        let (inner_tx, mut inner_rx) = channel(16);
        // Communication between the `Manager` and the caller
        let (outer_tx, outer_rx) = channel(16);

        let notifications = Arc::new(Mutex::new(HashMap::new()));

        let connection = Connection::session().await?;

        // Daemon
        Self::connect_daemon(&connection, inner_tx.clone()).await?;

        let object_server = connection
            .object_server()
            .interface::<_, Daemon>("/org/freedesktop/Notifications")
            .await?;

        let signal_emitter = object_server.signal_emitter().to_owned();

        let notifications_task = notifications.clone();
        let signal_emitter_task = signal_emitter.clone();

        tokio::spawn(async move {
            while let Some(event) = inner_rx.recv().await {
                let mut notifs = notifications_task.lock().await;

                let event = match event {
                    Event::Incoming(n) | Event::Replaced(n) => {
                        let replaced = notifs.insert(n.id, n.clone()).is_some();

                        if replaced {
                            Event::Replaced(n)
                        } else {
                            Event::Incoming(n)
                        }
                    }
                    Event::Closed(id) => {
                        // Closing something that is already gone is not worth reporting
                        if notifs.remove(&id).is_none() {
                            continue;
                        }

                        let _ = signal_emitter_task
                            .notification_closed(id, ClosingReason::Inner)
                            .await;

                        Event::Closed(id)
                    }
                };

                drop(notifs);
                let _ = outer_tx.send(event).await;
            }
        });

        Ok((
            Self {
                _connection: connection,
                notifications,
                signal_emitter,
            },
            outer_rx,
        ))
    }

    /// Registers as the systems notification daemon.
    ///
    /// # Errors
    /// Errors when the connection fails (for example, when another notification daemon is
    /// running.)
    async fn connect_daemon(connection: &Connection, event_tx: Sender<Event>) -> crate::Result<()> {
        let daemon = Daemon::new(event_tx);

        connection
            .object_server()
            .at("/org/freedesktop/Notifications", daemon)
            .await?;

        // Don't queue up behind another daemon, that would just silently never receive anything
        connection
            .request_name_with_flags(
                "org.freedesktop.Notifications",
                RequestNameFlags::DoNotQueue.into(),
            )
            .await?;

        Ok(())
    }

    /// Returns all notifications that haven't been closed yet, oldest first.
    pub async fn get_all(&self) -> Vec<Notification> {
        let mut all: Vec<Notification> = self.notifications.lock().await.values().cloned().collect();
        all.sort_by_key(|n| n.id);
        all
    }

    /// Returns a `Notification` given its id, or `None` if none is found.
    pub async fn get_by_id(&self, id: u32) -> Option<Notification> {
        self.notifications.lock().await.get(&id).cloned()
    }

    /// A `Notification` can have a list of actions (corresponding to buttons the user can
    /// click on the client). This method can invoke an action on a notification with a certain
    /// id.
    ///
    /// # Errors
    /// Errors when the dbus signal fails to be emitted
    pub async fn invoke_action(&self, id: u32, action_key: String) -> crate::Result<()> {
        self.signal_emitter.action_invoked(id, action_key).await?;

        Ok(())
    }

    /// Closes a notification with a certain id.
    ///
    /// # Errors
    /// Errors when the dbus signal fails to be emitted
    pub async fn close_notification(
        &self,
        id: u32,
        reason: ClosingReason,
    ) -> crate::Result<Option<Notification>> {
        let removed = self.notifications.lock().await.remove(&id);

        if removed.is_some() {
            self.signal_emitter.notification_closed(id, reason).await?;
        }

        Ok(removed)
    }
}
