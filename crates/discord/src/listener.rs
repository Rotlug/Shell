use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::{
    sync::mpsc::{Receiver, Sender, channel},
    task::JoinHandle,
    time,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{event::Event, user::User};

/// Listens to the discord applet's server, and tells who is there.
pub struct Listener {
    handles: Vec<JoinHandle<()>>,
}

impl Listener {
    /// Connects to the server at `url` (e.g. `ws://localhost:49152`), and keeps doing so
    /// until it is dropped: the server may not be running yet, and may come and go.
    ///
    /// Returns a `Receiver` which can be used to listen to the users joining and leaving.
    /// Users that were there when the connection was lost leave, so that nobody is left over
    /// from a connection that is gone.
    #[must_use]
    pub fn new(url: String) -> (Self, Receiver<Event>) {
        let (event_tx, event_rx) = channel(16);
        let mut handles = vec![];

        let recv_task = tokio::spawn(async move {
            let mut old_user_list: Vec<User> = vec![];

            let mut backoff = 1; // Exponential backoff

            loop {
                if let Ok((ws_stream, _)) = connect_async(&url).await {
                    // Connection Successful, reset backoff
                    backoff = 1;

                    let (mut sender, mut receiver) = ws_stream.split();

                    let _ = sender.send(Message::Text("Hi server :)".into())).await;

                    while let Some(Ok(msg)) = receiver.next().await {
                        if let Message::Text(text) = msg {
                            // Parse json into Vec<User> and send diff to rx
                            if let Ok(new_user_list) = serde_json::from_str::<Vec<User>>(&text) {
                                if !send_user_events(&old_user_list, &new_user_list, &event_tx).await {
                                    return;
                                }

                                old_user_list = new_user_list;
                            }
                        }
                    }

                    // Connection closed, nobody can be told to be there anymore
                    if !send_user_events(&old_user_list, &[], &event_tx).await {
                        return;
                    }
                    old_user_list.clear();
                }

                // Retry connection in 60s max
                time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
            }
        });

        handles.push(recv_task);

        (Self { handles }, event_rx)
    }
}

/// Tells what changed between two lists of users. Users leave before others join, so that a
/// user whose avatar changed (who leaves under their name, and joins again) ends up there.
///
/// Returns `false` once nobody is listening anymore.
async fn send_user_events(old_list: &[User], new_list: &[User], event_tx: &Sender<Event>) -> bool {
    for user in old_list {
        if !new_list.contains(user)
            && event_tx.send(Event::UserLeft(user.name.clone())).await.is_err()
        {
            return false;
        }
    }

    for user in new_list {
        if !old_list.contains(user) && event_tx.send(Event::UserJoined(user.clone())).await.is_err() {
            return false;
        }
    }

    true
}

impl Drop for Listener {
    fn drop(&mut self) {
        for handle in &self.handles {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(name: &str, avatar: &str) -> User {
        User { name: name.to_string(), avatar: avatar.to_string() }
    }

    async fn events(old: &[User], new: &[User]) -> Vec<Event> {
        let (tx, mut rx) = channel(16);
        assert!(send_user_events(old, new, &tx).await);
        drop(tx);

        let mut events = vec![];
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn leaves_come_before_joins() {
        // A changed avatar is a user leaving and joining under the same name
        let events = events(&[user("a", "old"), user("b", "x")], &[user("a", "new"), user("b", "x")]).await;

        assert!(matches!(&events[..], [Event::UserLeft(name), Event::UserJoined(joined)]
            if name == "a" && joined.avatar == "new"));
    }

    #[tokio::test]
    async fn everyone_leaves_when_the_list_is_emptied() {
        let events = events(&[user("a", "1"), user("b", "2")], &[]).await;

        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|event| matches!(event, Event::UserLeft(_))));
    }

    #[tokio::test]
    async fn nothing_changed() {
        assert!(events(&[user("a", "1")], &[user("a", "1")]).await.is_empty());
    }
}
