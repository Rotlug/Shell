use zbus::proxy;

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
pub trait NetworkManagerDaemon {
    /// State property: the `NMState` of the whole system
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
}

pub const BUS_NAME: &str = "org.freedesktop.NetworkManager";
pub const OBJECT_PATH: &str = "/org/freedesktop/NetworkManager";
