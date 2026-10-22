//! 0x1004 UnifiedBattery (read-only).
//!
//! Sources: kernel `hid-logitech-hidpp.c` (GET_CAPABILITIES 0x00,
//! GET_STATUS 0x10, level and flag bits, charging-status mapping) and
//! Solaar `hidpp20.py` `decipher_battery_unified`. Status: LIKELY.
//!
//! The two references disagree on charging-status codes 2 and 4; both
//! readings are reported and the value is marked as a conflict.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader};
use crate::util::{bit_names, need, soft, tail_hex, unknown_bits};

pub const FEATURE: FeatureId = FeatureId(0x1004);

const LEVELS: &[(u16, &str)] = &[
    (0x01, "critical"),
    (0x02, "low"),
    (0x04, "good"),
    (0x08, "full"),
];
const CAP_FLAGS: &[(u16, &str)] = &[(0x01, "rechargeable"), (0x02, "state_of_charge")];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub supported_levels: Vec<&'static str>,
    pub flags: Vec<&'static str>,
    pub unknown_flag_bits: Option<String>,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChargingStatus {
    pub code: u8,
    /// Meaning per the kernel driver.
    pub kernel: Option<&'static str>,
    /// Meaning per Solaar.
    pub solaar: Option<&'static str>,
    /// `agree`, `conflict` or `unknown`.
    pub references: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// State of charge, percent.
    pub state_of_charge: u8,
    pub levels: Vec<&'static str>,
    pub charging: ChargingStatus,
    /// Byte 3. The kernel passes it along but does not interpret it.
    pub external_power: u8,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Battery {
    pub capabilities: Option<Capabilities>,
    pub status: Status,
    pub notes: Vec<String>,
}

pub fn charging_status(code: u8) -> ChargingStatus {
    let kernel = match code {
        0 => Some("discharging"),
        1 => Some("charging"),
        2 => Some("charging_slow"),
        3 => Some("complete"),
        4 => Some("error"),
        _ => None,
    };
    let solaar = match code {
        0 => Some("discharging"),
        1 => Some("recharging"),
        2 => Some("almost_full"),
        3 => Some("full"),
        4 => Some("slow_recharge"),
        5 => Some("invalid_battery"),
        6 => Some("thermal_error"),
        _ => None,
    };
    let references = match code {
        0 | 1 | 3 => "agree",
        2 | 4 => "conflict",
        _ => "unknown",
    };
    ChargingStatus {
        code,
        kernel,
        solaar,
        references,
    }
}

pub fn decode_capabilities(p: &[u8]) -> Result<Capabilities, ReadError> {
    need(p, 2, "getCapabilities")?;
    Ok(Capabilities {
        supported_levels: bit_names(p[0].into(), LEVELS),
        flags: bit_names(p[1].into(), CAP_FLAGS),
        unknown_flag_bits: unknown_bits(p[1].into(), 0x03),
        raw_hex: tail_hex(p, 0),
    })
}

pub fn decode_status(p: &[u8]) -> Result<Status, ReadError> {
    need(p, 4, "getStatus")?;
    Ok(Status {
        state_of_charge: p[0],
        levels: bit_names(p[1].into(), LEVELS),
        charging: charging_status(p[2]),
        external_power: p[3],
        raw_hex: tail_hex(p, 0),
    })
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<Battery, ReadError> {
    let mut notes = Vec::new();
    let capabilities = soft(
        r.call(FEATURE, 0, &[])
            .and_then(|p| decode_capabilities(&p)),
        &mut notes,
    )?;
    let status = decode_status(&r.call(FEATURE, 1, &[])?)?;
    Ok(Battery {
        capabilities,
        status,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_status() {
        // SYNTHETIC
        let s = decode_status(&[57, 0x04, 0x00, 0x00, 0, 0]).unwrap();
        assert_eq!(s.state_of_charge, 57);
        assert_eq!(s.levels, ["good"]);
        assert_eq!(s.charging.references, "agree");
        assert_eq!(s.charging.kernel, Some("discharging"));
        assert_eq!(s.raw_hex, "39 04");
    }

    #[test]
    fn conflicting_codes_are_flagged() {
        let c = charging_status(2);
        assert_eq!(c.references, "conflict");
        assert_eq!(c.kernel, Some("charging_slow"));
        assert_eq!(c.solaar, Some("almost_full"));
        assert_eq!(charging_status(9).references, "unknown");
    }

    #[test]
    fn decodes_capabilities() {
        let c = decode_capabilities(&[0x0F, 0x03]).unwrap();
        assert_eq!(c.supported_levels, ["critical", "low", "good", "full"]);
        assert_eq!(c.flags, ["rechargeable", "state_of_charge"]);
        assert_eq!(c.unknown_flag_bits, None);
        assert_eq!(
            decode_capabilities(&[0, 0x81])
                .unwrap()
                .unknown_flag_bits
                .as_deref(),
            Some("0x80")
        );
    }
}
