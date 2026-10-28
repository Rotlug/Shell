use std::{collections::HashMap, str::FromStr, time::Duration};

use zbus::zvariant::{self, Array};

#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    /// The player that updates are being reported for has changed (`None` when
    /// there are no players left). A full state update for the new player
    /// follows.
    ActivePlayer(Option<String>),
    PlaybackStatus(PlaybackStatus),
    PlaybackRate(f64),
    LoopStatus(LoopStatus),
    Shuffle(bool),
    Volume(f64),
    Position(Duration),
    Metadata(Metadata),
}

/* PlaybackUpdate */
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl FromStr for PlaybackStatus {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Playing" => Ok(Self::Playing),
            "Paused" => Ok(Self::Paused),
            "Stopped" => Ok(Self::Stopped),
            _ => Err(crate::Error::ParseError(s.to_string())),
        }
    }
}

/* LoopStatus */
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoopStatus {
    #[default]
    None,
    Track,
    Playlist,
}

impl LoopStatus {
    /// The string the `LoopStatus` property uses for this status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Track => "Track",
            Self::Playlist => "Playlist",
        }
    }
}

impl FromStr for LoopStatus {
    type Err = crate::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "None" => Ok(Self::None),
            "Track" => Ok(Self::Track),
            "Playlist" => Ok(Self::Playlist),
            _ => Err(crate::Error::ParseError(s.to_string())),
        }
    }
}

/* Metadata */
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    /// What identifies the track for the player (an object path), `SetPosition` needs it
    pub track_id: Option<String>,
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub art_uri: Option<String>,
    pub length: Option<Duration>,
}

/// Players are free to leave out any of the metadata keys, so nothing here is
/// required to be present.
impl From<HashMap<String, zvariant::OwnedValue>> for Metadata {
    fn from(value: HashMap<String, zvariant::OwnedValue>) -> Self {
        let string = |key: &str| {
            value
                .get(key)
                .and_then(|v| v.downcast_ref::<String>().ok())
                .filter(|s| !s.is_empty())
        };

        let artists = value
            .get("xesam:artist")
            .and_then(|v| v.downcast_ref::<Array>().ok())
            .map(|artists| {
                artists
                    .iter()
                    .filter_map(|artist| artist.downcast_ref::<String>().ok())
                    .collect()
            })
            .unwrap_or_default();

        let length = value
            .get("mpris:length")
            .and_then(|v| {
                v.downcast_ref::<i64>()
                    .ok()
                    .and_then(|micros| u64::try_from(micros).ok())
                    .or_else(|| v.downcast_ref::<u64>().ok())
            })
            .map(Duration::from_micros);

        // Most players send an object path, but some send a plain string
        let track_id = value.get("mpris:trackid").and_then(|v| {
            v.downcast_ref::<zvariant::ObjectPath>()
                .map(|path| path.to_string())
                .or_else(|_| v.downcast_ref::<String>())
                .ok()
        });

        Self {
            track_id,
            title: string("xesam:title"),
            artists,
            album: string("xesam:album"),
            art_uri: string("mpris:artUrl"),
            length,
        }
    }
}
