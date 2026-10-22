//! D-Bus names and the JSON documents the daemon returns. Shared with the
//! CLI (and later the GUI), which deserialize them.
//!
//! Methods of [`INTERFACE`] at [`OBJECT_PATH`]:
//!
//! | Method | In | Out |
//! |---|---|---|
//! | `GetStatus` | — | [`Status`] JSON |
//! | `Set` | setting name, value | [`SetReply`] JSON |
//! | `Reapply` | — | [`ReapplyReply`] JSON |
//!
//! Signals: `DeviceChanged` ([`DeviceEvent`] JSON) and `Applied`
//! ([`ReapplyReply`] JSON).

use hidlab_features::settings::{SettingKind, SettingValue};
use serde::{Deserialize, Serialize};

use crate::apply::ApplyReport;
use crate::conflicts::Conflict;

pub const BUS_NAME: &str = "dev.ozkkar.Cursiva.Daemon1";
pub const OBJECT_PATH: &str = "/dev/ozkkar/Cursiva/Daemon1";
pub const INTERFACE: &str = "dev.ozkkar.Cursiva.Daemon1";
/// systemd user unit, for messages.
pub const UNIT: &str = "hid-labd.service";

pub const STATUS_SCHEMA: &str = "hid-lab.daemon-status/1";
pub const SET_SCHEMA: &str = "hid-lab.daemon-set/1";
pub const REAPPLY_SCHEMA: &str = "hid-lab.daemon-reapply/1";
pub const DEVICE_EVENT_SCHEMA: &str = "hid-lab.daemon-device/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Profile key, e.g. `mx-master-3s`.
    pub profile: String,
    pub name: String,
    pub node: String,
    pub hid_device: Option<String>,
    pub bus: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// `ok`, `permission_denied`, ...
    pub access: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentValue {
    pub setting: SettingKind,
    pub value: Option<SettingValue>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub schema: String,
    pub daemon_version: String,
    pub config_file: String,
    /// Set when the config file is invalid; the daemon keeps the last valid one.
    pub config_error: Option<String>,
    pub state_error: Option<String>,
    pub device: Option<DeviceInfo>,
    /// What the daemon applies, in order.
    pub desired: Vec<SettingValue>,
    /// Read from the mouse for this status call; empty without a device.
    pub current: Vec<CurrentValue>,
    pub last_apply: Option<ApplyReport>,
    pub last_apply_unix: Option<u64>,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetReply {
    pub schema: String,
    pub value: SettingValue,
    /// Where the value was stored, or `None` if the mouse rejected it.
    pub saved_to: Option<String>,
    /// `None` when the mouse is not connected.
    pub applied: Option<ApplyReport>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReapplyReply {
    pub schema: String,
    pub config_error: Option<String>,
    pub report: ApplyReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceEvent {
    pub schema: String,
    pub connected: bool,
    pub device: Option<DeviceInfo>,
}
