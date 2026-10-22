//! Typed, read-only access to individual HID++ 2.0 features.
//!
//! Every request goes through [`Reader`], which only sends functions listed
//! in [`allowlist::READ_ALLOWLIST`]. Feature indices are resolved on the
//! device at runtime. Bytes whose meaning no source documents are kept raw.

pub mod allowlist;
pub mod battery;
pub mod change_host;
pub mod device_name;
pub mod dpi;
pub mod firmware;
pub mod hires_wheel;
pub mod reader;
pub mod reprog_controls;
pub mod smartshift;
pub mod state;
pub mod thumbwheel;
mod util;

pub use allowlist::{
    KNOWN_WRITES, NEVER_WRITE, READ_ALLOWLIST, ReadFn, WRITE_EXPERIMENT_ALLOWLIST, allowed,
    write_allowed,
};
pub use reader::{FeatureMap, ReadError, Reader, WriteIntent};
pub use state::{DeviceState, Reading, TARGET_FEATURES, read_all};
