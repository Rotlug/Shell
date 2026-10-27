use std::sync::LazyLock;
use std::time::Duration;
use futures_signals::signal::Mutable;

use super::bus::{NM_BUS_NAME, NM_MANAGER_PATH};
use super::proxy::manager::OrgFreedesktopNetworkManager;

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);

/// https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMState
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    /// Connected to a network, but without full internet access (e.g. a local-only or captive network).
    Limited,
    Connected,
}

pub static CONNECTION_STATUS: LazyLock<Mutable<ConnectionStatus>> = LazyLock::new(|| Mutable::new(ConnectionStatus::default()));

fn state_to_status(state: u32) -> ConnectionStatus {
    match state {
        40 => ConnectionStatus::Connecting,
        50 | 60 => ConnectionStatus::Limited,
        70 => ConnectionStatus::Connected,
        _ => ConnectionStatus::Disconnected,
    }
}

pub fn activate() {
    std::thread::spawn(|| {
        loop {
            let status = dbus::blocking::Connection::new_system().ok().and_then(|connection| {
                let proxy = connection.with_proxy(NM_BUS_NAME, NM_MANAGER_PATH, Duration::from_millis(2000));
                proxy.state_().ok()
            }).map_or(ConnectionStatus::Disconnected, state_to_status);

            CONNECTION_STATUS.set(status);
            std::thread::sleep(REFRESH_INTERVAL);
        }
    });
}
