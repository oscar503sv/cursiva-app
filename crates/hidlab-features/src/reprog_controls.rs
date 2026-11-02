//! 0x1B04 ReprogControlsV4 (read-only).
//!
//! Sources: kernel `hid-logitech-hidpp.c` (GET_COUNT 0x00, GET_CID_INFO 0x10;
//! info bytes 0-1 = CID, byte 4 flags with bit0 mouse and bit5 divertable)
//! and Solaar `hidpp20.py` (`KeysArrayV4._query_key`: CID, TID, flags1,
//! pos, group, gmask, flags2; `_getCidReporting` 0x20: CID, mapping flags 1,
//! mapped-to CID, mapping flags 2; flag names from `KeyFlag` and
//! `MappingFlag`). Status: LIKELY.
//!
//! setCidReporting (function 3) is sent only by `hid-lab lab write` to remap
//! a control (see [`RemapControl`]), never to divert one. Source: Solaar
//! `ReprogrammableKeyV4._setCidReporting` and `remappable_to`: the
//! parameters are the CID (2 bytes, big-endian), a flags byte (0 changes no
//! reporting flag) and the target CID (2 bytes; 0 keeps the mapping). A
//! control may be remapped to itself or to any control whose group is in its
//! group mask (bit `n` of the mask = group `n + 1`). The kernel sends the
//! same function, with the CID and flags layout, only to divert buttons of
//! the Signature M650.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::{Deserialize, Serialize};

use crate::reader::{ReadError, Reader};
use crate::util::{bit_names, need, soft, tail_hex, unknown_bits};

pub const FEATURE: FeatureId = FeatureId(0x1B04);

/// Capability flags, bytes 4 and 8 of getCidInfo (Solaar `KeyFlag`).
const KEY_FLAGS: &[(u16, &str)] = &[
    (0x0001, "mouse"),
    (0x0002, "is_fn"),
    (0x0004, "nonstandard"),
    (0x0008, "fn_sensitive"),
    (0x0010, "reprogrammable"),
    (0x0020, "divertable"),
    (0x0040, "persistently_divertable"),
    (0x0080, "virtual"),
    (0x0100, "raw_xy"),
    (0x0200, "force_raw_xy"),
    (0x0400, "analytics_key_events"),
    (0x0800, "raw_wheel"),
];

/// Reporting flags, bytes 2 and 5 of getCidReporting (Solaar `MappingFlag`).
const MAPPING_FLAGS: &[(u16, &str)] = &[
    (0x0001, "diverted"),
    (0x0004, "persistently_diverted"),
    (0x0010, "raw_xy_diverted"),
    (0x0040, "force_raw_xy_diverted"),
    (0x0100, "analytics_key_events_reporting"),
    (0x0400, "raw_wheel"),
];

/// Reference names for control IDs, from Solaar `special_keys.py` `CONTROL`.
/// NOT verified on any device. Only IDs reported by a device under test
/// have been added; unknown IDs return `None`.
const CID_NAMES: &[(u16, &str)] = &[
    (0x0050, "Left_Button"),
    (0x0051, "Right_Button"),
    (0x0052, "Middle_Button"),
    (0x0053, "Back_Button"),
    (0x0056, "Forward_Button"),
    (0x00C3, "Mouse_Gesture_Button"),
    (0x00C4, "Smart_Shift"),
    (0x00D7, "Virtual_Gesture_Button"),
];

/// Reference names for task IDs, from Solaar `special_keys.py` `Task`.
/// NOT verified on any device.
const TID_NAMES: &[(u16, &str)] = &[
    (0x0038, "LEFT_CLICK"),
    (0x0039, "RIGHT_CLICK"),
    (0x003A, "MOUSE_MIDDLE_BUTTON"),
    (0x003C, "MOUSE_BACK_BUTTON"),
    (0x003E, "MOUSE_FORWARD_BUTTON"),
    (0x009D, "SMART_SHIFT"),
    (0x00A9, "GESTURE_BUTTON_NAVIGATION"),
    (0x00B4, "VIRTUAL_GESTURE_BUTTON"),
];

/// A control `hid-lab lab write` may remap, by its control ID (Solaar
/// `special_keys.CONTROL`; which physical button each one is, is LIKELY,
/// P-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

fn lookup(table: &[(u16, &'static str)], id: u16) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == id).map(|(_, n)| *n)
}

/// Unverified reference name of a control ID.
pub fn cid_reference_name(cid: u16) -> Option<&'static str> {
    lookup(CID_NAMES, cid)
}

/// Unverified reference name of a task ID.
pub fn tid_reference_name(tid: u16) -> Option<&'static str> {
    lookup(TID_NAMES, tid)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reporting {
    pub flags: u16,
    pub flag_names: Vec<&'static str>,
    pub unknown_flag_bits: Option<String>,
    /// Mapped-to CID; 0 on the wire means "itself" and is shown as the CID.
    pub mapped_to: u16,
    pub remapped: bool,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Control {
    pub index: u8,
    /// Control ID, as `0x....`.
    pub cid: String,
    /// Unverified reference name (Solaar), if listed.
    pub cid_reference_name: Option<&'static str>,
    /// Task ID, as `0x....`.
    pub tid: String,
    /// Unverified reference name (Solaar), if listed.
    pub tid_reference_name: Option<&'static str>,
    pub flags: u16,
    pub flag_names: Vec<&'static str>,
    pub unknown_flag_bits: Option<String>,
    pub pos: u8,
    pub group: u8,
    pub group_mask: u8,
    pub info_raw_hex: String,
    pub reporting: Option<Reporting>,
    #[serde(skip)]
    pub cid_value: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReprogControls {
    pub count: u8,
    pub controls: Vec<Control>,
    pub notes: Vec<String>,
}

pub fn decode_info(index: u8, p: &[u8]) -> Result<Control, ReadError> {
    need(p, 9, "getCidInfo")?;
    let cid = u16::from_be_bytes([p[0], p[1]]);
    let tid = u16::from_be_bytes([p[2], p[3]]);
    let flags = u16::from(p[4]) | (u16::from(p[8]) << 8);
    Ok(Control {
        index,
        cid: format!("0x{cid:04X}"),
        cid_reference_name: cid_reference_name(cid),
        tid: format!("0x{tid:04X}"),
        tid_reference_name: tid_reference_name(tid),
        flags,
        flag_names: bit_names(flags, KEY_FLAGS),
        unknown_flag_bits: unknown_bits(flags, 0x0FFF),
        pos: p[5],
        group: p[6],
        group_mask: p[7],
        info_raw_hex: tail_hex(p, 0),
        reporting: None,
        cid_value: cid,
    })
}

/// Decodes getCidReporting. Returns the echoed CID alongside.
pub fn decode_reporting(p: &[u8]) -> Result<(u16, Reporting), ReadError> {
    need(p, 6, "getCidReporting")?;
    let echoed = u16::from_be_bytes([p[0], p[1]]);
    let flags = u16::from(p[2]) | (u16::from(p[5]) << 8);
    let mapped = u16::from_be_bytes([p[3], p[4]]);
    let known: u16 = MAPPING_FLAGS.iter().map(|(b, _)| b).fold(0, |a, b| a | b);
    Ok((
        echoed,
        Reporting {
            flags,
            flag_names: bit_names(flags, MAPPING_FLAGS),
            unknown_flag_bits: unknown_bits(flags, known),
            mapped_to: if mapped == 0 { echoed } else { mapped },
            remapped: mapped != 0 && mapped != echoed,
            raw_hex: tail_hex(p, 0),
        },
    ))
}

/// What a remap of one control needs: its groups, the targets the device
/// allows, and its current reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemapState {
    pub control: u16,
    pub group_mask: u8,
    /// Control IDs it may be mapped to, itself first.
    pub targets: Vec<u16>,
    pub reporting: Reporting,
}

/// Targets allowed for `control` among `all` (Solaar `remappable_to`): none
/// when its group mask is 0, otherwise itself and every control whose group
/// is in the mask.
pub fn remap_targets(all: &[Control], control: &Control) -> Vec<u16> {
    if control.group_mask == 0 {
        return Vec::new();
    }
    let mut out = vec![control.cid_value];
    for c in all {
        let in_mask = (1..=8).contains(&c.group) && control.group_mask & (1 << (c.group - 1)) != 0;
        if in_mask && c.cid_value != control.cid_value {
            out.push(c.cid_value);
        }
    }
    out
}

/// Reads every control's info and the reporting of `cid` (reads only).
/// The control list is a device capability, so getCount and getCidInfo are
/// memoized: remapping several controls reads it once per connection.
pub fn read_remap<C: RawChannel>(r: &mut Reader<'_, C>, cid: u16) -> Result<RemapState, ReadError> {
    let p = r.call_static(FEATURE, 0, &[])?;
    need(&p, 1, "getCount")?;
    let mut controls = Vec::new();
    for index in 0..p[0] {
        controls.push(decode_info(index, &r.call_static(FEATURE, 1, &[index])?)?);
    }
    let control = controls
        .iter()
        .find(|c| c.cid_value == cid)
        .ok_or_else(|| ReadError::Decode {
            what: "getCidInfo",
            reason: format!("the device has no control 0x{cid:04X}"),
        })?;
    let (echoed, reporting) = decode_reporting(&r.call(FEATURE, 2, &cid.to_be_bytes())?)?;
    if echoed != cid {
        return Err(ReadError::Decode {
            what: "getCidReporting",
            reason: format!("asked for 0x{cid:04X}, answered with CID 0x{echoed:04X}"),
        });
    }
    Ok(RemapState {
        control: cid,
        group_mask: control.group_mask,
        targets: remap_targets(&controls, control),
        reporting,
    })
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<ReprogControls, ReadError> {
    let p = r.call(FEATURE, 0, &[])?;
    need(&p, 1, "getCount")?;
    let count = p[0];
    let mut notes = Vec::new();
    let mut controls = Vec::new();

    for index in 0..count {
        let info = r
            .call(FEATURE, 1, &[index])
            .and_then(|p| decode_info(index, &p));
        let Some(mut c) = soft(info, &mut notes)? else {
            continue;
        };
        let rep = r
            .call(FEATURE, 2, &c.cid_value.to_be_bytes())
            .and_then(|p| decode_reporting(&p));
        if let Some((echoed, rep)) = soft(rep, &mut notes)? {
            if echoed != c.cid_value {
                notes.push(format!(
                    "getCidReporting for {} answered with CID 0x{echoed:04X}",
                    c.cid
                ));
            }
            c.reporting = Some(rep);
        }
        controls.push(c);
    }
    Ok(ReprogControls {
        count,
        controls,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_info() {
        // SYNTHETIC: CID 0x0053, TID 0x0038, flags1 0x31, pos 0, group 1, gmask 1, flags2 0x01
        let c = decode_info(2, &[0x00, 0x53, 0x00, 0x38, 0x31, 0, 1, 1, 0x01, 0]).unwrap();
        assert_eq!(c.cid, "0x0053");
        assert_eq!(c.tid, "0x0038");
        assert_eq!(c.cid_reference_name, Some("Back_Button"));
        assert_eq!(c.tid_reference_name, Some("LEFT_CLICK"));
        assert_eq!(cid_reference_name(0x1234), None);
        assert_eq!(c.flags, 0x0131);
        assert_eq!(
            c.flag_names,
            ["mouse", "reprogrammable", "divertable", "raw_xy"]
        );
        assert_eq!(c.unknown_flag_bits, None);
        assert!(decode_info(0, &[0; 8]).is_err());
    }

    #[test]
    fn remap_targets_follow_groups() {
        // SYNTHETIC controls with the group layout of P-024: left/right in
        // group 1 (mask 1), five buttons in group 2 (mask 3), virtual in 3.
        let info = |i: u8, cid: u8, group: u8, gmask: u8| {
            decode_info(i, &[0, cid, 0, 0, 0x31, 0, group, gmask, 0]).unwrap()
        };
        let all = [
            info(0, 0x50, 1, 1),
            info(1, 0x51, 1, 1),
            info(2, 0x52, 2, 3),
            info(3, 0x53, 2, 3),
            info(4, 0xD7, 3, 0),
        ];
        assert_eq!(remap_targets(&all, &all[3]), [0x53, 0x50, 0x51, 0x52]);
        assert_eq!(remap_targets(&all, &all[0]), [0x50, 0x51]);
        assert!(remap_targets(&all, &all[4]).is_empty());
        assert_eq!(target_cid("back"), Some(RemapControl::Back.cid()));
        assert_eq!(target_name(0x00C4), Some("mode"));
    }

    #[test]
    fn decodes_reporting() {
        let (cid, r) = decode_reporting(&[0x00, 0x53, 0x01, 0x00, 0x00, 0x00]).unwrap();
        assert_eq!(cid, 0x53);
        assert_eq!(r.flag_names, ["diverted"]);
        assert_eq!(r.mapped_to, 0x53);
        assert!(!r.remapped);
        let (_, r) = decode_reporting(&[0x00, 0x53, 0x00, 0x00, 0x56, 0x02]).unwrap();
        assert!(r.remapped);
        assert_eq!(r.unknown_flag_bits.as_deref(), Some("0x200"));
    }
}
