use std::{hash::Hash, str::FromStr, time::Duration};

use futures::StreamExt;
use tokio::{sync::broadcast, task::JoinHandle};
use zbus::{Connection, zvariant::ObjectPath};

use crate::{
    LoopStatus, Metadata, PlaybackStatus, Update, generated::player::PlayerProxy,
    player::position::{Position, Track},
};

mod position;

/// 1.0 is only a sensible max, some players allow more than this.
/// 1.5 is the true max limit for pulse/pipewire servers.
const MAX_VOLUME: f64 = 1.5;

/// What players say the track is when there is none, `SetPosition` can't be used with it.
const NO_TRACK: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekDirection {
    Forwards,
    Backwards,
}

#[derive(Debug)]
pub struct Player {
    pub name: String,
    proxy: PlayerProxy<'static>,

    tx: broadcast::Sender<Update>,

    handle: JoinHandle<()>,

    // position tracker
    position: Position,
}

/// Tells the position tracker about the track of `metadata`, and what it is going on about.
fn track_of(metadata: &Metadata) -> Track {
    Track { id: metadata.track_id.clone(), title: metadata.title.clone() }
}

impl Player {
    /// Creates a new `Player` object that starts listening to events.
    /// call `subscribe` to receive events for this player.
    ///
    /// # Errors
    /// Errors when the dbus connection fails
    ///
    /// # Panics
    /// Panics when failing to listen to the `Seeked` signal from the player.
    pub async fn new(name: String, connection: &Connection) -> crate::Result<Self> {
        let proxy = PlayerProxy::builder(connection)
            .destination(name.clone())?
            .build()
            .await?;
        let (tx, _) = broadcast::channel(16);

        let position = Position::new(proxy.clone(), tx.clone()).await?;

        /* Update task */
        let proxy_handle = proxy.clone();
        let tx_handle = tx.clone();
        let shared = position.shared();
        let handle = tokio::spawn(async move {
            let mut playback_status = proxy_handle.receive_playback_status_changed().await;
            let mut seeked = proxy_handle
                .receive_seeked()
                .await
                .expect("Failed to listen to seeked signal");
            let mut rate = proxy_handle.receive_rate_changed().await;
            let mut metadata = proxy_handle.receive_metadata_changed().await;
            let mut loop_status = proxy_handle.receive_loop_status_changed().await;
            let mut shuffle = proxy_handle.receive_shuffle_changed().await;
            let mut volume = proxy_handle.receive_volume_changed().await;

            loop {
                tokio::select! {
                    // Playback Status (Playing/Stopping/Pausing)
                    Some(change) = playback_status.next() => {
                        if let Ok(data) = change.get().await
                        && let Ok(upd) = PlaybackStatus::from_str(&data)
                        {
                            /*
                             * Some players automatically send PlaybackRate 0/1
                             * On start/stop, however, Some dont!
                             */
                            let playing = upd == PlaybackStatus::Playing;
                            let player_rate = if playing { proxy_handle.rate().await.unwrap_or(1.0) } else { 0.0 };

                            shared.set_pace(Some(playing), playing.then_some(player_rate));
                            let _ = tx_handle.send(Update::PlaybackRate(player_rate));
                            let _ = tx_handle.send(Update::PlaybackStatus(upd));

                            // Where it is is exactly known when it starts or stops
                            let shared = shared.clone();
                            tokio::spawn(async move { shared.read().await });
                        }
                    },
                    // Position (Seeked)
                    Some(change) = seeked.next() => {
                        let Ok(args) = change.args() else {
                            continue;
                        };

                        shared.set(Duration::from_micros(u64::try_from(args.value).unwrap_or(0)));
                    }
                    // Playback Rate
                    Some(change) = rate.next() => {
                        if let Ok(data) = change.get().await {
                            shared.set_pace(None, Some(data));
                            let _ = tx_handle.send(Update::PlaybackRate(data));
                        }
                    }
                    // Metadata
                    Some(change) = metadata.next() => {
                        if let Ok(data) = change.get().await {
                            let data = Metadata::from(data);

                            // Another track starts from the start, which the player doesn't say
                            shared.metadata_changed(track_of(&data), data.length);
                            let _ = tx_handle.send(Update::Metadata(data));
                        }
                    }
                    // Loop Status (None/Track/Playlist)
                    Some(change) = loop_status.next() => {
                        if let Ok(data) = change.get().await
                        && let Ok(upd) = LoopStatus::from_str(&data)
                        {
                            let _ = tx_handle.send(Update::LoopStatus(upd));
                        }
                    }
                    // Shuffle
                    Some(change) = shuffle.next() => {
                        if let Ok(data) = change.get().await {
                            let _ = tx_handle.send(Update::Shuffle(data));
                        }
                    }
                    // Volume
                    Some(change) = volume.next() => {
                        if let Ok(data) = change.get().await {
                            let _ = tx_handle.send(Update::Volume(data));
                        }
                    }
                    else => break,
                }
            }
        });

        Ok(Self {
            name,
            proxy,
            tx,
            handle,
            position,
        })
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Update> {
        self.tx.subscribe()
    }

    /// Get the current playback status (Playing, paused, stopped).
    ///
    /// # Errors
    /// Errors when a client returns a string that fails to get parsed by
    /// `PlaybackStatus`
    pub async fn get_playback_status(&self) -> crate::Result<PlaybackStatus> {
        PlaybackStatus::from_str(&self.proxy.playback_status().await?)
    }

    /// The (estimated) current position of the player.
    pub async fn get_position(&self) -> Duration {
        self.position.get_position()
    }

    /// This method is called from `Mpris` to request updated information when a player
    /// becomes the active one. This can also be used to sync a ui with the current state
    /// of the player.
    ///
    /// Properties that the player doesn't implement (most of them are optional)
    /// are skipped.
    ///
    /// # Errors
    /// Currently never errors, the result is kept so that failing to sync can be
    /// reported in the future without breaking callers.
    pub async fn sync(&self) -> crate::Result<()> {
        if let Ok(status) = self.get_playback_status().await {
            let _ = self.tx.send(Update::PlaybackStatus(status));
        }

        if let Ok(loop_status) = self.proxy.loop_status().await
            && let Ok(loop_status) = LoopStatus::from_str(&loop_status)
        {
            let _ = self.tx.send(Update::LoopStatus(loop_status));
        }

        if let Ok(shuffle) = self.proxy.shuffle().await {
            let _ = self.tx.send(Update::Shuffle(shuffle));
        }

        if let Ok(volume) = self.proxy.volume().await {
            let _ = self.tx.send(Update::Volume(volume));
        }

        let shared = self.position.shared();

        if let Ok(metadata) = self.proxy.metadata().await {
            let metadata = Metadata::from(metadata);

            shared.metadata_changed(track_of(&metadata), metadata.length);
            let _ = self.tx.send(Update::Metadata(metadata));
        }

        // What the player says, or what is believed if it doesn't say
        shared.read().await;
        shared.announce();
        Ok(())
    }

    /* Control Methods */

    /// Toggles between playing and paused.
    ///
    /// # Errors
    /// Errors when the player can't pause, or the dbus call fails.
    pub async fn play_pause(&self) -> crate::Result<()> {
        if self.proxy.can_pause().await? {
            Ok(self.proxy.play_pause().await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// # Errors
    /// Errors when the player can't be controlled, or the dbus call fails.
    pub async fn stop(&self) -> crate::Result<()> {
        if self.proxy.can_control().await? {
            Ok(self.proxy.stop().await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// # Errors
    /// Errors when the player can't play, or the dbus call fails.
    pub async fn play(&self) -> crate::Result<()> {
        if self.proxy.can_play().await? {
            Ok(self.proxy.play().await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// # Errors
    /// Errors when the player can't pause, or the dbus call fails.
    pub async fn pause(&self) -> crate::Result<()> {
        if self.proxy.can_pause().await? {
            Ok(self.proxy.pause().await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// Skips to the next track.
    ///
    /// # Errors
    /// Errors when the player can't go to the next track, or the dbus call fails.
    pub async fn next(&self) -> crate::Result<()> {
        if self.proxy.can_go_next().await? {
            Ok(self.proxy.next().await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// Goes back to the previous track.
    ///
    /// # Errors
    /// Errors when the player can't go to the previous track, or the dbus call fails.
    pub async fn previous(&self) -> crate::Result<()> {
        if self.proxy.can_go_previous().await? {
            Ok(self.proxy.previous().await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// Seeks `duration` forwards or backwards, relative to the current position.
    ///
    /// # Errors
    /// Errors when the player can't seek, the duration is too long to be
    /// sent over dbus, or the dbus call fails.
    pub async fn seek(&self, duration: Duration, direction: SeekDirection) -> crate::Result<()> {
        let micros = i64::try_from(duration.as_micros())
            .map_err(|_| crate::Error::InvalidDuration(duration))?;

        let micros = match direction {
            SeekDirection::Forwards => micros,
            SeekDirection::Backwards => -micros,
        };

        if !self.proxy.can_seek().await? {
            return Err(crate::Error::MissingCapability);
        }

        self.proxy.seek(micros).await?;

        // Not every player says that it was seeked
        let shared = self.position.shared();
        let current = shared.current();
        shared.set(match direction {
            SeekDirection::Forwards => current.saturating_add(duration),
            SeekDirection::Backwards => current.saturating_sub(duration),
        });

        Ok(())
    }

    /// Seeks to an absolute position.
    ///
    /// This is a position the player is told, not one worked out from where it is believed to be
    /// (which is a little behind by the time the player hears it). Players that can't be told
    /// are told how far to go from where they are believed to be instead.
    ///
    /// # Errors
    /// Errors when the player can't seek, the duration is too long to be
    /// sent over dbus, or the dbus call fails.
    pub async fn set_position(&self, duration: Duration) -> crate::Result<()> {
        let micros = i64::try_from(duration.as_micros())
            .map_err(|_| crate::Error::InvalidDuration(duration))?;

        if !self.proxy.can_seek().await? {
            return Err(crate::Error::MissingCapability);
        }

        let shared = self.position.shared();
        let track = self.proxy.metadata().await.ok().map(Metadata::from).and_then(|metadata| metadata.track_id);

        let told = match track.as_deref().filter(|track| *track != NO_TRACK).map(ObjectPath::try_from) {
            Some(Ok(track)) => self.proxy.set_position(&track, micros).await.is_ok(),
            _ => false,
        };

        if !told {
            let current = shared.current();

            if duration >= current {
                self.seek(duration - current, SeekDirection::Forwards).await?;
            } else {
                self.seek(current - duration, SeekDirection::Backwards).await?;
            }
        }

        shared.set(duration);
        Ok(())
    }

    /// # Errors
    /// Errors when the player can't be controlled, or the dbus call fails.
    pub async fn set_loop_status(&self, loop_status: LoopStatus) -> crate::Result<()> {
        if self.proxy.can_control().await? {
            Ok(self.proxy.set_loop_status(loop_status.as_str()).await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// # Errors
    /// Errors when the player can't be controlled, or the dbus call fails.
    pub async fn set_shuffle(&self, shuffle: bool) -> crate::Result<()> {
        if self.proxy.can_control().await? {
            Ok(self.proxy.set_shuffle(shuffle).await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// Sets the player's volume, clamped to `0.0..=1.5`.
    ///
    /// # Errors
    /// Errors when the player can't be controlled, or the dbus call fails.
    pub async fn set_volume(&self, volume: f64) -> crate::Result<()> {
        if self.proxy.can_control().await? {
            Ok(self.proxy.set_volume(volume.clamp(0.0, MAX_VOLUME)).await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// Changes the player's volume by `delta`.
    ///
    /// # Errors
    /// Errors when the player can't be controlled, or the dbus call fails.
    pub async fn adjust_volume(&self, delta: f64) -> crate::Result<()> {
        let volume = self.proxy.volume().await?;
        self.set_volume(volume + delta).await
    }
}

/* Needed for storing in hashmap */
impl Hash for Player {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq for Player {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Player {}

/* Cleanup */
impl Drop for Player {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
