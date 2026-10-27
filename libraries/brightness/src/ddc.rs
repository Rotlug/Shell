//! External monitors, which are adjusted over DDC/CI on their I2C bus.
use std::{
    collections::HashMap,
    fs, io,
    path::PathBuf,
    sync::mpsc::{Receiver as CommandReceiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use ddc::{Ddc, Edid as _};
use ddc_i2c::{I2cDeviceDdc, from_i2c_device};
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
/// How often it is checked whether monitors were connected or disconnected. That is only
/// looking at what the kernel says about the connectors, talking to monitors is slow.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// How often a monitor that is not answering is asked for its brightness, before concluding
/// that it does not do DDC/CI
const MAX_ATTEMPTS: u32 = 5;
/// The VCP feature code of the brightness (luminance) of a monitor
const BRIGHTNESS: u8 = 0x10;
/// Internal panels have a backlight to adjust, that is a lot quicker and doesn't depend on the panel
const INTERNAL_CONNECTORS: [&str; 3] = ["eDP", "LVDS", "DSI"];

struct Display {
    ddc: I2cDeviceDdc,
    /// The number of the I2C bus the monitor is on
    bus: u32,
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

fn is_internal(connector: &str) -> bool {
    INTERNAL_CONNECTORS.iter().any(|prefix| connector.starts_with(prefix))
}

/// The external monitors that are plugged in, by the number of the I2C bus they are on. This
/// is all there is to look at: the other buses are not for monitors, and talking to them is
/// pointless at best.
fn connected_externals() -> HashMap<u32, Connector> {
    connectors_by_bus()
        .into_iter()
        .filter(|(_, connector)| {
            !is_internal(&connector.name)
                && fs::read_to_string(connector.path.join("status")).is_ok_and(|status| status.trim() == "connected")
        })
        .collect()
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
    /// How often reading the brightness of a monitor failed, by the bus it is on. It may just
    /// not be ready yet after being connected, but not answering forever means it does not do
    /// DDC/CI. Forgotten once the monitor is disconnected.
    failures: HashMap<u32, u32>,
}

impl Scanner {
    /// Looks for monitors that were connected or disconnected. Only the monitors that are
    /// plugged in are talked to, and only once: their brightness is not polled, it is read when
    /// they appear and kept up to date by the changes made here.
    /// Returns `false` once nobody is listening.
    fn scan(&mut self, event_tx: &Sender<Event>) -> bool {
        let connected = connected_externals();

        let gone: Vec<MonitorId> = self
            .displays
            .iter()
            .filter(|(_, display)| !connected.contains_key(&display.bus))
            .map(|(id, _)| id.clone())
            .collect();
        for id in gone {
            self.displays.remove(&id);

            if event_tx.blocking_send(Event::Removed(id)).is_err() {
                return false;
            }
        }

        self.failures.retain(|bus, _| connected.contains_key(bus));

        for (bus, connector) in &connected {
            let id = MonitorId(format!("{ID_PREFIX}i2c-{bus}"));

            if self.displays.contains_key(&id) || self.failures.get(bus).is_some_and(|&n| n >= MAX_ATTEMPTS) {
                continue;
            }

            let opened = from_i2c_device(format!("/dev/i2c-{bus}")).ok();
            let Some((mut ddc, (current, max))) = opened.and_then(|mut ddc| {
                let brightness = Display::read(&mut ddc)?;
                Some((ddc, brightness))
            }) else {
                *self.failures.entry(*bus).or_default() += 1;
                continue;
            };

            let name = read_edid(Some(connector), &mut ddc).map_or_else(
                || "External display".to_string(),
                |edid| edid.display_name(),
            );
            let display = Display { ddc, bus: *bus, max, current };

            let monitor = Monitor {
                id: id.clone(),
                kind: MonitorKind::External,
                connector: Some(connector.name.clone()),
                name,
                brightness: display.fraction(),
            };

            self.displays.insert(id, display);

            if event_tx.blocking_send(Event::Added(monitor)).is_err() {
                return false;
            }
        }

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

            next_scan = Instant::now() + POLL_INTERVAL;
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
