use std::sync::LazyLock;
use std::time::Duration;
use dbus::blocking::stdintf::org_freedesktop_dbus::Properties as _;
use futures_signals::signal::Mutable;

const BUS_NAME: &str = "net.hadess.PowerProfiles";
const OBJECT_PATH: &str = "/net/hadess/PowerProfiles";
const INTERFACE: &str = "net.hadess.PowerProfiles";
const ACTIVE_PROFILE: &str = "ActiveProfile";

const REFRESH_INTERVAL: Duration = Duration::from_secs(3);
const CALL_TIMEOUT: Duration = Duration::from_millis(2000);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

impl PowerProfile {
    pub const ALL: [Self; 3] = [Self::PowerSaver, Self::Balanced, Self::Performance];

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "power-saver" => Some(Self::PowerSaver),
            "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    /// The next profile when cycling power-saver -> balanced -> performance -> power-saver.
    pub const fn next(self) -> Self {
        match self {
            Self::PowerSaver => Self::Balanced,
            Self::Balanced => Self::Performance,
            Self::Performance => Self::PowerSaver,
        }
    }
}

/// The active profile, or `None` if power-profiles-daemon is not running.
pub static ACTIVE_PROFILE_STATE: LazyLock<Mutable<Option<PowerProfile>>> = LazyLock::new(|| Mutable::new(None));

fn read_active_profile(connection: &dbus::blocking::Connection) -> Option<PowerProfile> {
    let proxy = connection.with_proxy(BUS_NAME, OBJECT_PATH, CALL_TIMEOUT);
    let name: String = proxy.get(INTERFACE, ACTIVE_PROFILE).ok()?;
    PowerProfile::from_name(&name)
}

fn set_active_profile(connection: &dbus::blocking::Connection, profile: PowerProfile) -> Result<(), dbus::Error> {
    let proxy = connection.with_proxy(BUS_NAME, OBJECT_PATH, CALL_TIMEOUT);
    proxy.set(INTERFACE, ACTIVE_PROFILE, profile.name().to_owned())
}

/// Switches to the profile after the current one, skipping any the hardware doesn't offer
/// (e.g. "performance" isn't available on every machine).
pub fn cycle_profile() {
    std::thread::spawn(|| {
        let Ok(connection) = dbus::blocking::Connection::new_system() else {
            return;
        };

        let Some(mut candidate) = read_active_profile(&connection) else {
            return;
        };

        for _ in 1..PowerProfile::ALL.len() {
            candidate = candidate.next();

            match set_active_profile(&connection, candidate) {
                Ok(()) => {
                    ACTIVE_PROFILE_STATE.set(Some(candidate));
                    return;
                },
                Err(err) => warn!(profile = candidate.name(), %err, "Failed to set power profile"),
            }
        }
    });
}

pub fn activate() {
    std::thread::spawn(|| {
        loop {
            let profile = dbus::blocking::Connection::new_system().ok()
                .and_then(|connection| read_active_profile(&connection));

            ACTIVE_PROFILE_STATE.set_if(profile, |old, new| old != new);
            std::thread::sleep(REFRESH_INTERVAL);
        }
    });
}
