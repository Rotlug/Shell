use discord::Listener;

#[tokio::main]
async fn main() -> discord::Result<()> {
    let url = std::env::args().nth(1).unwrap_or_else(|| "ws://localhost:49152".to_string());
    let (_listener, mut rx) = Listener::new(url);

    while let Some(event) = rx.recv().await {
        println!("{event:?}");
    }

    Ok(())
}
