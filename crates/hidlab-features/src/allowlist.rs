//! The only HID++ functions hid-lab is allowed to call in milestone 3.
//!
//! Every entry is a read documented by at least one public source. A
//! function that no source documents is never called, because it could be
//! a write. See docs/protocol/README.md for the status of each entry.

use hidlab_hidpp::FeatureId;

/// One allowed read function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadFn {
    pub feature: FeatureId,
    /// Function number, 0..=15 (the `0x10`-style byte in sources is `function << 4`).
    pub function: u8,
    pub name: &'static str,
    /// Which reference documents it: "kernel", "solaar" or both.
    pub source: &'static str,
}

const fn r(feature: u16, function: u8, name: &'static str, source: &'static str) -> ReadFn {
    ReadFn {
        feature: FeatureId(feature),
        function,
        name,
        source,
    }
}

pub const READ_ALLOWLIST: &[ReadFn] = &[
    r(0x0000, 0, "getFeature", "kernel, solaar"),
    r(0x0000, 1, "getProtocolVersion", "kernel, solaar"),
    r(0x0001, 0, "getCount", "kernel, solaar"),
    r(0x0001, 1, "getFeatureID", "kernel, solaar"),
    r(0x0003, 0, "getDeviceInfo", "kernel, solaar"),
    r(0x0003, 1, "getFwInfo", "solaar"),
    r(0x0005, 0, "getDeviceNameCount", "kernel, solaar"),
    r(0x0005, 1, "getDeviceName", "kernel, solaar"),
    r(0x0005, 2, "getDeviceType", "kernel, solaar"),
    r(0x1004, 0, "getCapabilities", "kernel"),
    r(0x1004, 1, "getStatus", "kernel, solaar"),
    r(0x1814, 0, "getHostInfo", "solaar"),
    r(0x1B04, 0, "getCount", "kernel, solaar"),
    r(0x1B04, 1, "getCidInfo", "kernel, solaar"),
    r(0x1B04, 2, "getCidReporting", "solaar"),
    r(0x2110, 0, "getRatchetControlMode", "solaar"),
    r(0x2121, 0, "getWheelCapability", "kernel, solaar"),
    r(0x2121, 1, "getWheelMode", "solaar"),
    r(0x2121, 3, "getRatchetSwitchState", "solaar"),
    r(0x2150, 1, "getThumbwheelStatus", "solaar"),
    r(0x2201, 1, "getSensorDpiList", "solaar"),
    r(0x2201, 2, "getSensorDpi", "solaar"),
];

/// Functions that the references use as writes. Kept here so a test can
/// prove none of them is ever allowed.
pub const KNOWN_WRITES: &[(u16, u8)] = &[
    (0x1814, 1), // setCurrentHost (solaar)
    (0x1B04, 3), // setCidReporting (kernel, solaar)
    (0x2110, 1), // setRatchetControlMode (solaar)
    (0x2121, 2), // setWheelMode (kernel, solaar)
    (0x2150, 2), // setThumbwheelReporting (solaar)
    (0x2201, 3), // setSensorDpi (solaar)
];

/// Write functions that `hid-lab lab write` may send to confirm them on the
/// real device (requirements §13 exception). Each is documented by a source;
/// none is CONFIRMED until its hardware test passes.
pub const WRITE_EXPERIMENT_ALLOWLIST: &[ReadFn] = &[
    r(0x2201, 3, "setSensorDpi", "solaar"),
    r(0x2110, 1, "setRatchetControlMode", "solaar"),
    r(0x2150, 2, "setThumbwheelReporting", "solaar"),
];

/// Writes hid-lab must never send in this milestone: the kernel owns the
/// wheel mode, and changing host moves the mouse away.
pub const NEVER_WRITE: &[(u16, u8)] = &[
    (0x2121, 2), // setWheelMode
    (0x1814, 1), // setCurrentHost
];

/// Looks up an allowed experimental write.
pub fn write_allowed(feature: FeatureId, function: u8) -> Option<&'static ReadFn> {
    if NEVER_WRITE.contains(&(feature.0, function)) {
        return None;
    }
    WRITE_EXPERIMENT_ALLOWLIST
        .iter()
        .find(|f| f.feature == feature && f.function == function)
}

/// Looks up an allowed read.
pub fn allowed(feature: FeatureId, function: u8) -> Option<&'static ReadFn> {
    READ_ALLOWLIST
        .iter()
        .find(|f| f.feature == feature && f.function == function)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_known_write_is_allowed() {
        for &(feature, function) in KNOWN_WRITES {
            assert!(
                allowed(FeatureId(feature), function).is_none(),
                "write 0x{feature:04X} f{function} is in the allowlist"
            );
        }
    }

    #[test]
    fn write_list_is_exactly_the_approved_set() {
        let got: Vec<(u16, u8)> = WRITE_EXPERIMENT_ALLOWLIST
            .iter()
            .map(|w| (w.feature.0, w.function))
            .collect();
        assert_eq!(got, [(0x2201, 3), (0x2110, 1), (0x2150, 2)]);
        for &(f, func) in NEVER_WRITE {
            assert!(write_allowed(FeatureId(f), func).is_none());
            assert!(allowed(FeatureId(f), func).is_none());
        }
        // No write function is readable and no read function is writable.
        for w in WRITE_EXPERIMENT_ALLOWLIST {
            assert!(allowed(w.feature, w.function).is_none());
        }
        for rf in READ_ALLOWLIST {
            assert!(write_allowed(rf.feature, rf.function).is_none());
        }
    }

    #[test]
    fn entries_are_unique_and_in_range() {
        for (i, a) in READ_ALLOWLIST.iter().enumerate() {
            assert!(a.function <= 0x0F);
            assert!(!a.source.is_empty());
            for b in &READ_ALLOWLIST[i + 1..] {
                assert!(!(a.feature == b.feature && a.function == b.function));
            }
        }
    }
}
