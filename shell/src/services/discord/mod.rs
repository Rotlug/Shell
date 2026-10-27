//! Who is in the Discord call, backed by the `discord` crate.
//!
//! The crate reports the users over a channel; this keeps the users that are there, so that
//! whoever starts listening later can catch up, and fans the events out.
pub mod avatar;

use std::sync::{LazyLock, OnceLock, RwLock};
use async_broadcast::Receiver;
use discord::Listener;

pub use discord::{Event, user::User};

use crate::utils::broadcast::BroadcastChannel;

/// Where the discord applet's server listens
const SERVER_URL: &str = "ws://localhost:49152";

static CHANNEL: LazyLock<BroadcastChannel<Event>> = LazyLock::new(|| BroadcastChannel::new(32));
static USERS: LazyLock<RwLock<Vec<User>>> = LazyLock::new(|| RwLock::new(Vec::new()));
static LISTENER: OnceLock<Listener> = OnceLock::new();

fn apply(event: &Event) {
    let Ok(mut users) = USERS.write() else {
        return;
    };

    match event {
        // Users are told apart by name, so a user that joins again replaces the old entry
        Event::UserJoined(user) => {
            users.retain(|existing| existing.name != user.name);
            users.push(user.clone());
        },
        Event::UserLeft(name) => users.retain(|user| user.name != *name),
    }
}

pub fn activate() {
    let (listener, mut events) = Listener::new(SERVER_URL.to_owned());

    if LISTENER.set(listener).is_err() {
        return;
    }

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            apply(&event);
            CHANNEL.send(event).await;
        }
    });
}

/// Subscribes to users joining and leaving. Users that are there already are not announced,
/// get those from `users`.
pub fn subscribe() -> Receiver<Event> {
    CHANNEL.subscribe()
}

/// The users that are there.
pub fn users() -> Vec<User> {
    USERS.read().map(|users| users.clone()).unwrap_or_default()
}
