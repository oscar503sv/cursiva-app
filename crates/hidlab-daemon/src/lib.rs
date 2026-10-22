//! hid-lab user daemon (`hid-labd`).
//!
//! The daemon is the single owner of the mouse configuration (requirements
//! §5). It writes only functions in
//! [`hidlab_features::CONFIRMED_WRITES`], and only settings the device's
//! profile in `hidlab-devices` lists.

pub mod apply;
pub mod config;
pub mod conflicts;
pub mod log;
pub mod paths;
pub mod state;
