//! Typed, read-only access to individual HID++ 2.0 features.
//!
//! Every request goes through [`Reader`], which only sends functions listed
//! in [`allowlist::READ_ALLOWLIST`]. Feature indices are resolved on the
//! device at runtime. Bytes whose meaning no source documents are kept raw.

pub mod allowlist;
pub mod battery;
pub mod device_name;
pub mod firmware;
pub mod reader;
mod util;

pub use allowlist::{KNOWN_WRITES, READ_ALLOWLIST, ReadFn, allowed};
pub use reader::{FeatureMap, ReadError, Reader};
