//! Generic HID++ 2.0 protocol support. Knows nothing about specific devices.

pub mod error_code;
pub mod frame;

pub use error_code::{Hidpp10Error, Hidpp20Error};
pub use frame::{
    DEVICE_INDEX_DIRECT, Frame, FrameError, HIDPP10_ERROR_SUB_ID, HIDPP20_ERROR_INDEX, ReportKind,
    Request, decode, hex,
};
