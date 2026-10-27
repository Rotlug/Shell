use std::fmt;

/// Identifies a monitor for as long as it stays connected.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MonitorId(pub(crate) String);

impl MonitorId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MonitorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorKind {
    /// The panel of a laptop
    Internal,
    /// A monitor that is connected to the machine
    External,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    pub id: MonitorId,
    pub kind: MonitorKind,
    /// How the monitor is connected, e.g. `eDP-1` or `DP-2`, if that is known.
    pub connector: Option<String>,
    /// The model of the monitor, e.g. `DELL U2720Q`.
    pub name: String,
    /// From 0.0 to 1.0
    pub brightness: f64,
}
