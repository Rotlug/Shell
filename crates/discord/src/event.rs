use crate::user::User;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    UserJoined(User),
    UserLeft(String),
}
