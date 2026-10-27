use std::str::FromStr;

use futures::StreamExt;
use tokio::{
    sync::mpsc::{Receiver, channel},
    task::JoinHandle,
};
use zbus::{Connection, fdo, proxy::CacheProperties};

use crate::{
    Profile, event::Event,
    dbus::{BUS_NAME, OBJECT_PATH, PowerProfilesDaemonProxy},
};

/// A connection to power-profiles-daemon, used for changing the power profile and to learn
/// about it changing.
pub struct PowerProfiles {
    proxy: PowerProfilesDaemonProxy<'static>,
    handle: JoinHandle<()>,
}

impl PowerProfiles {
    /// Connects to power-profiles-daemon on the system bus. Returns a `Receiver` which can be
    /// used to listen to the active profile.
    ///
    /// The daemon doesn't have to be running (yet), `Event::ActiveProfile(None)` is sent
    /// instead until it is.
    ///
    /// # Errors
    /// Errors when the dbus connection fails
    pub async fn new() -> crate::Result<(Self, Receiver<Event>)> {
        Self::with_connection(Connection::system().await?).await
    }

    /// Same as `new`, on a connection of your own.
    ///
    /// # Errors
    /// Errors when the dbus connection fails
    pub async fn with_connection(connection: Connection) -> crate::Result<(Self, Receiver<Event>)> {
        let (event_tx, event_rx) = channel(16);

        // The daemon may come and go, so don't cache what it says
        let proxy = PowerProfilesDaemonProxy::builder(&connection)
            .cache_properties(CacheProperties::No)
            .build()
            .await?;

        // The typed property streams rely on the cache, which would go stale if the daemon restarts
        let mut properties_changed = fdo::PropertiesProxy::builder(&connection)
            .destination(BUS_NAME)?
            .path(OBJECT_PATH)?
            .build()
            .await?
            .receive_properties_changed()
            .await?;
        let mut name_owner_changed = fdo::DBusProxy::new(&connection)
            .await?
            .receive_name_owner_changed_with_args(&[(0, BUS_NAME)])
            .await?;

        let proxy_task = proxy.clone();
        let handle = tokio::spawn(async move {
            let mut last = None;
            let mut first = true;

            loop {
                // The daemon showing up or disappearing is a change as well
                let current = Self::read_active_profile(&proxy_task).await.ok();

                if first || current != last {
                    first = false;
                    last = current;

                    if event_tx.send(Event::ActiveProfile(current)).await.is_err() {
                        break;
                    }
                }

                tokio::select! {
                    Some(_) = properties_changed.next() => {},
                    Some(_) = name_owner_changed.next() => {},
                    else => break,
                }
            }
        });

        Ok((Self { proxy, handle }, event_rx))
    }

    async fn read_active_profile(proxy: &PowerProfilesDaemonProxy<'_>) -> crate::Result<Profile> {
        Profile::from_str(&proxy.active_profile().await?)
    }

    /// The profile that is active right now.
    ///
    /// # Errors
    /// Errors when the daemon isn't running, or reports a profile that isn't known.
    pub async fn active_profile(&self) -> crate::Result<Profile> {
        Self::read_active_profile(&self.proxy).await
    }

    /// The profiles that can be set on this machine. Not all hardware offers all of them.
    ///
    /// # Errors
    /// Errors when the daemon isn't running.
    pub async fn available_profiles(&self) -> crate::Result<Vec<Profile>> {
        let profiles = self.proxy.profiles().await?;

        Ok(profiles
            .iter()
            .filter_map(|profile| {
                profile
                    .get("Profile")
                    .and_then(|name| name.downcast_ref::<&str>().ok())
                    .and_then(|name| Profile::from_str(name).ok())
            })
            .collect())
    }

    /// Changes the active profile.
    ///
    /// # Errors
    /// Errors when the daemon isn't running, or when the profile isn't available.
    pub async fn set_active_profile(&self, profile: Profile) -> crate::Result<()> {
        Ok(self.proxy.set_active_profile(profile.as_str()).await?)
    }

    /// Switches to the next profile (see `Profile::next`) that is available, and returns it.
    ///
    /// # Errors
    /// Errors when the daemon isn't running, or when no other profile is available.
    pub async fn cycle_profile(&self) -> crate::Result<Profile> {
        let available = self.available_profiles().await?;
        let mut candidate = self.active_profile().await?;

        for _ in 1..Profile::ALL.len() {
            candidate = candidate.next();

            if available.contains(&candidate) {
                self.set_active_profile(candidate).await?;
                return Ok(candidate);
            }
        }

        Err(crate::Error::NoProfileAvailable)
    }
}

impl Drop for PowerProfiles {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
