//! Keeps track of where in the track a player is.
//!
//! The `Position` of a player is not announced when it changes: it is only read when asked
//! for, and `Seeked` is only sent when something jumps around. So the position is estimated
//! from the last one that is known and the time that went by, and that estimate is checked
//! against the player now and then, and whenever the track or the playback changes.
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use tokio::{
    sync::broadcast,
    task::JoinHandle,
    time::{self, Instant},
};

use crate::{Update, generated::player::PlayerProxy};

/// How often the estimated position is reported.
const UPDATE_INTERVAL: Duration = Duration::from_millis(500);

/// How many reports it takes until the position is read from the player again, to correct
/// the estimate (players stall, buffer, and get seeked by other clients that don't announce it).
const READ_EVERY: u32 = 10;

/// How long after a track changed to ask the player where it is: players that were just told to
/// go to the next track don't always say so right away.
const READ_AFTER_TRACK_CHANGE: Duration = Duration::from_millis(300);

/// How long to wait for a player to say where it is: one that is busy shouldn't hold anything up.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// What tells one track from another.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    pub id: Option<String>,
    pub title: Option<String>,
}

struct State {
    /// Where the track was at `updated`
    position: Duration,
    updated: Instant,
    /// How fast the playback goes, e.g. 1.0 or 2.0 (the `Rate` of the player)
    rate: f64,
    playing: bool,
    length: Option<Duration>,
    track: Option<Track>,
}

impl State {
    fn current(&self) -> Duration {
        let rate = if self.playing { self.rate } else { 0.0 };
        let position = self.position + self.updated.elapsed().mul_f64(rate.max(0.0));

        self.length.map_or(position, |length| position.min(length))
    }

    fn set(&mut self, position: Duration) {
        self.position = position;
        self.updated = Instant::now();
    }

    /// Changes how fast the position moves, without moving it
    fn change_pace(&mut self, change: impl FnOnce(&mut Self)) {
        self.position = self.current();
        self.updated = Instant::now();
        change(self);
    }
}

/// Everything that needs to know about the position of a player: the task that reports it
/// and the ones that hear about it changing.
pub struct Shared {
    state: Mutex<State>,
    tx: broadcast::Sender<Update>,
    proxy: PlayerProxy<'static>,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        // Nothing in here can panic while it holds the state
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The (estimated) current position.
    pub fn current(&self) -> Duration {
        self.state().current()
    }

    /// Reports the position. This is done while holding the state, so what is reported always
    /// comes in the order things happened: an old estimate can't come after a newer position.
    fn report(&self, state: &State) {
        let _ = self.tx.send(Update::Position(state.current()));
    }

    /// The player is at `position`, for sure.
    pub fn set(&self, position: Duration) {
        let mut state = self.state();
        state.set(position);
        self.report(&state);
    }

    /// Tells where the player is now, as it is believed to be.
    pub fn announce(&self) {
        self.report(&self.state());
    }

    /// The player starts or stops playing, or plays at another `rate`.
    pub fn set_pace(&self, playing: Option<bool>, rate: Option<f64>) {
        self.state().change_pace(|state| {
            state.playing = playing.unwrap_or(state.playing);
            state.rate = rate.unwrap_or(state.rate);
        });
    }

    /// The metadata of the player changed. If that is another track, it starts from the
    /// beginning.
    pub fn metadata_changed(self: &Arc<Self>, track: Track, length: Option<Duration>) {
        let mut state = self.state();
        state.length = length;

        if state.track.as_ref() == Some(&track) {
            return;
        }

        // The very first track we hear about keeps the position it has
        let first = state.track.replace(track).is_none();
        if first {
            return;
        }

        state.set(Duration::ZERO);
        self.report(&state);
        drop(state);

        let shared = self.clone();
        tokio::spawn(async move {
            time::sleep(READ_AFTER_TRACK_CHANGE).await;
            shared.read().await;
        });
    }

    /// Asks the player where it is. When it doesn't tell (it doesn't have to), the estimate stays.
    pub async fn read(&self) {
        let Ok(Ok(position)) = time::timeout(READ_TIMEOUT, self.proxy.position()).await else {
            return;
        };

        self.set(Duration::from_micros(u64::try_from(position).unwrap_or(0)));
    }
}

pub struct Position {
    shared: Arc<Shared>,
    handle: JoinHandle<()>,
}

impl Position {
    pub async fn new(proxy: PlayerProxy<'static>, tx: broadcast::Sender<Update>) -> crate::Result<Self> {
        let position = proxy.position().await?;

        // Position only advances while the player is actually playing
        let playing = proxy.playback_status().await.is_ok_and(|status| status == "Playing");
        let rate = proxy.rate().await.unwrap_or(1.0);

        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                position: Duration::from_micros(u64::try_from(position).unwrap_or(0)),
                updated: Instant::now(),
                rate,
                playing,
                length: None,
                track: None,
            }),
            tx,
            proxy,
        });

        /* Task that sends position updates every `UPDATE_INTERVAL` */
        let updater_shared = shared.clone();
        let handle = tokio::spawn(async move {
            let mut reports = 0_u32;

            loop {
                updater_shared.announce();

                time::sleep(UPDATE_INTERVAL).await;

                reports += 1;
                if reports % READ_EVERY == 0 && updater_shared.state().playing {
                    updater_shared.read().await;
                }
            }
        });

        Ok(Self { shared, handle })
    }

    /// What the tasks that hear about the player use to tell where it is.
    pub fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }

    pub fn get_position(&self) -> Duration {
        self.shared.current()
    }
}

impl Drop for Position {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl std::fmt::Debug for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Position").field("current", &self.get_position()).finish_non_exhaustive()
    }
}
