//! Kernel power_supply entries that belong to a HID device.
//!
//! The kernel HID++ driver registers a battery under
//! `/sys/class/power_supply/<name>`, whose `device` link points at the HID
//! device directory (e.g. `0005:046D:B034.0003`). Matching on that directory
//! name links a battery to the hidraw node without relying on numbers.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::error::TransportError;

/// A power_supply entry. Only non-identifying attributes are read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerSupply {
    /// Entry name, e.g. `hidpp_battery_0`.
    pub name: String,
    /// `capacity`, percent.
    pub capacity: Option<u8>,
    /// `capacity_level`, e.g. `Normal`.
    pub capacity_level: Option<String>,
    /// `status`, e.g. `Discharging`.
    pub status: Option<String>,
    /// `scope`, e.g. `Device`.
    pub scope: Option<String>,
}

/// Power supplies on the running system whose device is `hid_device`.
pub fn power_supplies_for(hid_device: &str) -> Result<Vec<PowerSupply>, TransportError> {
    power_supplies_for_in(Path::new("/"), hid_device)
}

/// Same as [`power_supplies_for`], under `root` (for tests).
pub fn power_supplies_for_in(
    root: &Path,
    hid_device: &str,
) -> Result<Vec<PowerSupply>, TransportError> {
    let dir = root.join("sys/class/power_supply");
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        // No power_supply class at all just means no batteries.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => {
            return Err(TransportError::Sysfs {
                path: dir,
                reason: e.to_string(),
            });
        }
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let parent = fs::canonicalize(path.join("device"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        if parent.as_deref() != Some(hid_device) {
            continue;
        }
        let read = |attr: &str| {
            fs::read_to_string(path.join(attr))
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        };
        out.push(PowerSupply {
            name: entry.file_name().to_string_lossy().into_owned(),
            capacity: read("capacity").and_then(|s| s.parse().ok()),
            capacity_level: read("capacity_level"),
            status: read("status"),
            scope: read("scope"),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;
    use std::os::unix::fs::symlink;

    fn add(root: &Path, name: &str, device_dir: &str, attrs: &[(&str, &str)]) {
        let dev = root.join("sys/devices/virtual").join(device_dir);
        let psy = dev.join("power_supply").join(name);
        fs::create_dir_all(&psy).unwrap();
        symlink(&dev, psy.join("device")).unwrap();
        for (k, v) in attrs {
            fs::write(psy.join(k), format!("{v}\n")).unwrap();
        }
        let class = root.join("sys/class/power_supply");
        fs::create_dir_all(&class).unwrap();
        symlink(&psy, class.join(name)).unwrap();
    }

    #[test]
    fn finds_battery_of_hid_device() {
        let tmp = TempDir::new();
        add(
            &tmp.0,
            "hidpp_battery_0",
            "0005:046D:B034.0003",
            &[
                ("capacity", "35"),
                ("status", "Discharging"),
                ("scope", "Device"),
            ],
        );
        add(&tmp.0, "BAT0", "PNP0C0A:00", &[("capacity", "54")]);

        let found = power_supplies_for_in(&tmp.0, "0005:046D:B034.0003").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "hidpp_battery_0");
        assert_eq!(found[0].capacity, Some(35));
        assert_eq!(found[0].status.as_deref(), Some("Discharging"));
        assert_eq!(found[0].capacity_level, None);

        assert!(
            power_supplies_for_in(&tmp.0, "0005:046D:FFFF.0009")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn missing_class_is_empty() {
        let tmp = TempDir::new();
        assert!(power_supplies_for_in(&tmp.0, "x").unwrap().is_empty());
    }
}
