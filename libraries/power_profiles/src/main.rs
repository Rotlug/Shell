use power_profiles::PowerProfiles;

#[tokio::main]
async fn main() -> power_profiles::Result<()> {
    let (power_profiles, mut rx) = PowerProfiles::new().await?;

    // `power_profiles cycle` switches to the next profile and exits
    if std::env::args().nth(1).as_deref() == Some("cycle") {
        println!("{:?}", power_profiles.cycle_profile().await?);
        return Ok(());
    }

    println!("available: {:?}", power_profiles.available_profiles().await);

    while let Some(event) = rx.recv().await {
        println!("{event:?}");
    }

    Ok(())
}
