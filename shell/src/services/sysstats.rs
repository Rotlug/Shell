//! Stats of the system, backed by the `system_stats` crate.
use std::sync::LazyLock;
use futures_signals::signal::Mutable;
use system_stats::{Event, SystemStats};

pub use system_stats::MemoryInfo;

pub static SYS_STATS: LazyLock<SysStats> = LazyLock::new(SysStats::default);

#[derive(Default)]
pub struct SysStats {
    pub uptime: Mutable<u64>,
    pub memory: Mutable<MemoryInfo>,
    pub swap: Mutable<MemoryInfo>,
    pub global_cpu_usage: Mutable<f64>,
    pub cpu_temperature: Mutable<f64>,
    pub gpu_memory: Mutable<MemoryInfo>,
    pub battery_percentage: Mutable<f64>,
    pub battery_charging: Mutable<bool>,
}

impl SysStats {
    fn apply(&self, event: Event) {
        match event {
            Event::CpuUsage(usage) => self.global_cpu_usage.set(usage),
            Event::Memory(memory) => self.memory.set(memory),
            Event::Swap(swap) => self.swap.set(swap),
            Event::Uptime(uptime) => self.uptime.set(uptime.as_secs()),
            Event::CpuTemperature(temperature) => self.cpu_temperature.set(temperature),
            // TODO: Add support for other GPU vendors
            Event::GpuMemory(memory) => self.gpu_memory.set(memory),
            Event::Battery(battery) => {
                self.battery_percentage.set(battery.percentage);
                self.battery_charging.set(battery.charging);
            },
        }
    }
}

pub fn activate() {
    let (stats, mut events) = match SystemStats::builder().all().build() {
        Ok(stats) => stats,
        Err(e) => return error!(%e, "Failed to start reading the system stats"),
    };

    tokio::spawn(async move {
        // The stats are only read for as long as this is around
        let _stats = stats;

        while let Some(event) = events.recv().await {
            SYS_STATS.apply(event);
        }
    });
}
