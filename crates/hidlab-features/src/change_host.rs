//! 0x1814 ChangeHost (read-only).
//!
//! Source: Solaar `settings_templates.py` `ChangeHost` (read function 0x00:
//! byte 0 = number of hosts, byte 1 = current host, 0-based; shown to users
//! as host + 1). The kernel does not use this feature. Status: LIKELY.
//! setCurrentHost (0x10) switches the mouse away and is never called.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader};
use crate::util::{need, tail_hex};

pub const FEATURE: FeatureId = FeatureId(0x1814);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeHost {
    pub host_count: u8,
    /// 0-based, as on the wire.
    pub current_host: u8,
    /// 1-based, as printed on the mouse.
    pub current_channel: u16,
    /// Bytes 2.., trailing zeros removed, meaning UNKNOWN.
    pub rest_hex: String,
    pub raw_hex: String,
}

pub fn decode(p: &[u8]) -> Result<ChangeHost, ReadError> {
    need(p, 2, "getHostInfo")?;
    Ok(ChangeHost {
        host_count: p[0],
        current_host: p[1],
        current_channel: u16::from(p[1]) + 1,
        rest_hex: tail_hex(p, 2),
        raw_hex: tail_hex(p, 0),
    })
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<ChangeHost, ReadError> {
    decode(&r.call(FEATURE, 0, &[])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_host_info() {
        // SYNTHETIC
        let h = decode(&[3, 1, 0x05, 0, 0]).unwrap();
        assert_eq!((h.host_count, h.current_host, h.current_channel), (3, 1, 2));
        assert_eq!(h.rest_hex, "05");
        assert!(decode(&[3]).is_err());
    }
}
