//! A minimal `StatusNotifierItem`, to try the tray with.
//!
//! `fake_item bus-name` registers using its bus name, `fake_item path` registers using just
//! its object path (like some older toolkits do). It changes its title and icon after a while.
use std::time::Duration;

use zbus::{interface, object_server::SignalEmitter};

struct Item {
    title: String,
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn id(&self) -> String {
        "fake".into()
    }

    #[zbus(property)]
    fn title(&self) -> String {
        self.title.clone()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "Active".into()
    }

    #[zbus(property)]
    fn icon_name(&self) -> String {
        "folder".into()
    }

    #[zbus(property)]
    fn menu(&self) -> zbus::zvariant::OwnedObjectPath {
        zbus::zvariant::OwnedObjectPath::try_from("/NO_DBUSMENU").unwrap()
    }

    fn activate(&self, _x: i32, _y: i32) {
        println!("activated");
    }

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[tokio::main]
async fn main() -> zbus::Result<()> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "bus-name".into());

    let connection = zbus::Connection::session().await?;
    connection
        .object_server()
        .at("/StatusNotifierItem", Item { title: "Fake".into() })
        .await?;

    let service = if mode == "path" {
        "/StatusNotifierItem".to_string()
    } else {
        let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        connection.request_name(name.as_str()).await?;
        name
    };

    connection
        .call_method(
            Some("org.kde.StatusNotifierWatcher"),
            "/StatusNotifierWatcher",
            Some("org.kde.StatusNotifierWatcher"),
            "RegisterStatusNotifierItem",
            &(service.as_str(),),
        )
        .await?;
    println!("registered as {service}");

    tokio::time::sleep(Duration::from_secs(2)).await;

    let iface = connection
        .object_server()
        .interface::<_, Item>("/StatusNotifierItem")
        .await?;
    iface.get_mut().await.title = "Renamed".into();
    Item::new_title(iface.signal_emitter()).await?;
    Item::new_icon(iface.signal_emitter()).await?;

    tokio::time::sleep(Duration::from_secs(60)).await;
    Ok(())
}
