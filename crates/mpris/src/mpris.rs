use futures::StreamExt;
use std::{collections::HashSet, sync::Arc};
use tokio::{
    sync::{Mutex, broadcast, mpsc},
    task::JoinHandle,
};
use tokio_stream::{StreamMap, wrappers::BroadcastStream};
use zbus::{Connection, fdo};

use crate::{PlaybackStatus, Update, player::Player};

const MPRIS_BUS: &str = "org.mpris.MediaPlayer2";

type Players = StreamMap<Arc<Player>, BroadcastStream<Update>>;
type ActivePlayer = Arc<Mutex<Option<Arc<Player>>>>;

/// This update enum is used to add/remove players from the `StreamMap`,
/// Since the `handle` holds the mutex for the entire time (idk)
/// players can't be added/removed directly from `watch_for_players`
/// therefore that method sends commands to the `handle` to update the `StreamMap`
/// accordingly.
enum PlayerUpdate {
    Added(Arc<Player>),
    /// The name of the player that disappeared from the bus.
    Removed(String),
}

/// The `Mpris` struct is used to receive updates from the MPRIS player that is
/// currently active: the one that is playing, or if none are, the one that
/// was last active.
pub struct Mpris {
    active_player: ActivePlayer,
    handle: JoinHandle<()>,
    tx: broadcast::Sender<Update>,
    players_set: Arc<Mutex<HashSet<Arc<Player>>>>,
}

impl Mpris {
    /// Construct a new MPRIS listener
    ///
    /// Updates sent before `subscribe` is called are lost, call `sync` after
    /// subscribing to get the current state.
    ///
    /// # Errors
    /// Errors when the dbus connection fails
    pub async fn new() -> crate::Result<Self> {
        let connection = Connection::session().await?;
        let active_player = Arc::new(Mutex::new(None));
        let players_set = Arc::new(Mutex::new(HashSet::new()));
        let (tx, _) = broadcast::channel(16);

        /* Construct Players Map*/
        let mut players: Players = StreamMap::new();
        for name in Self::get_current_players(&connection).await? {
            let Ok(player) = Player::new(name, &connection).await else {
                continue;
            };
            let player = Arc::new(player);

            players.insert(player.clone(), player.subscribe().into());
            players_set.lock().await.insert(player);
        }

        /* Start watching for updates */
        let active_player_handle = active_player.clone();
        let tx_handle = tx.clone();
        let mut rx = Self::watch_for_players(connection.clone()).await?;
        let players_set_handle = players_set.clone();
        let handle = tokio::spawn(async move {
            Self::select_active_player(&players, &active_player_handle, &tx_handle).await;

            loop {
                tokio::select! {
                    Some((player, upd)) = players.next() => {
                        let Ok(upd) = upd else {
                            continue;
                        };

                        // Only send updates if the player is the one thats
                        // currently active
                        let is_active = active_player_handle.lock().await.as_ref() == Some(&player);
                        if is_active {
                            let _ = tx_handle.send(upd.clone());
                        }

                        if let Update::PlaybackStatus(_) = upd {
                            Self::select_active_player(&players, &active_player_handle, &tx_handle).await;
                        }
                    },
                    Some(upd) = rx.recv() => {
                        /* Add/Remove players from the map */
                        match upd {
                            PlayerUpdate::Added(p) => {
                                players.insert(p.clone(), p.subscribe().into());
                                players_set_handle.lock().await.insert(p);
                            },
                            PlayerUpdate::Removed(name) => {
                                let removed = {
                                    let mut set = players_set_handle.lock().await;
                                    let found = set.iter().find(|p| p.name == name).cloned();
                                    if let Some(found) = &found {
                                        set.remove(found);
                                    }
                                    found
                                };

                                if let Some(removed) = removed {
                                    players.remove(&removed);
                                }
                            }
                        }

                        Self::select_active_player(&players, &active_player_handle, &tx_handle).await;
                    }
                }
            }
        });

        Ok(Self {
            active_player,
            handle,
            tx,
            players_set,
        })
    }

    /// Picks the active player, and notifies subscribers if it changed:
    ///
    /// 1. The current active player, if it is still playing.
    /// 2. Otherwise, any player that is playing.
    /// 3. Otherwise, the current active player, if it still exists.
    /// 4. Otherwise, any player (or none, if there aren't any).
    async fn select_active_player(
        players: &Players,
        active_player: &ActivePlayer,
        tx: &broadcast::Sender<Update>,
    ) {
        let current = active_player.lock().await.clone();

        // Sorted, so that the choice between equal candidates is stable
        let mut candidates: Vec<&Arc<Player>> = players.keys().collect();
        candidates.sort_by(|a, b| a.name.cmp(&b.name));

        let mut playing = None;
        for player in &candidates {
            if matches!(player.get_playback_status().await, Ok(PlaybackStatus::Playing)) {
                if current.as_ref() == Some(*player) {
                    playing = Some(*player);
                    break;
                }

                playing = playing.or(Some(*player));
            }
        }

        let selected = playing
            .or_else(|| candidates.iter().find(|p| current.as_ref() == Some(**p)).copied())
            .or_else(|| candidates.first().copied())
            .cloned();

        if selected == current {
            return;
        }

        *active_player.lock().await = selected.clone();
        let _ = tx.send(Update::ActivePlayer(selected.as_ref().map(|p| p.name.clone())));

        // The new active player's updates will be forwarded from now on
        if let Some(selected) = selected {
            let _ = selected.sync().await;
        }
    }

    /// Finds all active players and returns their names
    ///
    /// # Errors
    /// errors when the dbus connection fails
    async fn get_current_players(connection: &Connection) -> crate::Result<Vec<String>> {
        let fdo_proxy = fdo::DBusProxy::new(connection).await?;

        let names: Vec<String> = fdo_proxy
            .list_names()
            .await?
            .iter()
            .filter(|name| name.starts_with(MPRIS_BUS))
            .map(std::string::ToString::to_string)
            .collect();

        Ok(names)
    }

    /// Watch for when players are added/removed
    ///
    /// # Errors
    /// errors when the dbus connection fails
    async fn watch_for_players(
        connection: Connection,
    ) -> crate::Result<mpsc::Receiver<PlayerUpdate>> {
        let fdo_proxy = fdo::DBusProxy::new(&connection).await?;

        let (tx, rx) = mpsc::channel(16);

        let mut name_owner_changed = fdo_proxy.receive_name_owner_changed().await?;

        tokio::spawn(async move {
            while let Some(upd) = name_owner_changed.next().await {
                let Ok(args) = upd.args() else {
                    continue;
                };

                if !args.name.starts_with(MPRIS_BUS) {
                    continue;
                }

                let name = args.name.to_string();

                if args.old_owner.is_none() && args.new_owner.is_some() {
                    let Ok(player) = Player::new(name, &connection).await else {
                        continue;
                    };

                    let _ = tx.send(PlayerUpdate::Added(Arc::new(player))).await;
                } else if args.new_owner.is_none() && args.old_owner.is_some() {
                    // The player is already gone, so there is nothing to
                    // construct (or query), only to forget about.
                    let _ = tx.send(PlayerUpdate::Removed(name)).await;
                }
            }
        });

        Ok(rx)
    }

    /// Subscribe to updates from the ACTIVE player
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Update> {
        self.tx.subscribe()
    }

    /// Re-sends the current state: the active player, followed by all of its
    /// properties. Call this after `subscribe` to catch up with what happened
    /// before subscribing.
    pub async fn sync(&self) {
        let Some(active) = self.get_active_player().await else {
            let _ = self.tx.send(Update::ActivePlayer(None));
            return;
        };

        let _ = self.tx.send(Update::ActivePlayer(Some(active.name.clone())));
        let _ = active.sync().await;
    }

    /// Returns the active player, or `None` if there are no players.
    #[must_use]
    pub async fn get_active_player(&self) -> Option<Arc<Player>> {
        self.active_player.lock().await.clone()
    }

    /// Returns a `HashSet` of all players currently tracked by the struct
    #[must_use]
    pub async fn get_players(&self) -> HashSet<Arc<Player>> {
        self.players_set.lock().await.clone()
    }

    /// Finds and returns a player, give its `name`.
    /// If no match was found, returns `None`.
    #[must_use]
    pub async fn get_player(&self, name: &str) -> Option<Arc<Player>> {
        self.players_set
            .lock()
            .await
            .iter()
            .find(|p| p.name == name)
            .cloned()
    }
}

impl Drop for Mpris {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
