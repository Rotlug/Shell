//! The power profile, backed by the `power_profiles` crate.
use std::sync::{Arc, LazyLock, OnceLock};
use futures_signals::signal::Mutable;
use power_profiles::PowerProfiles;
use power_profiles::event::Event;

pub use power_profiles::Profile;

/// The active profile, or `None` if power-profiles-daemon isn't running.
pub static ACTIVE_PROFILE_STATE: LazyLock<Mutable<Option<Profile>>> = LazyLock::new(|| Mutable::new(None));

static POWER_PROFILES: OnceLock<Arc<PowerProfiles>> = OnceLock::new();

pub async fn activate() {
    let (power_profiles, mut events) = match PowerProfiles::new().await {
        Ok(power_profiles) => power_profiles,
        Err(e) => return error!(%e, "Failed to connect to the system bus, power profiles will not be available"),
    };

    if POWER_PROFILES.set(Arc::new(power_profiles)).is_err() {
        return;
    }

    tokio::spawn(async move {
        while let Some(Event::ActiveProfile(profile)) = events.recv().await {
            ACTIVE_PROFILE_STATE.set(profile);
        }
    });
}

/// Switches to the next profile that the hardware offers (e.g. "performance" isn't available
/// on every machine).
pub fn cycle_profile() {
    let Some(power_profiles) = POWER_PROFILES.get().cloned() else {
        return;
    };

    tokio::spawn(async move {
        if let Err(e) = power_profiles.cycle_profile().await {
            warn!(%e, "Failed to switch power profile");
        }
    });
}
