use crate::{Monitor, MonitorId};

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A monitor was found, either at the start or because it got connected.
    Added(Monitor),
    Removed(MonitorId),
    /// The brightness of a monitor changed, from 0.0 to 1.0. This includes changes that
    /// weren't made through this crate (brightness keys, the monitor's own buttons, ...).
    Brightness(MonitorId, f64),
}
