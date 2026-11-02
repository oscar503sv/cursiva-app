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
//! | `Reset` | setting name or `all` | [`ResetReply`] JSON |
//!
//! Signals: `DeviceChanged` ([`DeviceEvent`] JSON), `Applied`
//! ([`ApplyRecord`] JSON) and `ModeChanged` ([`ModeEvent`] JSON).

use hidlab_features::settings::{SettingKind, SettingValue};
use serde::{Deserialize, Serialize};

use crate::apply::ApplyReport;
use crate::conflicts::Conflict;

pub const BUS_NAME: &str = "dev.ozkkar.Cursiva.Daemon1";
pub const OBJECT_PATH: &str = "/dev/ozkkar/Cursiva/Daemon1";
pub const INTERFACE: &str = "dev.ozkkar.Cursiva.Daemon1";
/// systemd user unit, for messages.
pub const UNIT: &str = "hid-labd.service";

pub const STATUS_SCHEMA: &str = "hid-lab.daemon-status/2";
pub const SET_SCHEMA: &str = "hid-lab.daemon-set/1";
pub const REAPPLY_SCHEMA: &str = "hid-lab.daemon-reapply/1";
pub const RESET_SCHEMA: &str = "hid-lab.daemon-reset/1";
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

/// What made the daemon apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// The daemon started with the mouse connected.
    Start,
    /// The mouse (re)connected: udev `add`.
    Connect,
    /// The system woke up (logind `PrepareForSleep(false)`).
    Resume,
    /// `hid-lab reapply`.
    Reapply,
    /// `hid-lab set`.
    Set,
    /// `hid-lab reset`.
    Reset,
}

/// The second verification, 1 s after the first (requirements, Milestone 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecondCheck {
    pub ok: bool,
    /// From the event to the end of this check, including any re-apply.
    pub since_event_ms: u64,
    /// Settings found changed after the first verification, with the value read.
    pub overwritten: Vec<CurrentValue>,
    /// The re-apply sent because of `overwritten`.
    pub reapplied: Option<ApplyReport>,
}

/// One application of settings and its checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyRecord {
    pub trigger: Trigger,
    pub at_unix: u64,
    pub report: ApplyReport,
    /// From the event (udev `add`, wake-up, start of the call) to the end of
    /// the first verification.
    pub since_event_ms: u64,
    /// For `connect`, `resume` and `start` only.
    pub second_check: Option<SecondCheck>,
}

impl ApplyRecord {
    /// True when the first verification passed and the second one, if it
    /// ran, found nothing overwritten (or fixed it).
    pub fn ok(&self) -> bool {
        self.report.ok && self.second_check.as_ref().is_none_or(|c| c.ok)
    }

    /// Time from the event to the last check.
    pub fn total_ms(&self) -> u64 {
        self.second_check
            .as_ref()
            .map_or(self.since_event_ms, |c| c.since_event_ms)
    }
}

/// The user changed the SmartShift mode on the mouse and the daemon
/// remembered it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeEvent {
    pub schema: String,
    pub mode: SettingValue,
}

pub const MODE_EVENT_SCHEMA: &str = "hid-lab.daemon-mode/1";

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
    pub last_apply: Option<ApplyRecord>,
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

/// `Reset`: the settings were removed from the config file and their
/// defaults written to the mouse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResetReply {
    pub schema: String,
    /// The defaults written, one per setting reset.
    pub values: Vec<SettingValue>,
    /// The files changed.
    pub saved_to: Vec<String>,
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
