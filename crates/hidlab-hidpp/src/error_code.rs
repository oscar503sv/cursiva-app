//! HID++ error codes.
//!
//! Status: LIKELY (docs/protocol/README.md, P-006 and P-007). Names follow
//! the Linux kernel driver `hid-logitech-hidpp.c` (GPL-2.0); behavior
//! described, no code copied. Unknown values are kept, never guessed.

use serde::Serialize;

/// HID++ 2.0 error code (byte 5 of an error response).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hidpp20Error {
    NoError,
    Unknown,
    InvalidArgument,
    OutOfRange,
    HardwareError,
    LogitechInternal,
    InvalidFeatureIndex,
    InvalidFunctionId,
    Busy,
    Unsupported,
    Other(u8),
}

impl From<u8> for Hidpp20Error {
    fn from(code: u8) -> Self {
        match code {
            0x00 => Self::NoError,
            0x01 => Self::Unknown,
            0x02 => Self::InvalidArgument,
            0x03 => Self::OutOfRange,
            0x04 => Self::HardwareError,
            0x05 => Self::LogitechInternal,
            0x06 => Self::InvalidFeatureIndex,
            0x07 => Self::InvalidFunctionId,
            0x08 => Self::Busy,
            0x09 => Self::Unsupported,
            other => Self::Other(other),
        }
    }
}

impl Hidpp20Error {
    pub fn code(self) -> u8 {
        match self {
            Self::NoError => 0x00,
            Self::Unknown => 0x01,
            Self::InvalidArgument => 0x02,
            Self::OutOfRange => 0x03,
            Self::HardwareError => 0x04,
            Self::LogitechInternal => 0x05,
            Self::InvalidFeatureIndex => 0x06,
            Self::InvalidFunctionId => 0x07,
            Self::Busy => 0x08,
            Self::Unsupported => 0x09,
            Self::Other(c) => c,
        }
    }

    /// Plain-language description.
    pub fn describe(self) -> &'static str {
        match self {
            Self::NoError => "no error",
            Self::Unknown => "unknown error",
            Self::InvalidArgument => "invalid argument",
            Self::OutOfRange => "value out of range",
            Self::HardwareError => "hardware error",
            Self::LogitechInternal => "internal device error",
            Self::InvalidFeatureIndex => "invalid feature index",
            Self::InvalidFunctionId => "invalid function",
            Self::Busy => "device busy",
            Self::Unsupported => "unsupported",
            Self::Other(_) => "unrecognised error code",
        }
    }
}

/// HID++ 1.0 error code (byte 5 of a sub-ID 0x8F report).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hidpp10Error {
    Success,
    InvalidSubId,
    InvalidAddress,
    InvalidValue,
    ConnectFail,
    TooManyDevices,
    AlreadyExists,
    Busy,
    UnknownDevice,
    ResourceError,
    RequestUnavailable,
    InvalidParamValue,
    WrongPinCode,
    Other(u8),
}

impl From<u8> for Hidpp10Error {
    fn from(code: u8) -> Self {
        match code {
            0x00 => Self::Success,
            0x01 => Self::InvalidSubId,
            0x02 => Self::InvalidAddress,
            0x03 => Self::InvalidValue,
            0x04 => Self::ConnectFail,
            0x05 => Self::TooManyDevices,
            0x06 => Self::AlreadyExists,
            0x07 => Self::Busy,
            0x08 => Self::UnknownDevice,
            0x09 => Self::ResourceError,
            0x0A => Self::RequestUnavailable,
            0x0B => Self::InvalidParamValue,
            0x0C => Self::WrongPinCode,
            other => Self::Other(other),
        }
    }
}

impl Hidpp10Error {
    pub fn code(self) -> u8 {
        match self {
            Self::Success => 0x00,
            Self::InvalidSubId => 0x01,
            Self::InvalidAddress => 0x02,
            Self::InvalidValue => 0x03,
            Self::ConnectFail => 0x04,
            Self::TooManyDevices => 0x05,
            Self::AlreadyExists => 0x06,
            Self::Busy => 0x07,
            Self::UnknownDevice => 0x08,
            Self::ResourceError => 0x09,
            Self::RequestUnavailable => 0x0A,
            Self::InvalidParamValue => 0x0B,
            Self::WrongPinCode => 0x0C,
            Self::Other(c) => c,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidpp20_roundtrip() {
        for c in 0..=0xFFu8 {
            assert_eq!(Hidpp20Error::from(c).code(), c);
        }
        assert_eq!(Hidpp20Error::from(0x42), Hidpp20Error::Other(0x42));
    }

    #[test]
    fn hidpp10_roundtrip() {
        for c in 0..=0xFFu8 {
            assert_eq!(Hidpp10Error::from(c).code(), c);
        }
    }
}
