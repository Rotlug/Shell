use crate::Profile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The active profile changed, it is `None` when power-profiles-daemon isn't running.
    ActiveProfile(Option<Profile>),
}
