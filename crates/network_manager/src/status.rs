/// How well the machine is connected, boiled down from NetworkManager's `NMState`:
/// <https://www.networkmanager.dev/docs/api/latest/nm-dbus-types.html#NMState>
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Not connected, asleep, or NetworkManager isn't running
    #[default]
    Disconnected,
    Connecting,
    /// Connected to a network, but without full internet access (e.g. a local-only or captive
    /// network).
    Limited,
    Connected,
}

impl From<u32> for ConnectionStatus {
    fn from(state: u32) -> Self {
        match state {
            40 => Self::Connecting,
            50 | 60 => Self::Limited,
            70 => Self::Connected,
            _ => Self::Disconnected,
        }
    }
}
