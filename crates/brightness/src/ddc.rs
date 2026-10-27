//! External monitors, which are adjusted over DDC/CI on their I2C bus.
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    os::fd::AsRawFd as _,
    path::PathBuf,
    sync::mpsc::{Receiver as CommandReceiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use ddc::{Ddc, Edid as _};
use ddc_i2c::{I2cDeviceDdc, I2cDeviceEnumerator};
use tokio::sync::mpsc::Sender;

use crate::{
    Monitor, MonitorId, MonitorKind,
    backlight::connector_from_drm_name,
    brightness::{Command, coalesce},
    edid::Edid,
    event::Event,
};

const DRM_PATH: &str = "/sys/class/drm";
const ID_PREFIX: &str = "ddc:";
/// Looking for monitors means talking to all of them, which is slow. Monitors don't get
/// connected that often, and this is also how often they are asked for their brightness.
const SCAN_INTERVAL: Duration = Duration::from_secs(10);
/// The VCP feature code of the brightness (luminance) of a monitor
const BRIGHTNESS: u8 = 0x10;
/// Internal panels have a backlight to adjust, that is a lot quicker and doesn't depend on the panel
const INTERNAL_CONNECTORS: [&str; 3] = ["eDP", "LVDS", "DSI"];

struct Display {
    ddc: I2cDeviceDdc,
    max: u16,
    current: u16,
}

impl Display {
    /// Reads the brightness, `None` if the monitor doesn't do DDC/CI (or isn't answering).
    fn read(ddc: &mut I2cDeviceDdc) -> Option<(u16, u16)> {
        let value = ddc.get_vcp_feature(BRIGHTNESS).ok()?;

        (value.maximum() > 0).then(|| (value.value(), value.maximum()))
    }

    fn fraction(&self) -> f64 {
        f64::from(self.current) / f64::from(self.max)
    }
}

/// The DRM connectors that have an I2C bus, by number of that bus.
struct Connector {
    name: String,
    path: PathBuf,
}

fn connectors_by_bus() -> HashMap<u32, Connector> {
    let Ok(entries) = fs::read_dir(DRM_PATH) else {
        return HashMap::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let dir_name = entry.file_name().to_string_lossy().into_owned();
            let name = connector_from_drm_name(&dir_name)?.to_string();

            // `ddc` is a link to the I2C adapter: `i2c-4`
            let adapter = fs::read_link(entry.path().join("ddc")).ok()?;
            let bus = adapter.file_name()?.to_str()?.strip_prefix("i2c-")?.parse().ok()?;

            Some((bus, Connector { name, path: entry.path() }))
        })
        .collect()
}

/// Which `/dev/i2c-N` a handle is for
fn bus_number(ddc: &I2cDeviceDdc) -> Option<u32> {
    let fd = ddc.inner_ref().inner_ref().as_raw_fd();
    let path = fs::read_link(format!("/proc/self/fd/{fd}")).ok()?;

    path.file_name()?.to_str()?.strip_prefix("i2c-")?.parse().ok()
}

fn is_internal(connector: &str) -> bool {
    INTERNAL_CONNECTORS.iter().any(|prefix| connector.starts_with(prefix))
}

fn read_edid(connector: Option<&Connector>, ddc: &mut I2cDeviceDdc) -> Option<Edid> {
    if let Some(edid) = connector
        .and_then(|connector| fs::read(connector.path.join("edid")).ok())
        .and_then(|bytes| Edid::parse(&bytes))
    {
        return Some(edid);
    }

    let mut bytes = [0u8; 128];
    ddc.read_edid(0, &mut bytes).ok()?;
    Edid::parse(&bytes)
}

pub fn spawn(rx: CommandReceiver<Command>, event_tx: Sender<Event>) -> io::Result<()> {
    thread::Builder::new()
        .name("brightness-ddc".to_string())
        .spawn(move || run(&rx, &event_tx))?;

    Ok(())
}

#[derive(Default)]
struct Scanner {
    displays: HashMap<MonitorId, Display>,
    /// Monitors that don't do DDC/CI, that aren't asked again for as long as they stay connected
    unsupported: HashSet<MonitorId>,
}

impl Scanner {
    /// Looks for monitors that appeared or disappeared, and for brightness that changed.
    /// Returns `false` once nobody is listening.
    fn scan(&mut self, event_tx: &Sender<Event>) -> bool {
        let connectors = connectors_by_bus();
        let mut seen = HashSet::new();

        // Not being able to enumerate means that there is nothing to find
        let enumerator = I2cDeviceEnumerator::new().into_iter().flatten();

        for mut ddc in enumerator {
            let Some(bus) = bus_number(&ddc) else {
                continue;
            };

            let connector = connectors.get(&bus);
            if connector.is_some_and(|connector| is_internal(&connector.name)) {
                continue;
            }

            let id = MonitorId(format!("{ID_PREFIX}i2c-{bus}"));
            seen.insert(id.clone());

            if let Some(display) = self.displays.get_mut(&id) {
                if let Some((current, _)) = Display::read(&mut display.ddc)
                    && current != display.current
                {
                    display.current = current;

                    if event_tx.blocking_send(Event::Brightness(id, display.fraction())).is_err() {
                        return false;
                    }
                }

                continue;
            }

            if self.unsupported.contains(&id) {
                continue;
            }

            let Some((current, max)) = Display::read(&mut ddc) else {
                self.unsupported.insert(id);
                continue;
            };

            let name = read_edid(connector, &mut ddc).map_or_else(
                || "External display".to_string(),
                |edid| edid.display_name(),
            );
            let display = Display { ddc, max, current };

            let monitor = Monitor {
                id: id.clone(),
                kind: MonitorKind::External,
                connector: connector.map(|connector| connector.name.clone()),
                name,
                brightness: display.fraction(),
            };

            self.displays.insert(id, display);

            if event_tx.blocking_send(Event::Added(monitor)).is_err() {
                return false;
            }
        }

        let gone: Vec<MonitorId> = self.displays.keys().filter(|id| !seen.contains(id)).cloned().collect();
        for id in gone {
            self.displays.remove(&id);

            if event_tx.blocking_send(Event::Removed(id)).is_err() {
                return false;
            }
        }

        self.unsupported.retain(|id| seen.contains(id));

        true
    }

    fn set(&mut self, command: &Command) {
        let Some(display) = self.displays.get_mut(&command.id) else {
            return;
        };

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let raw = (command.brightness * f64::from(display.max)).round() as u16;

        // Not saving the settings: that writes to the monitor's memory, which wears out, and
        // the brightness is kept anyway for as long as the monitor is on.
        if display.ddc.set_vcp_feature(BRIGHTNESS, raw).is_ok() {
            display.current = raw;
        }
    }
}

fn run(rx: &CommandReceiver<Command>, event_tx: &Sender<Event>) {
    let mut scanner = Scanner::default();
    let mut next_scan = Instant::now();

    loop {
        let now = Instant::now();

        if now >= next_scan {
            if !scanner.scan(event_tx) {
                return;
            }

            next_scan = Instant::now() + SCAN_INTERVAL;
        }

        match rx.recv_timeout(next_scan.saturating_duration_since(Instant::now())) {
            Ok(command) => {
                for command in coalesce(command, rx) {
                    scanner.set(&command);

                    // Our own change, report it right away
                    if let Some(display) = scanner.displays.get(&command.id)
                        && event_tx.blocking_send(Event::Brightness(command.id, display.fraction())).is_err()
                    {
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_connectors() {
        assert!(is_internal("eDP-1"));
        assert!(is_internal("LVDS-1"));
        assert!(!is_internal("DP-2"));
        assert!(!is_internal("HDMI-A-1"));
    }
}
