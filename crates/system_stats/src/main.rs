use system_stats::SystemStats;

fn main() -> system_stats::Result<()> {
    let (_stats, mut rx) = SystemStats::builder().all().build()?;

    while let Some(event) = rx.blocking_recv() {
        println!("{event:?}");
    }

    Ok(())
}
