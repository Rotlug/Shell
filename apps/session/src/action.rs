use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Lock,
    Logout,
    Suspend,
    Hibernate,
    Reboot,
    Shutdown,
}

impl Action {
    /// The shell command that does it. The first thing that works counts, so that this works
    /// with and without systemd.
    const fn command(self) -> &'static str {
        match self {
            Self::Lock => "loginctl lock-session",
            Self::Logout => "pkill Hyprland || loginctl terminate-user $USER",
            Self::Suspend => "systemctl suspend || loginctl suspend",
            Self::Hibernate => "systemctl hibernate || loginctl hibernate",
            Self::Reboot => "systemctl reboot || loginctl reboot",
            Self::Shutdown => "systemctl poweroff || loginctl poweroff",
        }
    }

    /// Does it, and waits for that to be over.
    pub fn run(self) -> std::io::Result<()> {
        let status = Command::new("bash").arg("-c").arg(self.command()).status()?;

        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!("`{}` failed: {status}", self.command())))
        }
    }
}
