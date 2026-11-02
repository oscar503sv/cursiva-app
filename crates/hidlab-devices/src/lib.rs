//! Per-device knowledge (requirements §5): which mice hid-lab supports and
//! which settings each one accepts. `hidlab-hidpp` and `hidlab-features`
//! stay generic; anything specific to one model lives here.

use hidlab_features::reprog_controls::RemapControl;
use hidlab_features::settings::{SettingKind, SettingValue};
use hidlab_transport::Bus;

/// How a supported device shows up on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceIds {
    pub bus: Bus,
    pub vendor_id: u16,
    pub product_id: u16,
}

/// What hid-lab knows about one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceProfile {
    /// Key in the config file: `[device."<key>"]`.
    pub key: &'static str,
    pub name: &'static str,
    pub ids: &'static [DeviceIds],
    /// Settings the daemon may apply, each with a confirmed write function.
    pub settings: &'static [SettingKind],
    /// What `hid-lab reset` writes, one per setting in `settings`.
    pub defaults: &'static [SettingValue],
}

impl DeviceProfile {
    pub fn matches(&self, bus: Bus, vendor_id: u16, product_id: u16) -> bool {
        self.ids
            .iter()
            .any(|i| i.bus == bus && i.vendor_id == vendor_id && i.product_id == product_id)
    }

    pub fn supports(&self, kind: SettingKind) -> bool {
        self.settings.contains(&kind)
    }

    /// The value `hid-lab reset` writes for `kind`.
    pub fn default_value(&self, kind: SettingKind) -> Option<SettingValue> {
        self.defaults.iter().copied().find(|v| v.kind() == kind)
    }
}

/// Logitech MX Master 3S. Bluetooth ID from docs/diagnosis.md (CONFIRMED);
/// settings confirmed by the hardware tests H4 and H5 (docs/protocol P-030
/// to P-032, P-035, P-036).
///
/// Defaults: DPI 1000, threshold 10 and thumb wheel not inverted are what
/// the mouse returns to after power-off (P-033, CONFIRMED). Ratchet mode is
/// hid-lab's choice (power-off keeps the mode, so the mouse has no reset
/// value to copy). Every button does its own task.
pub const MX_MASTER_3S: DeviceProfile = DeviceProfile {
    key: "mx-master-3s",
    name: "MX Master 3S",
    ids: &[DeviceIds {
        bus: Bus::Bluetooth,
        vendor_id: 0x046D,
        product_id: 0xB034,
    }],
    settings: &[
        SettingKind::Dpi,
        SettingKind::SmartshiftMode,
        SettingKind::SmartshiftThreshold,
        SettingKind::ThumbwheelInvert,
        SettingKind::ButtonRemap(RemapControl::Middle),
        SettingKind::ButtonRemap(RemapControl::Back),
        SettingKind::ButtonRemap(RemapControl::Forward),
        SettingKind::ButtonRemap(RemapControl::Gesture),
        SettingKind::ButtonRemap(RemapControl::ModeButton),
    ],
    defaults: &[
        SettingValue::Dpi(1000),
        SettingValue::SmartshiftMode(2),
        SettingValue::SmartshiftThreshold(10),
        SettingValue::ThumbwheelInvert(false),
        SettingValue::ButtonRemap(RemapControl::Middle, 0x0052),
        SettingValue::ButtonRemap(RemapControl::Back, 0x0053),
        SettingValue::ButtonRemap(RemapControl::Forward, 0x0056),
        SettingValue::ButtonRemap(RemapControl::Gesture, 0x00C3),
        SettingValue::ButtonRemap(RemapControl::ModeButton, 0x00C4),
    ],
};

pub const PROFILES: &[DeviceProfile] = &[MX_MASTER_3S];

/// The profile for a device seen on `bus` with these IDs.
pub fn profile_for(bus: Bus, vendor_id: u16, product_id: u16) -> Option<&'static DeviceProfile> {
    PROFILES
        .iter()
        .find(|p| p.matches(bus, vendor_id, product_id))
}

/// The profile with config key `key`.
pub fn profile_by_key(key: &str) -> Option<&'static DeviceProfile> {
    PROFILES.iter().find(|p| p.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidlab_features::confirmed_write;

    #[test]
    fn finds_the_mx_master_3s_over_bluetooth_only() {
        let p = profile_for(Bus::Bluetooth, 0x046D, 0xB034).unwrap();
        assert_eq!(p.key, "mx-master-3s");
        // No other transport has been tested yet (Bolt is Milestone B).
        assert!(profile_for(Bus::Usb, 0x046D, 0xB034).is_none());
        assert!(profile_for(Bus::Bluetooth, 0x046D, 0xB035).is_none());
        assert_eq!(profile_by_key("mx-master-3s"), Some(p));
    }

    #[test]
    fn every_profile_setting_has_a_confirmed_write() {
        for p in PROFILES {
            for &k in p.settings {
                assert!(
                    confirmed_write(k.feature(), k.write_function()).is_some(),
                    "{} {} has no confirmed write",
                    p.key,
                    k.name()
                );
            }
        }
    }

    #[test]
    fn every_setting_has_one_default() {
        for p in PROFILES {
            assert_eq!(p.defaults.len(), p.settings.len(), "{}", p.key);
            for &k in p.settings {
                assert!(p.default_value(k).is_some(), "{} {}", p.key, k.name());
            }
            for c in RemapControl::ALL {
                let k = SettingKind::ButtonRemap(c);
                if p.supports(k) {
                    assert_eq!(
                        p.default_value(k),
                        Some(SettingValue::ButtonRemap(c, c.cid()))
                    );
                }
            }
        }
    }

    #[test]
    fn keys_are_unique() {
        for (i, a) in PROFILES.iter().enumerate() {
            assert!(PROFILES[i + 1..].iter().all(|b| b.key != a.key));
        }
    }
}
