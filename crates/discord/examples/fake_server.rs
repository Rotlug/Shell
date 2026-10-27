//! A stand-in for the discord applet's server, to try the crate (or the shell) with:
//! `fake_server [port]` listens on 49153 by default, and plays out people joining and leaving
//! a call. Avatars are whatever is given in `AVATARS`, or `avatar-a.png`... in the current
//! directory.
use std::time::Duration;

use futures::SinkExt;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

fn list(users: &[(&str, &str)]) -> String {
    let users: Vec<String> = users
        .iter()
        .map(|(name, avatar)| format!(r#"{{"name":"{name}","avatar":"{avatar}"}}"#))
        .collect();

    format!("[{}]", users.join(","))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port = std::env::args().nth(1).unwrap_or_else(|| "49153".to_string());
    let avatar = |letter: &str| {
        std::env::var(format!("AVATAR_{}", letter.to_uppercase()))
            .unwrap_or_else(|_| format!("avatar-{letter}.png"))
    };
    let (a, b, c) = (avatar("a"), avatar("b"), avatar("c"));

    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    println!("listening on {port}");

    while let Ok((stream, _)) = listener.accept().await {
        let (a, b, c) = (a.clone(), b.clone(), c.clone());

        tokio::spawn(async move {
            let Ok(mut ws) = accept_async(stream).await else {
                return;
            };

            let steps = [
                list(&[("alice", &a)]),
                list(&[("alice", &a), ("bob", &b)]),
                list(&[("alice", &a), ("bob", &b), ("carol", &c)]),
                // bob changes avatar
                list(&[("alice", &a), ("bob", &c), ("carol", &c)]),
                list(&[("carol", &c)]),
                list(&[]),
            ];

            for step in steps {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if ws.send(Message::Text(step.into())).await.is_err() {
                    return;
                }
            }

            tokio::time::sleep(Duration::from_secs(30)).await;
        });
    }

    Ok(())
}
