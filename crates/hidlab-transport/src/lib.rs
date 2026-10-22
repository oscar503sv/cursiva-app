//! hidraw discovery, HID report-descriptor walking and raw I/O.

pub mod descriptor;
pub mod error;
pub mod hidraw;
pub mod uevent;

pub use descriptor::{
    DescriptorSummary, HIDPP_LONG_REPORT_ID, HIDPP_SHORT_REPORT_ID, walk_descriptor,
};
pub use error::{TransportError, UDEV_INSTALL_HINT};
pub use hidraw::{Access, AccessCheck, Hidraw, RawChannel, check_access};
pub use uevent::{Bus, Uevent, UeventError, parse_uevent};
