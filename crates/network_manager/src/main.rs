use network_manager::NetworkManager;

#[tokio::main]
async fn main() -> network_manager::Result<()> {
    let (_network_manager, mut rx) = NetworkManager::new().await?;

    while let Some(event) = rx.recv().await {
        println!("{event:?}");
    }

    Ok(())
}
