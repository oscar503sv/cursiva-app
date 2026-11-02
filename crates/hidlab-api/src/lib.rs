//! The hid-labd contract: D-Bus names, the JSON documents of the D-Bus API,
//! and the settings vocabulary (names, values, remap targets).
//!
//! It depends on serde only, so the Cursiva app can use it without linking
//! any device code (requirements §5; the app talks only to the daemon).

pub mod daemon;
pub mod remap;
pub mod settings;
