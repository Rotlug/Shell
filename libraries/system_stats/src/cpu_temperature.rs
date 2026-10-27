//! Reads the CPU temperature from the kernel's hwmon interface, which is what libsensors
//! reads from as well.
use std::{
    fs,
    path::{Path, PathBuf},
};

const HWMON_PATH: &str = "/sys/class/hwmon";

/// The chips that report the CPU temperature, the ones listed first are preferred.
/// `acpitz` is the generic thermal zone, which is only a rough approximation.
const CHIPS: [&str; 5] = ["k10temp", "zenpower", "coretemp", "cpu_thermal", "acpitz"];

/// The sensors of a chip that represent the temperature of the CPU as a whole, in order of
/// preference: `Tctl` and `Tdie` on AMD, `Package id 0` on Intel.
const LABELS: [&str; 3] = ["Tctl", "Tdie", "Package id 0"];

#[derive(Debug, Default)]
pub struct CpuTemperature {
    input: Option<PathBuf>,
    looked: bool,
}

impl CpuTemperature {
    /// In degrees Celsius, or `None` if this machine has no sensor we know about.
    pub fn read(&mut self) -> Option<f64> {
        if !self.looked {
            self.looked = true;
            self.input = find_input(Path::new(HWMON_PATH));
        }

        let value = self
            .input
            .as_ref()
            .and_then(|input| read_input(input));

        // The sensor may have moved (or vanished), look for it again next time
        if value.is_none() && self.input.is_some() {
            self.input = None;
            self.looked = false;
        }

        value
    }
}

fn read_input(input: &Path) -> Option<f64> {
    let millidegrees = fs::read_to_string(input).ok()?.trim().parse::<f64>().ok()?;

    Some(millidegrees / 1000.0)
}

fn find_input(hwmon: &Path) -> Option<PathBuf> {
    let chips: Vec<(String, PathBuf)> = fs::read_dir(hwmon)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = fs::read_to_string(entry.path().join("name")).ok()?;
            Some((name.trim().to_string(), entry.path()))
        })
        .collect();

    CHIPS.iter().find_map(|wanted| {
        let (_, chip) = chips.iter().find(|(name, _)| name == wanted)?;

        find_package_sensor(chip).or_else(|| {
            let first = chip.join("temp1_input");
            first.exists().then_some(first)
        })
    })
}

/// Looks through the `temp*_label` files of a chip for the sensor of the whole CPU.
fn find_package_sensor(chip: &Path) -> Option<PathBuf> {
    let labels: Vec<(String, String)> = fs::read_dir(chip)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let sensor = file_name.strip_suffix("_label")?;
            let label = fs::read_to_string(entry.path()).ok()?;

            Some((sensor.to_string(), label.trim().to_string()))
        })
        .collect();

    LABELS.iter().find_map(|wanted| {
        let (sensor, _) = labels.iter().find(|(_, label)| label == wanted)?;
        let input = chip.join(format!("{sensor}_input"));

        input.exists().then_some(input)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chip(root: &Path, dir: &str, name: &str, sensors: &[(&str, Option<&str>, &str)]) {
        let chip = root.join(dir);
        fs::create_dir_all(&chip).unwrap();
        fs::write(chip.join("name"), format!("{name}\n")).unwrap();

        for (sensor, label, value) in sensors {
            fs::write(chip.join(format!("{sensor}_input")), format!("{value}\n")).unwrap();

            if let Some(label) = label {
                fs::write(chip.join(format!("{sensor}_label")), format!("{label}\n")).unwrap();
            }
        }
    }

    #[test]
    fn prefers_the_cpu_package_over_other_chips() {
        let root = std::env::temp_dir().join(format!("system_stats_hwmon_{}", std::process::id()));
        chip(&root, "hwmon0", "nvme", &[("temp1", Some("Composite"), "30000")]);
        chip(&root, "hwmon1", "acpitz", &[("temp1", None, "50000")]);
        chip(
            &root,
            "hwmon2",
            "coretemp",
            &[("temp1", Some("Package id 0"), "61000"), ("temp2", Some("Core 0"), "55000")],
        );

        let input = find_input(&root).unwrap();
        assert_eq!(input, root.join("hwmon2/temp1_input"));
        assert_eq!(read_input(&input), Some(61.0));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn picks_tctl_on_amd() {
        let root = std::env::temp_dir().join(format!("system_stats_hwmon_amd_{}", std::process::id()));
        chip(
            &root,
            "hwmon0",
            "k10temp",
            &[("temp1", Some("Tctl"), "45500"), ("temp3", Some("Tccd1"), "40000")],
        );

        assert_eq!(find_input(&root), Some(root.join("hwmon0/temp1_input")));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_sensor() {
        assert_eq!(find_input(Path::new("/nonexistent")), None);
    }
}
