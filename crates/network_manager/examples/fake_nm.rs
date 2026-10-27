//! A stand-in for NetworkManager on the *session* bus, whose state can be set with
//! `busctl --user set-property`. To try the crate against it:
//! `DBUS_SYSTEM_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS`.
use zbus::interface;

struct Daemon {
    state: u32,
}

#[interface(name = "org.freedesktop.NetworkManager")]
impl Daemon {
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.state
    }

    #[zbus(property)]
    fn set_state(&mut self, value: u32) {
        self.state = value;
    }
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    let _connection = zbus::connection::Builder::session()?
        .name("org.freedesktop.NetworkManager")?
        .serve_at("/org/freedesktop/NetworkManager", Daemon { state: 70 })?
        .build()
        .await?;

    std::future::pending::<()>().await;
    Ok(())
}
