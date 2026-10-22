//! 0x2110 SmartShift (read-only).
//!
//! Source: Solaar `settings_templates.py` `ScrollRatchet` and `SmartShift`
//! (read function 0x00; mode 1 = free-spin, 2 = ratchet; byte 1 = threshold).
//! The kernel does not use this feature. Status: LIKELY. Byte 2 is UNKNOWN.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader};
use crate::util::{need, tail_hex};

pub const FEATURE: FeatureId = FeatureId(0x2110);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SmartShift {
    pub mode_code: u8,
    /// `free_spin`, `ratchet`, or `None` if the code is not documented.
    pub mode: Option<&'static str>,
    /// Auto-disengage threshold.
    pub threshold: u8,
    /// Byte 2, meaning UNKNOWN.
    pub byte2: u8,
    pub raw_hex: String,
}

pub fn decode(p: &[u8]) -> Result<SmartShift, ReadError> {
    need(p, 3, "getRatchetControlMode")?;
    Ok(SmartShift {
        mode_code: p[0],
        mode: match p[0] {
            1 => Some("free_spin"),
            2 => Some("ratchet"),
            _ => None,
        },
        threshold: p[1],
        byte2: p[2],
        raw_hex: tail_hex(p, 0),
    })
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<SmartShift, ReadError> {
    decode(&r.call(FEATURE, 0, &[])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_modes() {
        // SYNTHETIC
        let s = decode(&[2, 10, 7, 0]).unwrap();
        assert_eq!(s.mode, Some("ratchet"));
        assert_eq!((s.threshold, s.byte2), (10, 7));
        assert_eq!(decode(&[1, 0, 0]).unwrap().mode, Some("free_spin"));
        assert_eq!(decode(&[9, 0, 0]).unwrap().mode, None);
        assert!(decode(&[1]).is_err());
    }
}
