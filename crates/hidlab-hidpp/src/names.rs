//! Reference names for feature IDs and feature-type flag bits.
//!
//! These names are NOT verified against any device. They come from public
//! references (Linux `hid-logitech-hidpp.c`, Solaar; GPL-2.0) and are shown
//! only as a reading aid. Unknown IDs return `None`; never guess.

use crate::feature::FeatureId;

const REFERENCE_NAMES: &[(u16, &str)] = &[
    (0x0000, "Root"),
    (0x0001, "FeatureSet"),
    (0x0003, "DeviceFwVersion"),
    (0x0005, "DeviceName"),
    (0x1004, "UnifiedBattery"),
    (0x1814, "ChangeHost"),
    (0x1B04, "ReprogControlsV4"),
    (0x2110, "SmartShift"),
    (0x2111, "SmartShiftEnhanced"),
    (0x2121, "HiResWheel"),
    (0x2150, "Thumbwheel"),
    (0x2201, "AdjustableDpi"),
];

/// Reference (unverified) name of a feature ID, if listed.
pub fn reference_name(id: FeatureId) -> Option<&'static str> {
    REFERENCE_NAMES
        .iter()
        .find(|(fid, _)| *fid == id.0)
        .map(|(_, name)| *name)
}

/// Feature-type flag bits (LIKELY, docs/protocol/README.md P-013).
const FLAG_NAMES: &[(u8, &str)] = &[(0x80, "obsolete"), (0x40, "hidden"), (0x20, "engineering")];

/// Labels for the set bits of a feature-type byte. Unlisted bits are shown
/// as `bit0xNN`.
pub fn flag_labels(flags: u8) -> Vec<String> {
    (0..8)
        .rev()
        .map(|b| 1u8 << b)
        .filter(|bit| flags & bit != 0)
        .map(|bit| {
            FLAG_NAMES
                .iter()
                .find(|(b, _)| *b == bit)
                .map_or_else(|| format!("bit0x{bit:02x}"), |(_, n)| (*n).to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_names() {
        assert_eq!(reference_name(FeatureId(0x1B04)), Some("ReprogControlsV4"));
        assert_eq!(reference_name(FeatureId(0xABCD)), None);
    }

    #[test]
    fn flags() {
        assert!(flag_labels(0).is_empty());
        assert_eq!(flag_labels(0x60), ["hidden", "engineering"]);
        assert_eq!(flag_labels(0x81), ["obsolete", "bit0x01"]);
    }
}
