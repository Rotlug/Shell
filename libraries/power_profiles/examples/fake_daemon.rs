//! A stand-in for power-profiles-daemon on the *session* bus, that only offers power-saver
//! and balanced (like hardware without a performance profile). To try the crate against it:
//! `DBUS_SYSTEM_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS`.
use std::collections::HashMap;

use zbus::{fdo, interface, zvariant::OwnedValue};

struct Daemon {
    active: String,
}

#[interface(name = "net.hadess.PowerProfiles")]
impl Daemon {
    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.active.clone()
    }

    #[zbus(property)]
    fn set_active_profile(&mut self, value: String) -> zbus::Result<()> {
        match value.as_str() {
            "power-saver" | "balanced" => {
                self.active = value;
                Ok(())
            }
            _ => Err(fdo::Error::InvalidArgs(format!("Can't set {value}")).into()),
        }
    }

    #[zbus(property)]
    fn profiles(&self) -> Vec<HashMap<String, OwnedValue>> {
        ["power-saver", "balanced"]
            .into_iter()
            .map(|name| {
                HashMap::from([(
                    "Profile".to_string(),
                    OwnedValue::try_from(zbus::zvariant::Value::from(name)).unwrap(),
                )])
            })
            .collect()
    }
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    let _connection = zbus::connection::Builder::session()?
        .name("net.hadess.PowerProfiles")?
        .serve_at("/net/hadess/PowerProfiles", Daemon { active: "balanced".into() })?
        .build()
        .await?;

    std::future::pending::<()>().await;
    Ok(())
}
