//! hidraw discovery, HID report-descriptor walking and raw I/O.

pub mod descriptor;
pub mod discover;
pub mod error;
pub mod hidraw;
pub mod power_supply;
#[cfg(test)]
mod test_support;
pub mod uevent;

pub use descriptor::{
    DescriptorSummary, HIDPP_LONG_REPORT_ID, HIDPP_SHORT_REPORT_ID, walk_descriptor,
};
pub use discover::{
    Discovery, HidrawDevice, LOGITECH_VENDOR_ID, SkippedEntry, discover, discover_in,
};
pub use error::{TransportError, UDEV_INSTALL_HINT};
pub use hidraw::{Access, AccessCheck, Hidraw, RawChannel, check_access};
pub use power_supply::{PowerSupply, power_supplies_for, power_supplies_for_in};
pub use uevent::{Bus, Uevent, UeventError, parse_uevent};
