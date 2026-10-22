//! hidraw discovery, HID report-descriptor walking and raw I/O.

pub mod descriptor;
pub mod uevent;

pub use descriptor::{
    DescriptorSummary, HIDPP_LONG_REPORT_ID, HIDPP_SHORT_REPORT_ID, walk_descriptor,
};
pub use uevent::{Bus, Uevent, UeventError, parse_uevent};
