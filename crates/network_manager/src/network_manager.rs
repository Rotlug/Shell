use futures::StreamExt;
use tokio::{
    sync::mpsc::{Receiver, channel},
    task::JoinHandle,
};
use zbus::{Connection, fdo, proxy::CacheProperties};

use crate::{
    ConnectionStatus,
    dbus::{BUS_NAME, OBJECT_PATH, NetworkManagerDaemonProxy},
    event::Event,
};

/// A connection to NetworkManager, used to learn about the state of the network.
pub struct NetworkManager {
    proxy: NetworkManagerDaemonProxy<'static>,
    handle: JoinHandle<()>,
}

impl NetworkManager {
    /// Connects to NetworkManager on the system bus. Returns a `Receiver` which can be used to
    /// listen to the connection status.
    ///
    /// NetworkManager doesn't have to be running (yet), the status is `Disconnected` until it is.
    ///
    /// # Errors
    /// Errors when the dbus connection fails
    pub async fn new() -> crate::Result<(Self, Receiver<Event>)> {
        Self::with_connection(Connection::system().await?).await
    }

    /// Same as `new`, on a connection of your own.
    ///
    /// # Errors
    /// Errors when the dbus connection fails
    pub async fn with_connection(connection: Connection) -> crate::Result<(Self, Receiver<Event>)> {
        let (event_tx, event_rx) = channel(16);

        // NetworkManager may come and go, so don't cache what it says
        let proxy = NetworkManagerDaemonProxy::builder(&connection)
            .cache_properties(CacheProperties::No)
            .build()
            .await?;

        // The typed property streams rely on the cache, which would go stale if NetworkManager restarts
        let mut properties_changed = fdo::PropertiesProxy::builder(&connection)
            .destination(BUS_NAME)?
            .path(OBJECT_PATH)?
            .build()
            .await?
            .receive_properties_changed()
            .await?;
        let mut name_owner_changed = fdo::DBusProxy::new(&connection)
            .await?
            .receive_name_owner_changed_with_args(&[(0, BUS_NAME)])
            .await?;

        let proxy_task = proxy.clone();
        let handle = tokio::spawn(async move {
            let mut last = None;

            loop {
                let current = Self::read_status(&proxy_task).await;

                if last != Some(current) {
                    last = Some(current);

                    if event_tx.send(Event::Status(current)).await.is_err() {
                        break;
                    }
                }

                tokio::select! {
                    Some(_) = properties_changed.next() => {},
                    Some(_) = name_owner_changed.next() => {},
                    else => break,
                }
            }
        });

        Ok((Self { proxy, handle }, event_rx))
    }

    async fn read_status(proxy: &NetworkManagerDaemonProxy<'_>) -> ConnectionStatus {
        proxy
            .state()
            .await
            .map_or(ConnectionStatus::Disconnected, ConnectionStatus::from)
    }

    /// The connection status right now.
    pub async fn status(&self) -> ConnectionStatus {
        Self::read_status(&self.proxy).await
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
