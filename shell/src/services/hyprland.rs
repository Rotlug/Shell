//! Hyprland-specific utilities that don't fit the compositor-neutral
//! abstraction in `services::compositor` (hyprctl has no niri equivalent).

/// For hyprctl operations that are not directly implemented into hyprland-rs (e.g. getting active submap),
/// you may use this to invoke hyprctl directly.
pub fn call_hyprctl(command: &str) -> Option<String> {
    let output = std::process::Command::new("hyprctl")
        .arg(command)
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Basically call_hyprctl, but using it's built-in batch flag to run multiple commands in
/// one hyprctl call.
pub fn call_hyprctl_batch(commands: &[&str]) -> Option<Vec<String>> {
    let batch_command = commands.join(";");
    let output = std::process::Command::new("hyprctl")
        .arg("--batch")
        .arg(batch_command)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<String> = stdout.lines().map(|line| line.trim().to_owned()).collect();
    Some(results)
}
