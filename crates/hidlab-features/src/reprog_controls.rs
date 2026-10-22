//! 0x1B04 ReprogControlsV4 (read-only).
//!
//! Sources: kernel `hid-logitech-hidpp.c` (GET_COUNT 0x00, GET_CID_INFO 0x10;
//! info bytes 0-1 = CID, byte 4 flags with bit0 mouse and bit5 divertable)
//! and Solaar `hidpp20.py` (`KeysArrayV4._query_key`: CID, TID, flags1,
//! pos, group, gmask, flags2; `_getCidReporting` 0x20: CID, mapping flags 1,
//! mapped-to CID, mapping flags 2; flag names from `KeyFlag` and
//! `MappingFlag`). Status: LIKELY. setCidReporting (0x30) is never called.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::Serialize;

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
    /// Task ID, as `0x....`.
    pub tid: String,
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
        tid: format!("0x{tid:04X}"),
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
        assert_eq!(c.flags, 0x0131);
        assert_eq!(
            c.flag_names,
            ["mouse", "reprogrammable", "divertable", "raw_xy"]
        );
        assert_eq!(c.unknown_flag_bits, None);
        assert!(decode_info(0, &[0; 8]).is_err());
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
