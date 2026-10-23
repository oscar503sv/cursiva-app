//! 0x2121 HiResWheel (read-only).
//!
//! Sources: kernel `hid-logitech-hidpp.c` (GET_WHEEL_CAPABILITY 0x00 byte 0 =
//! multiplier; SET_WHEEL_MODE 0x20 with bit0 HID++ target, bit1 high
//! resolution, bit2 invert) and Solaar `hidpp20.py` `get_hires_wheel`
//! (reads 0x00, 0x10, 0x30). Status: LIKELY.
//!
//! The kernel itself writes the wheel mode (high resolution on) when the
//! device connects. hid-lab only reads it.

use hidlab_hidpp::{FeatureId, Frame};
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader};
use crate::util::{need, soft, tail_hex, unknown_bits};

pub const FEATURE: FeatureId = FeatureId(0x2121);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capability {
    pub multiplier: u8,
    pub has_invert: bool,
    pub has_ratchet_switch: bool,
    pub unknown_flag_bits: Option<String>,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mode {
    pub raw: u8,
    /// Wheel events go to HID++ instead of normal HID input.
    pub diverted: bool,
    pub high_resolution: bool,
    pub inverted: bool,
    pub unknown_bits: Option<String>,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RatchetSwitch {
    pub raw: u8,
    /// Bit 0 set, read by Solaar as "ratchet".
    pub ratchet: bool,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HiResWheel {
    pub capability: Capability,
    pub mode: Mode,
    pub ratchet_switch: Option<RatchetSwitch>,
    pub notes: Vec<String>,
}

pub fn decode_capability(p: &[u8]) -> Result<Capability, ReadError> {
    need(p, 2, "getWheelCapability")?;
    Ok(Capability {
        multiplier: p[0],
        has_invert: p[1] & 0x08 != 0,
        has_ratchet_switch: p[1] & 0x04 != 0,
        unknown_flag_bits: unknown_bits(p[1].into(), 0x0C),
        raw_hex: tail_hex(p, 0),
    })
}

pub fn decode_mode(p: &[u8]) -> Result<Mode, ReadError> {
    need(p, 1, "getWheelMode")?;
    Ok(Mode {
        raw: p[0],
        diverted: p[0] & 0x01 != 0,
        high_resolution: p[0] & 0x02 != 0,
        inverted: p[0] & 0x04 != 0,
        unknown_bits: unknown_bits(p[0].into(), 0x07),
        raw_hex: tail_hex(p, 0),
    })
}

pub fn decode_ratchet(p: &[u8]) -> Result<RatchetSwitch, ReadError> {
    need(p, 1, "getRatchetSwitchState")?;
    Ok(RatchetSwitch {
        raw: p[0],
        ratchet: p[0] & 0x01 != 0,
        raw_hex: tail_hex(p, 0),
    })
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<HiResWheel, ReadError> {
    let capability = decode_capability(&r.call(FEATURE, 0, &[])?)?;
    let mode = decode_mode(&r.call(FEATURE, 1, &[])?)?;
    let mut notes = Vec::new();
    let ratchet_switch = soft(
        r.call(FEATURE, 3, &[]).and_then(|p| decode_ratchet(&p)),
        &mut notes,
    )?;
    Ok(HiResWheel {
        capability,
        mode,
        ratchet_switch,
        notes,
    })
}

/// Function of the ratchet event, and the software ID of notifications.
const RATCHET_EVENT: u8 = 1;

/// If `frame` is the 0x2121 ratchet event (function 1, software ID 0) from
/// the feature at `feature_index`, returns its ratchet bit: `true` for
/// ratchet, `false` for free spin. CONFIRMED: P-027, P-029. It fires on the
/// mode button and on a 0x2110 mode write, not on automatic switching.
pub fn ratchet_event(frame: &Frame, feature_index: u8) -> Option<bool> {
    match frame {
        Frame::Message {
            feature_index: fi,
            function: RATCHET_EVENT,
            sw_id: 0,
            params,
            ..
        } if *fi == feature_index => Some(params.first().is_some_and(|b| b & 0x01 != 0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_only_the_ratchet_event() {
        // SYNTHETIC frames; the layout matches `evidence/m4a/03-watch-mode.json`.
        let mut r = [0u8; 20];
        r[..5].copy_from_slice(&[0x11, 0xFF, 0x0F, 0x10, 0x01]);
        let f = hidlab_hidpp::decode(&r).unwrap();
        assert_eq!(ratchet_event(&f, 0x0F), Some(true));
        assert_eq!(ratchet_event(&f, 0x0E), None);
        r[4] = 0x00;
        assert_eq!(
            ratchet_event(&hidlab_hidpp::decode(&r).unwrap(), 0x0F),
            Some(false)
        );
        // A response to our own request (sw_id 0x0C) is not an event.
        r[3] = 0x1C;
        assert_eq!(
            ratchet_event(&hidlab_hidpp::decode(&r).unwrap(), 0x0F),
            None
        );
        // Mouse movement is not HID++.
        let mv = hidlab_hidpp::decode(&[0x02, 0, 1, 0, 0xFF, 0xFF, 0, 0]).unwrap();
        assert_eq!(ratchet_event(&mv, 0x0F), None);
    }

    #[test]
    fn decodes_all() {
        // SYNTHETIC
        let c = decode_capability(&[8, 0x0C]).unwrap();
        assert!(c.has_invert && c.has_ratchet_switch);
        assert_eq!(c.multiplier, 8);
        let m = decode_mode(&[0x02]).unwrap();
        assert!(m.high_resolution && !m.diverted && !m.inverted);
        assert_eq!(
            decode_mode(&[0x0A]).unwrap().unknown_bits.as_deref(),
            Some("0x08")
        );
        assert!(decode_ratchet(&[0x01]).unwrap().ratchet);
        assert!(!decode_ratchet(&[0x00]).unwrap().ratchet);
    }
}
