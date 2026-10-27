//! Internal panels, which the kernel exposes in `/sys/class/backlight`.
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{Receiver as CommandReceiver, RecvTimeoutError},
    thread,
    time::Duration,
};

use tokio::sync::mpsc::Sender;

use crate::{
    Monitor, MonitorId, MonitorKind,
    brightness::{Command, coalesce},
    edid::Edid,
    event::Event,
};

const BACKLIGHT_PATH: &str = "/sys/class/backlight";
const ID_PREFIX: &str = "backlight:";
/// The brightness keys of the machine can change the brightness too, so keep an eye on it
const POLL_INTERVAL: Duration = Duration::from_millis(250);

const LOGIND_BUS: &str = "org.freedesktop.login1";
const LOGIND_SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";
const LOGIND_SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";

pub fn owns(id: &MonitorId) -> bool {
    id.as_str().starts_with(ID_PREFIX)
}

/// How the kernel says a backlight is controlled, from best to worst. When a panel has more
/// than one, they all end up changing the same thing, and the first is the one to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Interface {
    Raw,
    Platform,
    Firmware,
    Unknown,
}

impl Interface {
    fn parse(kind: &str) -> Self {
        match kind.trim() {
            "raw" => Self::Raw,
            "platform" => Self::Platform,
            "firmware" => Self::Firmware,
            _ => Self::Unknown,
        }
    }
}

/// A backlight as found in sysfs, before anything is known about its panel.
#[derive(Clone, Debug)]
struct Found {
    name: String,
    path: PathBuf,
    interface: Interface,
    connector: Option<String>,
}

/// The connector a backlight belongs to, which is the directory it is in (`card1-eDP-1`) when it
/// belongs to one: `eDP-1`.
fn connector_of(path: &Path) -> Option<String> {
    let parent = path.canonicalize().ok()?.parent()?.file_name()?.to_str()?.to_string();

    connector_from_drm_name(&parent).map(str::to_string)
}

/// `card1-eDP-1` -> `eDP-1`
pub fn connector_from_drm_name(name: &str) -> Option<&str> {
    let (card, connector) = name.split_once('-')?;
    let index = card.strip_prefix("card")?;

    (!index.is_empty() && index.bytes().all(|b| b.is_ascii_digit())).then_some(connector)
}

/// Keeps one backlight per panel, the best one. Backlights without a known panel are all taken
/// to be the same panel.
fn pick_best(found: Vec<Found>) -> Vec<Found> {
    let mut best: HashMap<Option<String>, Found> = HashMap::new();

    for candidate in found {
        match best.get(&candidate.connector) {
            Some(current) if (current.interface, &current.name) <= (candidate.interface, &candidate.name) => {}
            _ => {
                best.insert(candidate.connector.clone(), candidate);
            }
        }
    }

    let mut picked: Vec<Found> = best.into_values().collect();
    picked.sort_by(|a, b| a.name.cmp(&b.name));
    picked
}

fn scan() -> Vec<Found> {
    let Ok(entries) = fs::read_dir(BACKLIGHT_PATH) else {
        return Vec::new();
    };

    let found = entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            let interface = fs::read_to_string(path.join("type"))
                .map_or(Interface::Unknown, |kind| Interface::parse(&kind));

            Found {
                name: entry.file_name().to_string_lossy().into_owned(),
                connector: connector_of(&path),
                path,
                interface,
            }
        })
        .collect();

    pick_best(found)
}

fn read_number(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

struct Device {
    found: Found,
    id: MonitorId,
    max: u32,
    current: u32,
}

impl Device {
    /// `None` for a backlight that can't be read or has nothing to adjust.
    fn open(found: Found) -> Option<Self> {
        let max = read_number(&found.path.join("max_brightness")).filter(|&max| max > 0)?;

        let mut device = Self {
            id: MonitorId(format!("{ID_PREFIX}{}", found.name)),
            found,
            max,
            current: 0,
        };
        device.current = device.read()?;

        Some(device)
    }

    /// The level the panel was set to: the same scale as what `write` takes.
    ///
    /// Not `actual_brightness`, which is what the hardware ends up at after its own curve is
    /// applied (amdgpu has a non-linear one). It differs from what was set, so a slider that
    /// is fed with it would never agree with the value it just sent.
    fn read(&self) -> Option<u32> {
        read_number(&self.found.path.join("brightness"))
    }

    fn fraction(&self) -> f64 {
        f64::from(self.current) / f64::from(self.max)
    }

    fn monitor(&self) -> Monitor {
        let name = self
            .found
            .connector
            .as_ref()
            .and_then(|_| {
                let connector_dir = self.found.path.canonicalize().ok()?.parent()?.to_path_buf();
                Edid::parse(&fs::read(connector_dir.join("edid")).ok()?)
            })
            .map_or_else(|| "Internal display".to_string(), |edid| edid.display_name());

        Monitor {
            id: self.id.clone(),
            kind: MonitorKind::Internal,
            connector: self.found.connector.clone(),
            name,
            brightness: self.fraction(),
        }
    }

    /// The darkest the panel is allowed to get, a panel that is off can't be turned back on.
    fn floor(&self) -> u32 {
        (self.max / 100).max(1)
    }

    fn raw_from_fraction(&self, brightness: f64) -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let raw = (brightness * f64::from(self.max)).round() as u32;

        raw.clamp(self.floor(), self.max)
    }

    /// Setting the brightness takes root, unless logind does it for us: it lets the user of
    /// the active session do it.
    fn write(&self, logind: Option<&zbus::blocking::Connection>, raw: u32) {
        if let Some(logind) = logind
            && logind
                .call_method(
                    Some(LOGIND_BUS),
                    LOGIND_SESSION_PATH,
                    Some(LOGIND_SESSION_INTERFACE),
                    "SetBrightness",
                    &("backlight", self.found.name.as_str(), raw),
                )
                .is_ok()
        {
            return;
        }

        // Works when running as root, or with a udev rule that hands out the permission
        let _ = fs::write(self.found.path.join("brightness"), raw.to_string());
    }
}

pub fn spawn(rx: CommandReceiver<Command>, event_tx: Sender<Event>) -> io::Result<()> {
    thread::Builder::new()
        .name("brightness-backlight".to_string())
        .spawn(move || run(&rx, &event_tx))?;

    Ok(())
}

fn run(rx: &CommandReceiver<Command>, event_tx: &Sender<Event>) {
    let logind = zbus::blocking::Connection::system().ok();
    let mut devices: HashMap<MonitorId, Device> = HashMap::new();

    loop {
        // Panels come and go too (the lid of some laptops, docks), and change brightness
        let found = scan();

        let gone: Vec<MonitorId> = devices
            .keys()
            .filter(|id| !found.iter().any(|found| found.name == id.as_str()[ID_PREFIX.len()..]))
            .cloned()
            .collect();

        for id in gone {
            devices.remove(&id);
            if event_tx.blocking_send(Event::Removed(id)).is_err() {
                return;
            }
        }

        for found in found {
            let id = MonitorId(format!("{ID_PREFIX}{}", found.name));

            if let Some(device) = devices.get_mut(&id) {
                if let Some(current) = device.read()
                    && current != device.current
                {
                    device.current = current;

                    if event_tx.blocking_send(Event::Brightness(id, device.fraction())).is_err() {
                        return;
                    }
                }
            } else if let Some(device) = Device::open(found) {
                if event_tx.blocking_send(Event::Added(device.monitor())).is_err() {
                    return;
                }

                devices.insert(id, device);
            }
        }

        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(command) => {
                for command in coalesce(command, rx) {
                    if let Some(device) = devices.get(&command.id) {
                        device.write(logind.as_ref(), device.raw_from_fraction(command.brightness));
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

    fn found(name: &str, interface: Interface, connector: Option<&str>) -> Found {
        Found {
            name: name.to_string(),
            path: PathBuf::from(name),
            interface,
            connector: connector.map(str::to_string),
        }
    }

    fn names(found: &[Found]) -> Vec<&str> {
        found.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn connector_names() {
        assert_eq!(connector_from_drm_name("card1-eDP-1"), Some("eDP-1"));
        assert_eq!(connector_from_drm_name("card0-HDMI-A-2"), Some("HDMI-A-2"));
        assert_eq!(connector_from_drm_name("card1"), None);
        assert_eq!(connector_from_drm_name("renderD128"), None);
        assert_eq!(connector_from_drm_name("cardx-eDP-1"), None);
    }

    #[test]
    fn one_backlight_per_panel() {
        let picked = pick_best(vec![
            found("acpi_video0", Interface::Firmware, Some("eDP-1")),
            found("intel_backlight", Interface::Raw, Some("eDP-1")),
            found("nvidia_0", Interface::Raw, Some("eDP-2")),
        ]);

        assert_eq!(names(&picked), ["intel_backlight", "nvidia_0"]);
    }

    #[test]
    fn backlights_without_a_panel_are_one_panel() {
        let picked = pick_best(vec![
            found("acpi_video0", Interface::Firmware, None),
            found("thinkpad_screen", Interface::Platform, None),
        ]);

        assert_eq!(names(&picked), ["thinkpad_screen"]);
    }

    #[test]
    fn never_fully_dark() {
        let device = Device {
            found: found("x", Interface::Raw, None),
            id: MonitorId("backlight:x".into()),
            max: 65535,
            current: 100,
        };

        assert_eq!(device.raw_from_fraction(0.0), 655);
        assert_eq!(device.raw_from_fraction(1.0), 65535);
        assert_eq!(device.raw_from_fraction(0.5), 32768);
    }
}
