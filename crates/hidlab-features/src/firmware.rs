//! 0x0003 DeviceFwVersion / Device Information (read-only).
//!
//! Sources: Solaar `hidpp20.py` `get_firmware`, `get_ids` and `common.py`
//! `FirmwareKind`; kernel `hid-logitech-hidpp.c` `hidpp_get_serial` (bytes
//! 1..5 of getDeviceInfo are the unit serial). Status: LIKELY.
//!
//! The unit ID is a serial number. Callers must redact it by default, in
//! decoded output and in the transcript ([`redact_unit_id`]).

use hidlab_hidpp::{FeatureId, Transcript};
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader};
use crate::util::{need, soft, tail_hex};

pub const FEATURE: FeatureId = FeatureId(0x0003);

/// Transport bits of getDeviceInfo byte 6, in the order Solaar consumes the
/// 2-byte model IDs that follow.
const TRANSPORTS: &[(u8, &str)] = &[(0x01, "bt"), (0x02, "btle"), (0x04, "wpid"), (0x08, "usb")];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelId {
    pub transport: &'static str,
    /// Uppercase hex, e.g. `B034`.
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceInfo {
    pub entity_count: u8,
    /// Serial number. Sensitive.
    pub unit_id: String,
    /// Byte 5, meaning UNKNOWN.
    pub byte5: u8,
    pub transport_bits: u8,
    pub model_ids: Vec<ModelId>,
    /// Bytes 7..13, as read.
    pub model_bytes_hex: String,
    /// Bytes 13.., trailing zeros removed, meaning UNKNOWN.
    pub rest_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FwEntity {
    pub entity: u8,
    /// Low nibble of byte 0.
    pub kind_code: u8,
    pub kind: &'static str,
    /// High nibble of byte 0, meaning UNKNOWN.
    pub kind_high_nibble: u8,
    pub name: Option<String>,
    /// `MM.mm` as Solaar formats it (hex digits).
    pub version: Option<String>,
    pub build: Option<u16>,
    /// Byte 8, meaning UNKNOWN.
    pub byte8: Option<u8>,
    pub extras_hex: Option<String>,
    pub hardware_version: Option<u8>,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Firmware {
    pub info: DeviceInfo,
    pub entities: Vec<FwEntity>,
    pub notes: Vec<String>,
}

pub fn decode_device_info(p: &[u8]) -> Result<DeviceInfo, ReadError> {
    need(p, 13, "getDeviceInfo")?;
    let transport_bits = p[6];
    let model = &p[7..13];
    let mut model_ids = Vec::new();
    let mut offset = 0;
    for &(bit, name) in TRANSPORTS {
        if transport_bits & bit != 0 && offset + 2 <= model.len() {
            model_ids.push(ModelId {
                transport: name,
                id: format!("{:02X}{:02X}", model[offset], model[offset + 1]),
            });
            offset += 2;
        }
    }
    Ok(DeviceInfo {
        entity_count: p[0],
        unit_id: format!("{:02X}{:02X}{:02X}{:02X}", p[1], p[2], p[3], p[4]),
        byte5: p[5],
        transport_bits,
        model_ids,
        model_bytes_hex: hidlab_hidpp::hex(model),
        rest_hex: tail_hex(p, 13),
    })
}

pub fn decode_fw_entity(entity: u8, p: &[u8]) -> Result<FwEntity, ReadError> {
    need(p, 2, "getFwInfo")?;
    let kind_code = p[0] & 0x0F;
    let kind = match kind_code {
        0 => "firmware",
        1 => "bootloader",
        2 => "hardware",
        3 => "other",
        _ => "unknown",
    };
    let mut e = FwEntity {
        entity,
        kind_code,
        kind,
        kind_high_nibble: p[0] >> 4,
        name: None,
        version: None,
        build: None,
        byte8: None,
        extras_hex: None,
        hardware_version: None,
        raw_hex: tail_hex(p, 0),
    };
    match kind_code {
        0 | 1 => {
            need(p, 9, "getFwInfo firmware entry")?;
            e.name = Some(String::from_utf8_lossy(&p[1..4]).into_owned());
            e.version = Some(format!("{:02X}.{:02X}", p[4], p[5]));
            e.build = Some(u16::from_be_bytes([p[6], p[7]]));
            e.byte8 = Some(p[8]);
            let extras = tail_hex(p, 9);
            e.extras_hex = (!extras.is_empty()).then_some(extras);
        }
        2 => e.hardware_version = Some(p[1]),
        _ => {}
    }
    Ok(e)
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<Firmware, ReadError> {
    let info = decode_device_info(&r.call(FEATURE, 0, &[])?)?;
    let mut notes = Vec::new();
    let mut entities = Vec::new();
    for entity in 0..info.entity_count {
        let res = r
            .call(FEATURE, 1, &[entity])
            .and_then(|p| decode_fw_entity(entity, &p));
        if let Some(e) = soft(res, &mut notes)? {
            entities.push(e);
        }
    }
    Ok(Firmware {
        info,
        entities,
        notes,
    })
}

/// Placeholder written over redacted bytes.
pub const REDACTED_BYTE: &str = "**";

/// Masks the unit ID (report bytes 5..9) in every getDeviceInfo response
/// recorded in `transcript`. `feature_index` is where 0x0003 lives on this
/// device; `sw_id` is the session's software ID.
pub fn redact_unit_id(transcript: &mut Transcript, feature_index: u8, sw_id: u8) {
    let fi = format!("{feature_index:02x}");
    let fn_sw = format!("{sw_id:02x}"); // function 0 => high nibble 0
    for e in &mut transcript.entries {
        let mut bytes: Vec<String> = e.hex.split(' ').map(str::to_owned).collect();
        if bytes.len() >= 9
            && (bytes[0] == "10" || bytes[0] == "11")
            && bytes[2] == fi
            && bytes[3] == fn_sw
            && e.dir == hidlab_hidpp::Direction::Rx
        {
            for b in &mut bytes[5..9] {
                *b = REDACTED_BYTE.to_owned();
            }
            e.hex = bytes.join(" ");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidlab_hidpp::{Direction, Disposition, TranscriptEntry};

    #[test]
    fn decodes_device_info() {
        // SYNTHETIC: 3 entities, unit 01020304, transports btle+usb.
        let p = [
            3, 0x01, 0x02, 0x03, 0x04, 0x55, 0x0A, 0xB0, 0x99, 0xC0, 0x88, 0, 0, 0x7E, 0, 0, 0,
        ];
        let i = decode_device_info(&p).unwrap();
        assert_eq!(i.entity_count, 3);
        assert_eq!(i.unit_id, "01020304");
        assert_eq!(i.byte5, 0x55);
        assert_eq!(
            i.model_ids,
            [
                ModelId {
                    transport: "btle",
                    id: "B099".into()
                },
                ModelId {
                    transport: "usb",
                    id: "C088".into()
                },
            ]
        );
        assert_eq!(i.rest_hex, "7e");
    }

    #[test]
    fn decodes_firmware_and_hardware_entries() {
        // SYNTHETIC
        let fw = [
            0x00, b'T', b'S', b'T', 0x12, 0x03, 0x00, 0x2A, 0x01, 0xAB, 0, 0,
        ];
        let e = decode_fw_entity(0, &fw).unwrap();
        assert_eq!(e.kind, "firmware");
        assert_eq!(e.name.as_deref(), Some("TST"));
        assert_eq!(e.version.as_deref(), Some("12.03"));
        assert_eq!(e.build, Some(0x002A));
        assert_eq!(e.byte8, Some(1));
        assert_eq!(e.extras_hex.as_deref(), Some("ab"));

        let hw = decode_fw_entity(2, &[0x02, 0x05, 0, 0]).unwrap();
        assert_eq!(hw.kind, "hardware");
        assert_eq!(hw.hardware_version, Some(5));
        assert_eq!(hw.name, None);

        assert_eq!(decode_fw_entity(3, &[0x0F, 0]).unwrap().kind, "unknown");
        assert!(decode_fw_entity(0, &[0x00, 1, 2]).is_err());
    }

    #[test]
    fn redacts_unit_id_in_transcript() {
        let entry = |dir, hex: &str| TranscriptEntry {
            request: 1,
            t_us: 0,
            dir,
            disposition: Disposition::Matched,
            hex: hex.into(),
        };
        let mut t = Transcript::default();
        t.push(entry(Direction::Tx, "11 ff 02 0b 00 00 00 00 00 00"));
        t.push(entry(Direction::Rx, "11 ff 02 0b 03 de ad be ef 00 0a b0"));
        t.push(entry(Direction::Rx, "11 ff 02 1b 00 de ad be ef 00 00 00"));
        redact_unit_id(&mut t, 0x02, 0x0B);
        assert_eq!(t.entries[0].hex, "11 ff 02 0b 00 00 00 00 00 00");
        assert_eq!(t.entries[1].hex, "11 ff 02 0b 03 ** ** ** ** 00 0a b0");
        assert_eq!(t.entries[2].hex, "11 ff 02 1b 00 de ad be ef 00 00 00");
    }
}
