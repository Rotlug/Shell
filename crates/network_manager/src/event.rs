use crate::ConnectionStatus;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The connection status changed. Sent once with the initial status as well.
    Status(ConnectionStatus),
}
