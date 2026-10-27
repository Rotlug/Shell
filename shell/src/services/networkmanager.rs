//! Network connectivity, backed by the `network_manager` crate.
use std::sync::LazyLock;
use futures_signals::signal::Mutable;
use network_manager::NetworkManager;
use network_manager::event::Event;

pub use network_manager::ConnectionStatus;

pub static CONNECTION_STATUS: LazyLock<Mutable<ConnectionStatus>> = LazyLock::new(|| Mutable::new(ConnectionStatus::default()));

pub async fn activate() {
    let (network_manager, mut events) = match NetworkManager::new().await {
        Ok(network_manager) => network_manager,
        Err(e) => return error!(%e, "Failed to connect to the system bus, the network status will not be available"),
    };

    tokio::spawn(async move {
        // Only reading events is not enough to keep the connection to NetworkManager going
        let _network_manager = network_manager;

        while let Some(Event::Status(status)) = events.recv().await {
            CONNECTION_STATUS.set(status);
        }
    });
}
