//! HID++ report framing.
//!
//! Status of each fact: see docs/protocol/README.md, entries P-001..P-007
//! (long-report framing CONFIRMED on the MX Master 3S over Bluetooth). Sources: Linux `drivers/hid/hid-logitech-hidpp.c` and
//! Solaar (both GPL-2.0), behavior described, no code copied.
//!
//! ```text
//! [report_id, device_index, feature_index, (function << 4) | sw_id, params...]
//! short: report 0x10, 7 bytes  (3 param bytes)
//! long:  report 0x11, 20 bytes (16 param bytes)
//! HID++ 2.0 error: feature_index 0xFF,
//!     params [orig_feature_index, orig_function_swid, error_code, ...]
//! HID++ 1.0 error: sub-id 0x8F in the feature_index position,
//!     params [orig_sub_id, orig_address, error_code, ...]
//! ```

use serde::Serialize;
use thiserror::Error;

use crate::error_code::{Hidpp10Error, Hidpp20Error};

/// Device index for a directly connected device (Bluetooth, wired).
pub const DEVICE_INDEX_DIRECT: u8 = 0xFF;
/// `feature_index` value that marks a HID++ 2.0 error response.
pub const HIDPP20_ERROR_INDEX: u8 = 0xFF;
/// Sub-ID that marks a HID++ 1.0 error response.
pub const HIDPP10_ERROR_SUB_ID: u8 = 0x8F;

/// HID++ report size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    Short,
    Long,
}

impl ReportKind {
    pub const fn report_id(self) -> u8 {
        match self {
            ReportKind::Short => 0x10,
            ReportKind::Long => 0x11,
        }
    }

    /// Total report length including the report ID.
    pub const fn report_len(self) -> usize {
        match self {
            ReportKind::Short => 7,
            ReportKind::Long => 20,
        }
    }

    /// Number of parameter bytes.
    pub const fn param_capacity(self) -> usize {
        self.report_len() - 4
    }

    pub const fn from_report_id(id: u8) -> Option<Self> {
        match id {
            0x10 => Some(ReportKind::Short),
            0x11 => Some(ReportKind::Long),
            _ => None,
        }
    }
}

/// A HID++ 2.0 request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub device_index: u8,
    pub feature_index: u8,
    /// 0..=15
    pub function: u8,
    /// 1..=15 for requests (0 is used by device notifications).
    pub sw_id: u8,
    pub params: Vec<u8>,
}

/// Framing errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FrameError {
    #[error("function {0} does not fit in 4 bits")]
    FunctionOutOfRange(u8),
    #[error("software ID {0} must be 1..=15")]
    SwIdOutOfRange(u8),
    #[error("{len} parameter bytes do not fit in a {kind:?} report ({capacity} max)")]
    ParamsTooLong {
        kind: ReportKind,
        len: usize,
        capacity: usize,
    },
    #[error("empty report")]
    Empty,
    #[error("HID++ report 0x{report_id:02x} is {len} bytes, expected at least {expected}")]
    TooShort {
        report_id: u8,
        len: usize,
        expected: usize,
    },
}

impl Request {
    /// Encodes into a full report, zero-padding the parameters.
    pub fn encode(&self, kind: ReportKind) -> Result<Vec<u8>, FrameError> {
        if self.function > 0x0F {
            return Err(FrameError::FunctionOutOfRange(self.function));
        }
        if self.sw_id == 0 || self.sw_id > 0x0F {
            return Err(FrameError::SwIdOutOfRange(self.sw_id));
        }
        if self.params.len() > kind.param_capacity() {
            return Err(FrameError::ParamsTooLong {
                kind,
                len: self.params.len(),
                capacity: kind.param_capacity(),
            });
        }
        let mut out = vec![0u8; kind.report_len()];
        out[0] = kind.report_id();
        out[1] = self.device_index;
        out[2] = self.feature_index;
        out[3] = (self.function << 4) | self.sw_id;
        out[4..4 + self.params.len()].copy_from_slice(&self.params);
        Ok(out)
    }
}

/// A decoded incoming report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// A normal HID++ 2.0 response or notification.
    Message {
        kind: ReportKind,
        device_index: u8,
        feature_index: u8,
        function: u8,
        sw_id: u8,
        params: Vec<u8>,
    },
    /// HID++ 2.0 error response.
    Error20 {
        device_index: u8,
        orig_feature_index: u8,
        orig_function: u8,
        sw_id: u8,
        code: Hidpp20Error,
    },
    /// HID++ 1.0 error response.
    Error10 {
        device_index: u8,
        orig_sub_id: u8,
        orig_address: u8,
        code: Hidpp10Error,
    },
    /// Any report that is not HID++ (mouse movement, etc.).
    NotHidpp { report_id: u8 },
}

/// Decodes an incoming report. Non-HID++ report IDs are not an error.
pub fn decode(bytes: &[u8]) -> Result<Frame, FrameError> {
    let &report_id = bytes.first().ok_or(FrameError::Empty)?;
    let Some(kind) = ReportKind::from_report_id(report_id) else {
        return Ok(Frame::NotHidpp { report_id });
    };
    if bytes.len() < kind.report_len() {
        return Err(FrameError::TooShort {
            report_id,
            len: bytes.len(),
            expected: kind.report_len(),
        });
    }
    let device_index = bytes[1];
    let feature_index = bytes[2];
    let b3 = bytes[3];

    Ok(match feature_index {
        HIDPP20_ERROR_INDEX => Frame::Error20 {
            device_index,
            orig_feature_index: b3,
            orig_function: bytes[4] >> 4,
            sw_id: bytes[4] & 0x0F,
            code: Hidpp20Error::from(bytes[5]),
        },
        HIDPP10_ERROR_SUB_ID => Frame::Error10 {
            device_index,
            orig_sub_id: b3,
            orig_address: bytes[4],
            code: Hidpp10Error::from(bytes[5]),
        },
        _ => Frame::Message {
            kind,
            device_index,
            feature_index,
            function: b3 >> 4,
            sw_id: b3 & 0x0F,
            params: bytes[4..kind.report_len()].to_vec(),
        },
    })
}

/// Lowercase space-separated hex, e.g. `10 ff 00 1b`.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(params: &[u8]) -> Request {
        Request {
            device_index: DEVICE_INDEX_DIRECT,
            feature_index: 0x00,
            function: 1,
            sw_id: 0x0B,
            params: params.to_vec(),
        }
    }

    #[test]
    fn encodes_short_and_long() {
        assert_eq!(
            req(&[0, 0, 0x5A]).encode(ReportKind::Short).unwrap(),
            [0x10, 0xFF, 0x00, 0x1B, 0x00, 0x00, 0x5A]
        );
        let long = req(&[0x00, 0x01]).encode(ReportKind::Long).unwrap();
        assert_eq!(long.len(), 20);
        assert_eq!(&long[..6], &[0x11, 0xFF, 0x00, 0x1B, 0x00, 0x01]);
        assert!(long[6..].iter().all(|&b| b == 0));
    }

    #[test]
    fn rejects_bad_requests() {
        let mut r = req(&[]);
        r.function = 16;
        assert_eq!(
            r.encode(ReportKind::Long),
            Err(FrameError::FunctionOutOfRange(16))
        );
        let mut r = req(&[]);
        r.sw_id = 0;
        assert_eq!(
            r.encode(ReportKind::Long),
            Err(FrameError::SwIdOutOfRange(0))
        );
        assert!(matches!(
            req(&[1, 2, 3, 4]).encode(ReportKind::Short),
            Err(FrameError::ParamsTooLong { .. })
        ));
    }

    #[test]
    fn decodes_message() {
        let f = decode(&[0x10, 0xFF, 0x00, 0x1B, 0x04, 0x05, 0x5A]).unwrap();
        assert_eq!(
            f,
            Frame::Message {
                kind: ReportKind::Short,
                device_index: 0xFF,
                feature_index: 0,
                function: 1,
                sw_id: 0x0B,
                params: vec![0x04, 0x05, 0x5A],
            }
        );
    }

    #[test]
    fn decodes_hidpp20_error() {
        let mut b = [0u8; 20];
        b[..6].copy_from_slice(&[0x11, 0xFF, 0xFF, 0x05, 0x1B, 0x02]);
        assert_eq!(
            decode(&b).unwrap(),
            Frame::Error20 {
                device_index: 0xFF,
                orig_feature_index: 0x05,
                orig_function: 1,
                sw_id: 0x0B,
                code: Hidpp20Error::InvalidArgument,
            }
        );
    }

    #[test]
    fn decodes_hidpp10_error() {
        assert_eq!(
            decode(&[0x10, 0x01, 0x8F, 0x00, 0x1B, 0x01, 0x00]).unwrap(),
            Frame::Error10 {
                device_index: 0x01,
                orig_sub_id: 0x00,
                orig_address: 0x1B,
                code: Hidpp10Error::InvalidSubId,
            }
        );
    }

    #[test]
    fn non_hidpp_and_short_reports() {
        assert_eq!(
            decode(&[0x02, 0, 0, 0]).unwrap(),
            Frame::NotHidpp { report_id: 0x02 }
        );
        assert_eq!(decode(&[]), Err(FrameError::Empty));
        assert!(matches!(
            decode(&[0x11, 0xFF, 0x00]),
            Err(FrameError::TooShort { .. })
        ));
    }

    #[test]
    fn roundtrip_long() {
        let r = Request {
            device_index: 2,
            feature_index: 7,
            function: 0xF,
            sw_id: 0xF,
            params: (1..=16).collect(),
        };
        let f = decode(&r.encode(ReportKind::Long).unwrap()).unwrap();
        let Frame::Message {
            device_index,
            feature_index,
            function,
            sw_id,
            params,
            ..
        } = f
        else {
            panic!("not a message");
        };
        assert_eq!(
            (device_index, feature_index, function, sw_id),
            (2, 7, 0xF, 0xF)
        );
        assert_eq!(params, r.params);
    }

    #[test]
    fn hex_format() {
        assert_eq!(hex(&[0x10, 0xFF, 0x0A]), "10 ff 0a");
        assert_eq!(hex(&[]), "");
    }
}
