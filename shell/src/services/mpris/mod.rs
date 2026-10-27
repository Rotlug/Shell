//! MPRIS media player state, backed by the `mpris` crate.
//!
//! The crate reports updates for the active player over a channel; this keeps
//! the latest state in signals the UI can subscribe to, and turns the
//! player controls into fire-and-forget functions.
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;
use futures_signals::signal::{Mutable, SignalExt as _};
use mpris::player::{Player, SeekDirection};
use mpris::{Mpris, Update};

pub use mpris::{LoopStatus, PlaybackStatus};

/// Metadata of the current track. Lengths are in microseconds.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub art_url: Option<String>,
    pub album: Option<String>,
    pub artist: Option<Vec<String>>,
    pub title: Option<String>,
    pub length: Option<i64>,
}

/// A snapshot of the active player.
#[derive(Debug, Clone, Default)]
pub struct MprisPlayer {
    pub playback_status: PlaybackStatus,
    pub loop_status: LoopStatus,
    pub shuffle: bool,
    pub volume: f64,
    pub metadata: Metadata,
}

/// The active player, or `None` if there are no players. Changes whenever
/// anything except the position changes.
pub static DEFAULT_PLAYER: LazyLock<Mutable<Option<MprisPlayer>>> = LazyLock::new(|| Mutable::new(None));

/// The position of the active player, in microseconds. Kept separate from the
/// rest of the state since it changes constantly while playing.
pub static POSITION: LazyLock<Mutable<i64>> = LazyLock::new(|| Mutable::new(0));

static MPRIS: OnceLock<Mpris> = OnceLock::new();

pub fn get_default_player() -> Option<MprisPlayer> {
    DEFAULT_PLAYER.get_cloned()
}

pub fn get_position() -> i64 {
    POSITION.get()
}

pub fn subscribe_to_default_player_changes<F>(callback: F)
where
    F: Fn(Option<MprisPlayer>) + 'static,
{
    glib::spawn_future_local(DEFAULT_PLAYER.signal_cloned().for_each(move |player| {
        callback(player);
        async {}
    }));
}

fn apply(update: Update) {
    let mut player = DEFAULT_PLAYER.lock_mut();

    match update {
        Update::ActivePlayer(name) => {
            *player = name.map(|_| MprisPlayer::default());
            POSITION.set(0);
        },

        Update::Position(position) => {
            POSITION.set(i64::try_from(position.as_micros()).unwrap_or(i64::MAX));
        },

        // Rate is already accounted for by the crate in the positions it reports
        Update::PlaybackRate(_) => {},

        Update::PlaybackStatus(status) => if let Some(player) = player.as_mut() {
            player.playback_status = status;
        },

        Update::LoopStatus(loop_status) => if let Some(player) = player.as_mut() {
            player.loop_status = loop_status;
        },

        Update::Shuffle(shuffle) => if let Some(player) = player.as_mut() {
            player.shuffle = shuffle;
        },

        Update::Volume(volume) => if let Some(player) = player.as_mut() {
            player.volume = volume;
        },

        Update::Metadata(metadata) => if let Some(player) = player.as_mut() {
            player.metadata = Metadata {
                art_url: metadata.art_uri,
                album: metadata.album,
                artist: (!metadata.artists.is_empty()).then_some(metadata.artists),
                title: metadata.title,
                length: metadata.length.map(|l| i64::try_from(l.as_micros()).unwrap_or(i64::MAX)),
            };
        },
    }
}

/// Runs `action` on the active player in the background, logging failures.
fn control<F, Fut>(description: &'static str, action: F)
where
    F: FnOnce(Arc<Player>) -> Fut + Send + 'static,
    Fut: Future<Output = mpris::Result<()>> + Send,
{
    tokio::spawn(async move {
        let Some(player) = get_active_player().await else {
            return warn!("No MPRIS player available to {description}");
        };

        if let Err(e) = action(player).await {
            error!(%e, "Failed to {description}");
        }
    });
}

async fn get_active_player() -> Option<Arc<Player>> {
    MPRIS.get()?.get_active_player().await
}

pub fn play_pause() {
    control("toggle play/pause", |p| async move { p.play_pause().await });
}

pub fn play() {
    control("play", |p| async move { p.play().await });
}

pub fn pause() {
    control("pause", |p| async move { p.pause().await });
}

pub fn next() {
    control("skip to next track", |p| async move { p.next().await });
}

pub fn previous() {
    control("skip to previous track", |p| async move { p.previous().await });
}

/// Seeks relative to the current position, by `microseconds` (negative to go back).
pub fn seek(microseconds: i64) {
    let direction = if microseconds < 0 { SeekDirection::Backwards } else { SeekDirection::Forwards };
    let duration = Duration::from_micros(microseconds.unsigned_abs());

    control("seek", move |p| async move { p.seek(duration, direction).await });
}

pub fn set_loop_status(loop_status: LoopStatus) {
    control("set loop status", move |p| async move { p.set_loop_status(loop_status).await });
}

pub fn set_shuffle(shuffle: bool) {
    control("set shuffle", move |p| async move { p.set_shuffle(shuffle).await });
}

pub fn adjust_volume(delta: f64) {
    control("adjust volume", move |p| async move { p.adjust_volume(delta).await });
}

pub async fn activate() {
    let mpris = match Mpris::new().await {
        Ok(mpris) => mpris,
        Err(e) => return error!(%e, "Failed to start MPRIS, media players will not be available"),
    };

    let mut updates = mpris.subscribe();
    let Ok(()) = MPRIS.set(mpris) else { return };

    // Catch up with the players that were already running
    if let Some(mpris) = MPRIS.get() {
        mpris.sync().await;
    }

    tokio::spawn(async move {
        loop {
            match updates.recv().await {
                Ok(update) => apply(update),
                // We fell behind, ask for the current state instead of guessing what was missed
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => if let Some(mpris) = MPRIS.get() {
                    mpris.sync().await;
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Monitor IPC commands for controlling MPRIS players
    crate::ipc::listen_for_messages_local(|message| match message.as_str() {
        "mpris_next" => next(),
        "mpris_previous" => previous(),
        "mpris_play_pause" => play_pause(),
        "mpris_play" => play(),
        "mpris_pause" => pause(),
        "mpris_volume_up" => adjust_volume(0.05),
        "mpris_volume_down" => adjust_volume(-0.05),
        _ => {},
    });
}
