//! Button remapping names: the controls that can be remapped and the
//! targets they can be given (docs/protocol P-024, P-036).

use serde::{Deserialize, Serialize};

/// A control that can be remapped, by its HID++ 0x1B04 control ID (Solaar
/// `special_keys.CONTROL`; the physical buttons are confirmed in
/// docs/protocol P-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemapControl {
    Middle,
    Back,
    Forward,
    Gesture,
    #[serde(rename = "mode")]
    ModeButton,
}

impl RemapControl {
    pub const ALL: [RemapControl; 5] = [
        RemapControl::Middle,
        RemapControl::Back,
        RemapControl::Forward,
        RemapControl::Gesture,
        RemapControl::ModeButton,
    ];

    pub fn cid(self) -> u16 {
        match self {
            RemapControl::Middle => 0x0052,
            RemapControl::Back => 0x0053,
            RemapControl::Forward => 0x0056,
            RemapControl::Gesture => 0x00C3,
            RemapControl::ModeButton => 0x00C4,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RemapControl::Middle => "middle",
            RemapControl::Back => "back",
            RemapControl::Forward => "forward",
            RemapControl::Gesture => "gesture",
            RemapControl::ModeButton => "mode",
        }
    }
}

/// Names accepted as remap targets, by control ID (same source as
/// [`RemapControl`]). Whether a target is allowed for a control is decided
/// by the device's groups, not by this list.
pub const TARGET_NAMES: &[(&str, u16)] = &[
    ("left", 0x0050),
    ("right", 0x0051),
    ("middle", 0x0052),
    ("back", 0x0053),
    ("forward", 0x0056),
    ("gesture", 0x00C3),
    ("mode", 0x00C4),
];

pub fn target_cid(name: &str) -> Option<u16> {
    TARGET_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
}

pub fn target_name(cid: u16) -> Option<&'static str> {
    TARGET_NAMES
        .iter()
        .find(|(_, c)| *c == cid)
        .map(|(n, _)| *n)
}
