use std::collections::HashMap;

use systray::{event::Event, tray::Tray};

#[tokio::main]
async fn main() -> systray::Result<()> {
    let (tray, mut rx) = Tray::new().await?;

    println!("Serving the watcher ourselves: {}", tray.owns_watcher());

    let mut items = HashMap::new();

    while let Some(event) = rx.recv().await {
        match event {
            Event::ItemAdded(addr, item) => {
                println!("Welcome {addr}: {:?}", item.title().await);
                items.insert(addr, item);
            }
            Event::ItemRemoved(addr) => {
                println!("{addr} has been removed");
                items.remove(&addr);
            }
            Event::MenuChanged(addr, items) => {
                println!("New items for {addr}:");
                for item in items {
                    println!("{:?}", item.contents);
                }
            }
            Event::IconChanged(addr) => {
                println!("New icon for {addr}");
            }
            Event::TitleChanged(addr) => {
                println!("New title for {addr}");
            }
            Event::StatusChanged(addr, status) => {
                println!("{addr} is now {status}");
            }
            Event::ToolTipChanged(addr) => {
                println!("New tooltip for {addr}");
            }
        }
    }

    Ok(())
}
