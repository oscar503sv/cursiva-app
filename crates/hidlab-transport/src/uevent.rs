//! Parsing of `/sys/class/hidraw/*/device/uevent`.
//!
//! The relevant keys are written by the kernel HID core:
//! `HID_ID=BBBB:VVVVVVVV:PPPPPPPP`, `HID_NAME=...`, `HID_UNIQ=...`.

use serde::Serialize;
use thiserror::Error;

/// HID bus type, from the first field of `HID_ID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Bus {
    /// Bus `0x0003`.
    Usb,
    /// Bus `0x0005`.
    Bluetooth,
    /// Any other bus code.
    Other(u16),
}

impl Bus {
    /// Maps a raw kernel bus code to a [`Bus`].
    pub fn from_code(code: u16) -> Self {
        match code {
            0x0003 => Bus::Usb,
            0x0005 => Bus::Bluetooth,
            other => Bus::Other(other),
        }
    }

    /// Raw kernel bus code.
    pub fn code(self) -> u16 {
        match self {
            Bus::Usb => 0x0003,
            Bus::Bluetooth => 0x0005,
            Bus::Other(code) => code,
        }
    }

    /// Short human-readable label.
    pub fn label(self) -> String {
        match self {
            Bus::Usb => "usb".to_owned(),
            Bus::Bluetooth => "bluetooth".to_owned(),
            Bus::Other(code) => format!("0x{code:04x}"),
        }
    }
}

/// Fields of a hidraw parent device's uevent file that hid-lab uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uevent {
    pub bus: Bus,
    pub vendor_id: u16,
    pub product_id: u16,
    /// `HID_NAME`, if present and non-empty.
    pub name: Option<String>,
    /// `HID_UNIQ` (Bluetooth address or serial), if present and non-empty.
    /// Sensitive: callers must redact it unless the user opts in.
    pub uniq: Option<String>,
}

/// Why a uevent file could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UeventError {
    #[error("uevent has no HID_ID line")]
    MissingHidId,
    #[error("malformed HID_ID value {0:?} (expected BBBB:VVVVVVVV:PPPPPPPP)")]
    MalformedHidId(String),
}

/// Parses the text of a uevent file.
pub fn parse_uevent(text: &str) -> Result<Uevent, UeventError> {
    let mut hid_id = None;
    let mut name = None;
    let mut uniq = None;

    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "HID_ID" => hid_id = Some(value.to_owned()),
            "HID_NAME" if !value.is_empty() => name = Some(value.to_owned()),
            "HID_UNIQ" if !value.is_empty() => uniq = Some(value.to_owned()),
            _ => {}
        }
    }

    let hid_id = hid_id.ok_or(UeventError::MissingHidId)?;
    let (bus, vendor_id, product_id) =
        parse_hid_id(&hid_id).ok_or_else(|| UeventError::MalformedHidId(hid_id.clone()))?;

    Ok(Uevent {
        bus: Bus::from_code(bus),
        vendor_id,
        product_id,
        name,
        uniq,
    })
}

fn parse_hid_id(value: &str) -> Option<(u16, u16, u16)> {
    let mut parts = value.split(':');
    let bus = parse_hex_u16(parts.next()?)?;
    let vendor = parse_hex_u16(parts.next()?)?;
    let product = parse_hex_u16(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some((bus, vendor, product))
}

/// Parses a hex field that may be zero-padded wider than 16 bits.
fn parse_hex_u16(field: &str) -> Option<u16> {
    if field.is_empty() || field.len() > 8 {
        return None;
    }
    let value = u32::from_str_radix(field, 16).ok()?;
    u16::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic uevent shaped like the kernel's output; the address is fake.
    const SYNTHETIC_BT: &str = "DRIVER=logitech-hidpp-device\n\
        HID_ID=0005:0000046D:0000B034\n\
        HID_NAME=Logitech MX Master 3S\n\
        HID_PHYS=00:00:00:00:00:00\n\
        HID_UNIQ=aa:bb:cc:dd:ee:ff\n\
        MODALIAS=hid:b0005g0000v0000046Dp0000B034\n";

    #[test]
    fn parses_bluetooth_uevent() {
        let u = parse_uevent(SYNTHETIC_BT).unwrap();
        assert_eq!(u.bus, Bus::Bluetooth);
        assert_eq!(u.vendor_id, 0x046D);
        assert_eq!(u.product_id, 0xB034);
        assert_eq!(u.name.as_deref(), Some("Logitech MX Master 3S"));
        assert_eq!(u.uniq.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn parses_usb_and_empty_optional_fields() {
        let u = parse_uevent("HID_ID=0003:0000046D:0000C548\nHID_NAME=\nHID_UNIQ=\n").unwrap();
        assert_eq!(u.bus, Bus::Usb);
        assert_eq!(u.product_id, 0xC548);
        assert_eq!(u.name, None);
        assert_eq!(u.uniq, None);
    }

    #[test]
    fn unknown_bus_is_kept() {
        let u = parse_uevent("HID_ID=0019:00000001:00000002\n").unwrap();
        assert_eq!(u.bus, Bus::Other(0x19));
        assert_eq!(u.bus.label(), "0x0019");
    }

    #[test]
    fn missing_hid_id_is_an_error() {
        assert_eq!(parse_uevent("HID_NAME=x\n"), Err(UeventError::MissingHidId));
    }

    #[test]
    fn malformed_hid_id_is_an_error() {
        for bad in [
            "HID_ID=0005:046D",
            "HID_ID=zz05:0000046D:0000B034",
            "HID_ID=0005:0001046D:0000B034",
            "HID_ID=0005:0000046D:0000B034:1",
            "HID_ID=",
        ] {
            assert!(
                matches!(parse_uevent(bad), Err(UeventError::MalformedHidId(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn ignores_lines_without_equals() {
        let u = parse_uevent("garbage\nHID_ID=0005:0000046D:0000B034\n").unwrap();
        assert_eq!(u.product_id, 0xB034);
    }
}
