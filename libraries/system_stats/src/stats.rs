use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryInfo {
    /// In bytes
    pub total: u64,
    /// In bytes
    pub used: u64,
}

impl MemoryInfo {
    /// How much of the memory is used, from 0 to 100.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn usage_percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64) * 100.0
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BatteryInfo {
    /// From 0 to 100
    pub percentage: f64,
    pub charging: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Usage of all CPU cores together, from 0 to 100
    CpuUsage(f64),
    Memory(MemoryInfo),
    Swap(MemoryInfo),
    Uptime(Duration),
    /// In degrees Celsius
    CpuTemperature(f64),
    #[cfg(feature = "nvidia")]
    GpuMemory(MemoryInfo),
    Battery(BatteryInfo),
}

/// The latest value of each stat. A stat is `None` until it has been read, and stays that way
/// if it wasn't asked for or isn't available on this machine (no battery, no sensor, ...).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub cpu_usage: Option<f64>,
    pub memory: Option<MemoryInfo>,
    pub swap: Option<MemoryInfo>,
    pub uptime: Option<Duration>,
    pub cpu_temperature: Option<f64>,
    #[cfg(feature = "nvidia")]
    pub gpu_memory: Option<MemoryInfo>,
    pub battery: Option<BatteryInfo>,
}

impl Snapshot {
    /// Whether `event` has something new to say.
    pub(crate) fn is_new(&self, event: &Event) -> bool {
        match *event {
            Event::CpuUsage(v) => self.cpu_usage != Some(v),
            Event::Memory(v) => self.memory != Some(v),
            Event::Swap(v) => self.swap != Some(v),
            Event::Uptime(v) => self.uptime != Some(v),
            Event::CpuTemperature(v) => self.cpu_temperature != Some(v),
            #[cfg(feature = "nvidia")]
            Event::GpuMemory(v) => self.gpu_memory != Some(v),
            Event::Battery(v) => self.battery != Some(v),
        }
    }

    pub(crate) fn apply(&mut self, event: Event) {
        match event {
            Event::CpuUsage(v) => self.cpu_usage = Some(v),
            Event::Memory(v) => self.memory = Some(v),
            Event::Swap(v) => self.swap = Some(v),
            Event::Uptime(v) => self.uptime = Some(v),
            Event::CpuTemperature(v) => self.cpu_temperature = Some(v),
            #[cfg(feature = "nvidia")]
            Event::GpuMemory(v) => self.gpu_memory = Some(v),
            Event::Battery(v) => self.battery = Some(v),
        }
    }
}
