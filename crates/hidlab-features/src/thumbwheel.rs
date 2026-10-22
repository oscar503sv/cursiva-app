//! 0x2150 Thumbwheel (read-only).
//!
//! Source: Solaar `settings_templates.py` `ThumbMode` and `ThumbInvert`
//! (read function 0x10; byte 0 bit 0 = diverted, byte 1 bit 0 = inverted).
//! The kernel does not use this feature. Status: LIKELY. Function 0 is not
//! called: no source documents it.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader};
use crate::util::{need, tail_hex};

/// Bytes 0 and 1, plus any non-zero bytes after them.
fn raw(p: &[u8]) -> String {
    let rest = tail_hex(p, 2);
    let head = hidlab_hidpp::hex(&p[..2]);
    if rest.is_empty() {
        head
    } else {
        format!("{head} {rest}")
    }
}

pub const FEATURE: FeatureId = FeatureId(0x2150);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Thumbwheel {
    pub diverted: bool,
    pub inverted: bool,
    /// Bytes 0 and 1 with the documented bit 0 removed, if non-zero.
    pub unknown_bits: Option<String>,
    pub raw_hex: String,
}

pub fn decode(p: &[u8]) -> Result<Thumbwheel, ReadError> {
    need(p, 2, "getThumbwheelStatus")?;
    let rest = [p[0] & !0x01, p[1] & !0x01];
    Ok(Thumbwheel {
        diverted: p[0] & 0x01 != 0,
        inverted: p[1] & 0x01 != 0,
        unknown_bits: (rest != [0, 0]).then(|| hidlab_hidpp::hex(&rest)),
        raw_hex: raw(p),
    })
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<Thumbwheel, ReadError> {
    decode(&r.call(FEATURE, 1, &[])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bits() {
        // SYNTHETIC
        let t = decode(&[0x00, 0x01, 0x00]).unwrap();
        assert!(!t.diverted && t.inverted);
        assert_eq!(t.unknown_bits, None);
        assert_eq!(t.raw_hex, "00 01");
        let t = decode(&[0x03, 0x00]).unwrap();
        assert!(t.diverted);
        assert_eq!(t.unknown_bits.as_deref(), Some("02 00"));
        assert!(decode(&[0]).is_err());
    }
}
