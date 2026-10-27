use mpris::mpris::Mpris;

#[tokio::main]
async fn main() -> mpris::Result<()> {
    let mpris = Mpris::new().await?;

    let mut rx = mpris.subscribe();

    while let Ok(upd) = rx.recv().await {
        dbg!(upd);
    }
    futures::pending!();
    Ok(())
}
