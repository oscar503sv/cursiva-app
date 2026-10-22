//! 0x0005 DeviceName (read-only).
//!
//! Sources: kernel `hid-logitech-hidpp.c` (getCount 0x00, getDeviceName 0x10,
//! getType 0x20) and Solaar `hidpp20.py` `get_name`, `get_kind`, with the
//! type table from Solaar `hidpp20_constants.py` `DEVICE_KIND`. Status: LIKELY.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader};
use crate::util::{need, soft};

pub const FEATURE: FeatureId = FeatureId(0x0005);

/// Type codes as listed by Solaar `DEVICE_KIND` (unverified reference).
const DEVICE_TYPES: &[(u8, &str)] = &[
    (0, "keyboard"),
    (1, "remote_control"),
    (2, "numpad"),
    (3, "mouse"),
    (4, "touchpad"),
    (5, "trackball"),
    (6, "presenter"),
    (7, "receiver"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceType {
    pub code: u8,
    /// Reference label, `None` if the code is not listed.
    pub reference_label: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceName {
    pub length: u8,
    /// Name decoded as UTF-8 (lossy).
    pub name: String,
    pub name_hex: String,
    pub device_type: Option<DeviceType>,
    pub notes: Vec<String>,
}

pub fn device_type(code: u8) -> DeviceType {
    DeviceType {
        code,
        reference_label: DEVICE_TYPES
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, l)| *l),
    }
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<DeviceName, ReadError> {
    let p = r.call(FEATURE, 0, &[])?;
    need(&p, 1, "getDeviceNameCount")?;
    let length = p[0];

    let mut bytes: Vec<u8> = Vec::with_capacity(usize::from(length));
    while bytes.len() < usize::from(length) {
        let char_index = bytes.len() as u8;
        let chunk = r.call(FEATURE, 1, &[char_index])?;
        let take = chunk.len().min(usize::from(length) - bytes.len());
        if take == 0 {
            return Err(ReadError::Decode {
                what: "getDeviceName",
                reason: format!("empty chunk at index {char_index}"),
            });
        }
        bytes.extend_from_slice(&chunk[..take]);
    }

    let mut notes = Vec::new();
    let device_type = soft(r.call(FEATURE, 2, &[]), &mut notes)?
        .and_then(|p| p.first().copied())
        .map(device_type);

    Ok(DeviceName {
        length,
        name: String::from_utf8_lossy(&bytes).into_owned(),
        name_hex: hidlab_hidpp::hex(&bytes),
        device_type,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_labels() {
        assert_eq!(device_type(3).reference_label, Some("mouse"));
        assert_eq!(device_type(42).reference_label, None);
    }
}
