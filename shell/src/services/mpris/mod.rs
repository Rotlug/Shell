//! MPRIS media player state, backed by the `mpris` crate.
//!
//! The crate reports updates for the active player over a channel; this keeps
//! the latest state in signals the UI can subscribe to, and turns the
//! player controls into fire-and-forget functions.
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};
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
    /// `None` when the player doesn't have a loop status to change
    pub loop_status: Option<LoopStatus>,
    /// `None` when the player doesn't have a shuffle to change
    pub shuffle: Option<bool>,
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

/// A position that the player was asked to go to, and when it was asked. Until the player
/// says it is there (or it takes too long), that is the position that is shown.
static SEEK: Mutex<Option<(i64, Instant)>> = Mutex::new(None);

/// How close to a position the player has to say it is for it to have got there, in microseconds
const SEEK_CONFIRMED_WITHIN: i64 = 1_500_000;

/// How long to wait for the player to get to a position, which it might never do
const SEEK_GUARD: Duration = Duration::from_secs(2);

/// Keeps `microseconds` within the track, and shows it right away: waiting for the player to
/// say that it got there makes seeking feel slow.
fn expect_position(microseconds: i64) -> i64 {
    let length = DEFAULT_PLAYER.lock_ref().as_ref().and_then(|player| player.metadata.length);
    let target = microseconds.clamp(0, length.unwrap_or(i64::MAX));

    *SEEK.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((target, Instant::now()));
    POSITION.set(target);

    target
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
            *SEEK.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            POSITION.set(0);
        },

        Update::Position(position) => {
            let mut position = i64::try_from(position.as_micros()).unwrap_or(i64::MAX);

            // Never past the end of the track
            if let Some(length) = player.as_ref().and_then(|player| player.metadata.length) {
                position = position.min(length);
            }

            let mut seek = SEEK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            match *seek {
                // Positions that were on their way before the player heard about the seek would take
                // the position back to where it was, until the player says where it really is
                Some((target, asked)) if (position - target).abs() > SEEK_CONFIRMED_WITHIN && asked.elapsed() < SEEK_GUARD => return,
                _ => *seek = None,
            }

            POSITION.set(position);
        },

        // Rate is already accounted for by the crate in the positions it reports
        Update::PlaybackRate(_) => {},

        Update::PlaybackStatus(status) => if let Some(player) = player.as_mut() {
            player.playback_status = status;
        },

        Update::LoopStatus(loop_status) => if let Some(player) = player.as_mut() {
            player.loop_status = Some(loop_status);
        },

        Update::Shuffle(shuffle) => if let Some(player) = player.as_mut() {
            player.shuffle = Some(shuffle);
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

    expect_position(get_position().saturating_add(microseconds));
    control("seek", move |p| async move { p.seek(duration, direction).await });
}

/// Seeks to a position of the track, in microseconds.
pub fn set_position(microseconds: i64) {
    let target = Duration::from_micros(expect_position(microseconds).unsigned_abs());

    control("set position", move |p| async move { p.set_position(target).await });
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
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test for all of it: what is tested is shared by the whole program
    #[test]
    fn positions_follow_the_player_but_not_stale_ones_after_a_seek() {
        apply(Update::ActivePlayer(Some("player".to_owned())));
        apply(Update::Metadata(mpris::Metadata { length: Some(Duration::from_secs(300)), ..Default::default() }));

        apply(Update::Position(Duration::from_secs(10)));
        assert_eq!(get_position(), 10_000_000);

        // Never past the end of the track, even if the player says so (the next track starts
        // from the start, but not every player says that)
        apply(Update::Position(Duration::from_secs(514)));
        assert_eq!(get_position(), 300_000_000);

        // The user seeks: the position is where they asked for right away
        assert_eq!(expect_position(200_000_000), 200_000_000);
        assert_eq!(get_position(), 200_000_000);

        // An estimate that was on its way before the player heard about it doesn't take it back
        apply(Update::Position(Duration::from_secs(11)));
        assert_eq!(get_position(), 200_000_000);

        // The player got there (about, it keeps playing), and it is believed again
        apply(Update::Position(Duration::from_millis(200_400)));
        assert_eq!(get_position(), 200_400_000);
        apply(Update::Position(Duration::from_secs(5)));
        assert_eq!(get_position(), 5_000_000);

        // Seeking to somewhere that isn't in the track is somewhere in it
        assert_eq!(expect_position(900_000_000), 300_000_000);
        assert_eq!(expect_position(-5), 0);

        // A new player starts over
        apply(Update::ActivePlayer(Some("other".to_owned())));
        assert_eq!(get_position(), 0);
        apply(Update::Position(Duration::from_secs(12)));
        assert_eq!(get_position(), 12_000_000);

        // What a player can't do isn't offered: it has no shuffle until it says it has one
        assert_eq!(get_default_player().unwrap().shuffle, None);
        assert_eq!(get_default_player().unwrap().loop_status, None);
        apply(Update::Shuffle(false));
        assert_eq!(get_default_player().unwrap().shuffle, Some(false));
    }
}
