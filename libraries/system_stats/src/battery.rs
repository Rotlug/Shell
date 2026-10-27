use std::{fs, path::Path};

use crate::BatteryInfo;

const POWER_SUPPLY_PATH: &str = "/sys/class/power_supply";

/// Reads the first `BAT*` entry under `/sys/class/power_supply`.
pub fn read() -> Option<BatteryInfo> {
    read_from(Path::new(POWER_SUPPLY_PATH))
}

fn read_from(power_supply: &Path) -> Option<BatteryInfo> {
    let mut batteries: Vec<_> = fs::read_dir(power_supply)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("BAT"))
        .collect();

    // Not in any particular order otherwise
    batteries.sort_by_key(fs::DirEntry::file_name);

    let path = batteries.first()?.path();
    let percentage = fs::read_to_string(path.join("capacity"))
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()?;
    let status = fs::read_to_string(path.join("status")).ok()?;

    Some(BatteryInfo {
        percentage,
        charging: status.trim() == "Charging",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_first_battery() {
        let dir = std::env::temp_dir().join(format!("system_stats_battery_{}", std::process::id()));
        let battery = dir.join("BAT0");
        fs::create_dir_all(&battery).unwrap();
        fs::create_dir_all(dir.join("AC")).unwrap();
        fs::write(battery.join("capacity"), "83\n").unwrap();
        fs::write(battery.join("status"), "Charging\n").unwrap();

        assert_eq!(
            read_from(&dir),
            Some(BatteryInfo { percentage: 83.0, charging: true })
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn no_battery() {
        assert_eq!(read_from(Path::new("/nonexistent")), None);
    }
}
