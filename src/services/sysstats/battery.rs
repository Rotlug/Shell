const POWER_SUPPLY_PATH: &str = "/sys/class/power_supply";

pub struct BatteryInfo {
    pub percentage: f64,
    pub charging: bool,
}

/// Reads the first `BAT*` entry under `/sys/class/power_supply`.
pub fn read_battery() -> Option<BatteryInfo> {
    let entries = std::fs::read_dir(POWER_SUPPLY_PATH).ok()?;

    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("BAT") {
            continue;
        }

        let path = entry.path();
        let percentage = std::fs::read_to_string(path.join("capacity")).ok()?
            .trim()
            .parse::<f64>()
            .ok()?;
        let status = std::fs::read_to_string(path.join("status")).ok()?;

        return Some(BatteryInfo {
            percentage,
            charging: status.trim() == "Charging",
        });
    }

    None
}
