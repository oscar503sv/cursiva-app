//! HID++ session errors with plain-language messages.

use std::time::Duration;

use hidlab_transport::TransportError;
use thiserror::Error;

use crate::error_code::{Hidpp10Error, Hidpp20Error};
use crate::frame::FrameError;

#[derive(Debug, Error)]
pub enum HidppError {
    #[error(
        "No answer from the device to HID++ request (feature index 0x{feature_index:02x}, function {function}) within {} ms; {ignored} unrelated report(s) were ignored while waiting.\n\
         Likely cause: the mouse is asleep, the device index is wrong (for a receiver, try --index 1..6), or the device does not speak HID++ 2.0.\n\
         Try: move the mouse to wake it and run the command again, or raise --timeout-ms.",
        waited.as_millis()
    )]
    Timeout {
        feature_index: u8,
        function: u8,
        waited: Duration,
        ignored: u64,
    },

    #[error(
        "The device rejected HID++ request (feature index 0x{feature_index:02x}, function {function}): {} (HID++ 2.0 error 0x{:02x}).",
        code.describe(),
        code.code()
    )]
    Device {
        feature_index: u8,
        function: u8,
        code: Hidpp20Error,
    },

    #[error(
        "The device answered with a HID++ 1.0 error to request (feature index 0x{feature_index:02x}, function {function}): {code:?} (0x{:02x}).\n\
         Likely cause: the addressed device is a HID++ 1.0 device or a receiver, not a HID++ 2.0 device.\n\
         Try: if this is a receiver, pass --index 1..6 to address a paired device.",
        code.code()
    )]
    Device10 {
        feature_index: u8,
        function: u8,
        code: Hidpp10Error,
    },

    #[error(
        "Ping mismatch: sent 0x{sent:02x}, got 0x{got:02x}. Another program may be talking to the device."
    )]
    PingMismatch { sent: u8, got: u8 },

    #[error(
        "The device does not report the IFeatureSet feature (0x0001), so its features cannot be listed."
    )]
    FeatureSetMissing,

    #[error("Unexpected response: {0}")]
    Malformed(String),

    #[error(transparent)]
    Frame(#[from] FrameError),

    #[error(transparent)]
    Transport(#[from] TransportError),
}
