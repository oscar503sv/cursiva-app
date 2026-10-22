//! hidraw discovery, HID report-descriptor walking and raw I/O.

pub mod uevent;

pub use uevent::{Bus, Uevent, UeventError, parse_uevent};
