//! The `org.kde.StatusNotifierWatcher` service.
//!
//! Applications register their tray icon (a `StatusNotifierItem`) with the watcher, and tray
//! hosts ask it for the registered items. Nothing else on a bare system provides this service,
//! so `Tray` serves it itself, unless another watcher already owns the name.
use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::Mutex;
use zbus::{
    Connection, fdo,
    fdo::RequestNameFlags,
    interface,
    message::Header,
    object_server::SignalEmitter,
};

use crate::parse_address;

pub const WATCHER_BUS: &str = "org.kde.StatusNotifierWatcher";
pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";

#[derive(Debug, Default)]
struct State {
    /// Addresses (`destination/path`, see `parse_address`) of the registered items
    items: Vec<String>,
    /// Bus names of the registered hosts
    hosts: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Watcher {
    state: Arc<Mutex<State>>,
}

/// Turns what an item registered with into an address that `parse_address` understands.
///
/// Items register either with their bus name, or (mostly older ones) with just an object path,
/// in which case the bus name is that of whoever sent the registration.
fn item_address(service: &str, sender: Option<&str>) -> fdo::Result<String> {
    if service.starts_with('/') {
        let sender = sender
            .ok_or_else(|| fdo::Error::Failed("Registration has no sender".to_string()))?;

        Ok(format!("{sender}{service}"))
    } else if service.is_empty() {
        Err(fdo::Error::InvalidArgs("Empty service name".to_string()))
    } else {
        Ok(service.to_string())
    }
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let address = item_address(&service, header.sender().map(|s| s.as_str()))?;

        {
            let mut state = self.state.lock().await;

            if state.items.contains(&address) {
                return Ok(());
            }

            state.items.push(address.clone());
        }

        emitter.status_notifier_item_registered(&address).await?;
        self.registered_status_notifier_items_changed(&emitter)
            .await?;

        Ok(())
    }

    async fn register_status_notifier_host(
        &self,
        service: String,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        {
            let mut state = self.state.lock().await;

            if state.hosts.contains(&service) {
                return Ok(());
            }

            state.hosts.push(service);
        }

        emitter.status_notifier_host_registered().await?;
        self.is_status_notifier_host_registered_changed(&emitter)
            .await?;

        Ok(())
    }

    #[zbus(property)]
    async fn registered_status_notifier_items(&self) -> Vec<String> {
        self.state.lock().await.items.clone()
    }

    #[zbus(property)]
    async fn is_status_notifier_host_registered(&self) -> bool {
        !self.state.lock().await.hosts.is_empty()
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_unregistered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

impl Watcher {
    /// Starts serving the watcher on `connection`.
    ///
    /// Returns `false`, without serving anything, if another watcher is already running: the
    /// caller should then use that one as a client.
    ///
    /// # Errors
    /// Errors when the dbus connection fails
    pub async fn serve(connection: &Connection) -> crate::Result<bool> {
        let watcher = Self::default();
        let state = watcher.state.clone();

        connection.object_server().at(WATCHER_PATH, watcher).await?;

        let requested = connection
            .request_name_with_flags(WATCHER_BUS, RequestNameFlags::DoNotQueue.into())
            .await;

        match requested {
            Ok(_) => {}
            Err(zbus::Error::NameTaken) => {
                connection.object_server().remove::<Self, _>(WATCHER_PATH).await?;
                return Ok(false);
            }
            Err(err) => return Err(err.into()),
        }

        Self::watch_for_disappearances(connection, state).await?;

        Ok(true)
    }

    /// Items and hosts don't unregister when they quit (or crash), the bus just tells us that
    /// their name is gone.
    async fn watch_for_disappearances(
        connection: &Connection,
        state: Arc<Mutex<State>>,
    ) -> crate::Result<()> {
        let fdo_proxy = fdo::DBusProxy::new(connection).await?;
        let mut name_owner_changed = fdo_proxy.receive_name_owner_changed().await?;
        let interface = connection
            .object_server()
            .interface::<_, Self>(WATCHER_PATH)
            .await?;

        tokio::spawn(async move {
            while let Some(signal) = name_owner_changed.next().await {
                let Ok(args) = signal.args() else {
                    continue;
                };

                // Only interested in names that lost their owner
                if args.new_owner.is_some() {
                    continue;
                }

                let name = args.name.as_str();
                let emitter = interface.signal_emitter();

                let (removed_items, host_removed) = {
                    let mut state = state.lock().await;

                    let (removed, items): (Vec<_>, Vec<_>) = std::mem::take(&mut state.items)
                        .into_iter()
                        .partition(|address| parse_address(address).0 == name);
                    state.items = items;

                    let hosts_before = state.hosts.len();
                    state.hosts.retain(|host| host != name);

                    (removed, state.hosts.len() != hosts_before)
                };

                for address in &removed_items {
                    let _ = emitter.status_notifier_item_unregistered(address).await;
                }

                if !removed_items.is_empty() {
                    let _ = interface
                        .get()
                        .await
                        .registered_status_notifier_items_changed(emitter)
                        .await;
                }

                if host_removed {
                    let _ = emitter.status_notifier_host_unregistered().await;
                    let _ = interface
                        .get()
                        .await
                        .is_status_notifier_host_registered_changed(emitter)
                        .await;
                }
            }
        });

        Ok(())
    }
}
