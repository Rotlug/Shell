use std::{sync::Arc, time::Duration};

use notif_daemon::{event::Event, manager::Manager, notification::ClosingReason};

#[tokio::main]
async fn main() -> notif_daemon::Result<()> {
    let (mgr, mut rx) = Manager::new().await?;
    let mgr = Arc::new(mgr);

    while let Some(event) = rx.recv().await {
        dbg!(&event);

        if let Event::Incoming(notif) = event {
            let mgr = mgr.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let _ = mgr.close_notification(notif.id, ClosingReason::Expired).await;
            });
        }
    }

    Ok(())
}
