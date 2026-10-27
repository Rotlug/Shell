use std::{hash::Hash, str::FromStr, time::Duration};

use futures::StreamExt;
use tokio::{sync::broadcast, task::JoinHandle};
use zbus::Connection;

use crate::{
    LoopStatus, Metadata, PlaybackStatus, Update, generated::player::PlayerProxy,
    player::position::Position,
};

mod position;

/// 1.0 is only a sensible max, some players allow more than this.
/// 1.5 is the true max limit for pulse/pipewire servers.
const MAX_VOLUME: f64 = 1.5;

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

        /* Update task */
        let proxy_handle = proxy.clone();
        let tx_handle = tx.clone();
        let handle = tokio::spawn(async move {
            let mut playback_status = proxy_handle.receive_playback_status_changed().await;
            let mut position = proxy_handle.receive_position_changed().await;
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
                            match upd {
                                PlaybackStatus::Playing => {
                                    let _ = tx_handle.send(Update::PlaybackRate(1.0));
                                }
                                PlaybackStatus::Paused | PlaybackStatus::Stopped => {
                                    let _ = tx_handle.send(Update::PlaybackRate(0.0));
                                }
                            }
                            let _ = tx_handle.send(Update::PlaybackStatus(upd));
                        }
                    },
                    // Position
                    Some(change) = position.next() => {
                        if let Ok(data) = change.get().await {
                            #[allow(clippy::cast_sign_loss)]
                            let dur = Duration::from_micros(data as u64);
                            let _ = tx_handle.send(Update::Position(dur));
                        }
                    }
                    // Position (Seeked)
                    Some(change) = seeked.next() => {
                        let Ok(args) = change.args() else {
                            continue;
                        };

                        #[allow(clippy::cast_sign_loss)]
                        let dur = Duration::from_micros(args.value as u64);
                        let _ = tx_handle.send(Update::Position(dur));
                    }
                    // Playback Rate
                    Some(change) = rate.next() => {
                        if let Ok(data) = change.get().await {
                            let _ = tx_handle.send(Update::PlaybackRate(data));
                        }
                    }
                    // Metadata
                    Some(change) = metadata.next() => {
                        if let Ok(data) = change.get().await {
                            let _ = tx_handle.send(Update::Metadata(Metadata::from(data)));
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

        let position = Position::new(proxy.clone(), tx.subscribe(), tx.clone()).await?;

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
        self.position.get_position().await
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

        if let Ok(metadata) = self.proxy.metadata().await {
            let _ = self.tx.send(Update::Metadata(Metadata::from(metadata)));
        }

        let position = self.position.get_position().await;
        let _ = self.tx.send(Update::Position(position));
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

        if self.proxy.can_seek().await? {
            Ok(self.proxy.seek(micros).await?)
        } else {
            Err(crate::Error::MissingCapability)
        }
    }

    /// Seeks to an absolute position.
    ///
    /// # Errors
    /// Errors when the player can't seek, the duration is too long to be
    /// sent over dbus, or the dbus call fails.
    pub async fn set_position(&self, duration: Duration) -> crate::Result<()> {
        let current = self.position.get_position().await;

        if duration >= current {
            self.seek(duration - current, SeekDirection::Forwards).await
        } else {
            self.seek(current - duration, SeekDirection::Backwards).await
        }
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
