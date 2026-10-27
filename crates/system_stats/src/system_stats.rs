use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, Thread},
    time::Duration,
};

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tokio::sync::mpsc::{Receiver, Sender, channel, error::TrySendError};

use crate::{Event, Snapshot, battery, cpu_temperature::CpuTemperature};
#[cfg(feature = "nvidia")]
use crate::nvidia::Nvidia;

const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);

/// Which stats to read.
#[derive(Clone, Copy, Debug, Default)]
struct Selection {
    cpu_usage: bool,
    memory: bool,
    swap: bool,
    uptime: bool,
    cpu_temperature: bool,
    #[cfg(feature = "nvidia")]
    gpu_memory: bool,
    battery: bool,
}

/// Chooses the stats to read, see [`SystemStats::builder`].
#[derive(Clone, Copy, Debug)]
pub struct SystemStatsBuilder {
    interval: Duration,
    selection: Selection,
}

impl SystemStatsBuilder {
    /// How often the stats are read, one second by default.
    #[must_use]
    pub const fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Usage of all CPU cores together.
    #[must_use]
    pub const fn cpu_usage(mut self) -> Self {
        self.selection.cpu_usage = true;
        self
    }

    #[must_use]
    pub const fn memory(mut self) -> Self {
        self.selection.memory = true;
        self
    }

    #[must_use]
    pub const fn swap(mut self) -> Self {
        self.selection.swap = true;
        self
    }

    #[must_use]
    pub const fn uptime(mut self) -> Self {
        self.selection.uptime = true;
        self
    }

    /// Temperature of the CPU, if this machine has a sensor for it.
    #[must_use]
    pub const fn cpu_temperature(mut self) -> Self {
        self.selection.cpu_temperature = true;
        self
    }

    /// Memory of the first NVIDIA GPU, if there is one.
    #[cfg(feature = "nvidia")]
    #[must_use]
    pub const fn gpu_memory(mut self) -> Self {
        self.selection.gpu_memory = true;
        self
    }

    /// Charge of the first battery, if there is one.
    #[must_use]
    pub const fn battery(mut self) -> Self {
        self.selection.battery = true;
        self
    }

    /// Every stat there is.
    #[must_use]
    pub const fn all(self) -> Self {
        let builder = self.cpu_usage().memory().swap().uptime().cpu_temperature().battery();

        #[cfg(feature = "nvidia")]
        let builder = builder.gpu_memory();

        builder
    }

    /// Starts reading the stats. Returns a `Receiver` which can be used to listen to them: an
    /// event is sent for every stat as soon as it is read, and after that whenever it changes.
    ///
    /// Stats that aren't available on this machine (no battery, no sensor, ...) send nothing.
    ///
    /// # Errors
    /// Errors when the thread that reads the stats can't be started.
    pub fn build(self) -> crate::Result<(SystemStats, Receiver<Event>)> {
        let (event_tx, event_rx) = channel(32);
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));

        let poller = Poller {
            selection: self.selection,
            interval: self.interval,
            event_tx,
            snapshot: snapshot.clone(),
            stop: stop.clone(),
        };

        let thread = thread::Builder::new()
            .name("system_stats".to_string())
            .spawn(move || poller.run())?
            .thread()
            .clone();

        Ok((SystemStats { snapshot, stop, thread }, event_rx))
    }
}

/// Reads stats of the machine in the background, for as long as it is alive.
pub struct SystemStats {
    snapshot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    thread: Thread,
}

impl SystemStats {
    #[must_use]
    pub const fn builder() -> SystemStatsBuilder {
        SystemStatsBuilder {
            interval: DEFAULT_INTERVAL,
            selection: Selection {
                cpu_usage: false,
                memory: false,
                swap: false,
                uptime: false,
                cpu_temperature: false,
                #[cfg(feature = "nvidia")]
                gpu_memory: false,
                battery: false,
            },
        }
    }

    /// The latest value of each stat.
    ///
    /// # Panics
    /// Panics if the thread that reads the stats panicked.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        *self.snapshot.lock().unwrap()
    }
}

impl Drop for SystemStats {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.unpark();
    }
}

struct Poller {
    selection: Selection,
    interval: Duration,
    event_tx: Sender<Event>,
    snapshot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
}

impl Poller {
    fn run(self) {
        let selection = self.selection;

        let mut refresh = RefreshKind::nothing();
        if selection.memory || selection.swap {
            refresh = refresh.with_memory(MemoryRefreshKind::nothing().with_ram().with_swap());
        }
        if selection.cpu_usage {
            refresh = refresh.with_cpu(CpuRefreshKind::nothing().with_cpu_usage());
        }

        let mut system = System::new_with_specifics(refresh);
        let mut cpu_temperature = CpuTemperature::default();
        #[cfg(feature = "nvidia")]
        let nvidia = if selection.gpu_memory { Nvidia::new() } else { None };

        if selection.cpu_usage {
            // The usage is measured between two refreshes, so the first one doesn't tell much
            system.refresh_cpu_usage();
            thread::park_timeout(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        }

        while !self.stop.load(Ordering::Relaxed) {
            if selection.memory || selection.swap {
                system.refresh_memory();
            }

            if selection.cpu_usage {
                system.refresh_cpu_usage();

                if !self.send(Event::CpuUsage(f64::from(system.global_cpu_usage()))) {
                    return;
                }
            }

            let mut events = Vec::new();

            if selection.memory {
                events.push(Event::Memory(crate::MemoryInfo {
                    total: system.total_memory(),
                    used: system.used_memory(),
                }));
            }

            if selection.swap {
                events.push(Event::Swap(crate::MemoryInfo {
                    total: system.total_swap(),
                    used: system.used_swap(),
                }));
            }

            if selection.uptime {
                events.push(Event::Uptime(Duration::from_secs(System::uptime())));
            }

            if selection.cpu_temperature
                && let Some(temperature) = cpu_temperature.read()
            {
                events.push(Event::CpuTemperature(temperature));
            }

            #[cfg(feature = "nvidia")]
            if let Some(memory) = nvidia.as_ref().and_then(Nvidia::memory) {
                events.push(Event::GpuMemory(memory));
            }

            if selection.battery
                && let Some(battery) = battery::read()
            {
                events.push(Event::Battery(battery));
            }

            for event in events {
                if !self.send(event) {
                    return;
                }
            }

            thread::park_timeout(self.interval);
        }
    }

    /// Sends the event if it tells something new. Returns `false` once nobody is listening.
    fn send(&self, event: Event) -> bool {
        if !self.snapshot.lock().unwrap().is_new(&event) {
            return true;
        }

        match self.event_tx.try_send(event) {
            Ok(()) => self.snapshot.lock().unwrap().apply(event),
            // The listener is behind. Leave the snapshot as it was, so that this is sent again
            // (with a newer value) the next time around.
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Closed(_)) => return false,
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_reads_what_was_asked_for() {
        let (stats, mut rx) = SystemStats::builder()
            .interval(Duration::from_millis(20))
            .uptime()
            .memory()
            .build()
            .unwrap();

        let mut seen = Vec::new();
        while seen.len() < 2 {
            seen.push(rx.blocking_recv().unwrap());
        }

        assert!(seen.iter().all(|event| matches!(event, Event::Uptime(_) | Event::Memory(_))));

        let snapshot = stats.snapshot();
        assert!(snapshot.memory.is_some_and(|memory| memory.total > 0));
        assert_eq!(snapshot.cpu_usage, None);
        assert_eq!(snapshot.battery, None);
    }

    #[test]
    fn stops_when_dropped() {
        let (stats, mut rx) = SystemStats::builder()
            .interval(Duration::from_millis(20))
            .uptime()
            .build()
            .unwrap();

        drop(stats);

        // The channel closes once the thread has noticed, instead of staying open forever
        while rx.blocking_recv().is_some() {}
    }
}
