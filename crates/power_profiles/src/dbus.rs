use std::collections::HashMap;

use zbus::{proxy, zvariant::OwnedValue};

#[proxy(
    interface = "net.hadess.PowerProfiles",
    default_service = "net.hadess.PowerProfiles",
    default_path = "/net/hadess/PowerProfiles"
)]
pub trait PowerProfilesDaemon {
    /// ActiveProfile property
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, value: &str) -> zbus::Result<()>;

    /// Profiles property: one dictionary per profile that can be set, with its name under
    /// `Profile`
    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;
}

pub const BUS_NAME: &str = "net.hadess.PowerProfiles";
pub const OBJECT_PATH: &str = "/net/hadess/PowerProfiles";
