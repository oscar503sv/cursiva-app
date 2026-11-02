//! D-Bus names and the JSON documents hid-labd returns: the daemon's
//! public contract with the CLI and the app.
//!
//! Methods of [`INTERFACE`] at [`OBJECT_PATH`]:
//!
//! | Method | In | Out |
//! |---|---|---|
//! | `GetStatus` | — | [`Status`] JSON |
//! | `Set` | setting name, value | [`SetReply`] JSON |
//! | `Reapply` | — | [`ReapplyReply`] JSON |
//! | `Reset` | setting name or `all` | [`ResetReply`] JSON |
//! | `SetDesktop` | [`DesktopSetting`] name, value | [`DesktopReply`] JSON |
//! | `WatchButtons` | seconds (0 stops) | seconds granted |
//!
//! Signals: `DeviceChanged` ([`DeviceEvent`] JSON), `Applied`
//! ([`ApplyRecord`] JSON), `ModeChanged` ([`ModeEvent`] JSON),
//! `BatteryChanged` ([`BatteryEvent`] JSON) and, while a `WatchButtons`
//! lease lasts, `ButtonPressed` ([`ButtonEvent`] JSON).

use serde::{Deserialize, Serialize};

use crate::remap::RemapControl;
use crate::settings::{SettingKind, SettingValue};

pub const BUS_NAME: &str = "dev.ozkkar.Cursiva.Daemon1";
pub const OBJECT_PATH: &str = "/dev/ozkkar/Cursiva/Daemon1";
pub const INTERFACE: &str = "dev.ozkkar.Cursiva.Daemon1";
/// systemd user unit, for messages.
pub const UNIT: &str = "hid-labd.service";

pub const STATUS_SCHEMA: &str = "hid-lab.daemon-status/3";
pub const SET_SCHEMA: &str = "hid-lab.daemon-set/1";
pub const REAPPLY_SCHEMA: &str = "hid-lab.daemon-reapply/1";
pub const RESET_SCHEMA: &str = "hid-lab.daemon-reset/1";
pub const DESKTOP_SCHEMA: &str = "hid-lab.daemon-desktop/1";

/// A GNOME pointer setting the daemon reads and writes for the app (see
/// `crate::gnome`). GNOME stores it; it is not in the config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopSetting {
    /// `on` or `off`: scroll content the way the fingers move.
    NaturalScroll,
    /// `default`, `flat` or `adaptive` (GNOME `accel-profile`).
    Acceleration,
}

impl DesktopSetting {
    pub const ALL: [DesktopSetting; 2] =
        [DesktopSetting::NaturalScroll, DesktopSetting::Acceleration];

    pub fn name(self) -> &'static str {
        match self {
            DesktopSetting::NaturalScroll => "natural-scroll",
            DesktopSetting::Acceleration => "acceleration",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// Checks a user value: `on`/`off`, or `default`/`flat`/`adaptive`.
    pub fn check_value(self, value: &str) -> Result<(), String> {
        match (self, value) {
            (DesktopSetting::NaturalScroll, "on" | "off")
            | (DesktopSetting::Acceleration, "default" | "flat" | "adaptive") => Ok(()),
            (DesktopSetting::NaturalScroll, _) => Err(format!(
                "natural-scroll must be `on` or `off`, got `{value}`"
            )),
            (DesktopSetting::Acceleration, _) => Err(format!(
                "acceleration must be `default`, `flat` or `adaptive`, got `{value}`"
            )),
        }
    }
}

/// GNOME's pointer settings as read now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopSettings {
    pub natural_scroll: Option<bool>,
    pub acceleration: Option<String>,
    /// Why a value could not be read (for example, not running GNOME).
    pub error: Option<String>,
}

/// `SetDesktop`: the settings after the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopReply {
    pub schema: String,
    pub setting: DesktopSetting,
    pub value: String,
    pub desktop: DesktopSettings,
}
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

/// The mouse battery as the kernel reports it (`/sys/class/power_supply`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    /// `capacity`, percent.
    pub percent: Option<u8>,
    /// `capacity_level`, e.g. `Normal`, `Low`, `Critical`.
    pub level: Option<String>,
    /// `status`, e.g. `Discharging`, `Charging`, `Full`.
    pub status: Option<String>,
}

pub const BATTERY_EVENT_SCHEMA: &str = "hid-lab.daemon-battery/1";
pub const BUTTON_EVENT_SCHEMA: &str = "hid-lab.daemon-button/1";
/// Longest `WatchButtons` lease; clients renew it while they show presses.
pub const WATCH_BUTTONS_MAX_S: u32 = 60;

/// A button went down or up on the mouse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonEvent {
    pub schema: String,
    /// Button number in the input report; `None` for the mode button, which
    /// is seen through the SmartShift ratchet event (press only).
    pub number: Option<u16>,
    /// Control name (`left`, `back`, `mode`, ...), if known. A remapped
    /// button reports the control whose task it performs.
    pub control: Option<String>,
    pub pressed: bool,
}

/// The battery of the managed mouse changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryEvent {
    pub schema: String,
    pub battery: Option<Battery>,
}

/// DPI values the mouse accepts: `from` to `to` in steps of `step`; a single
/// value has `from == to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpiRange {
    pub from: u16,
    pub to: u16,
    pub step: u16,
}

/// What one button may be remapped to, as reported by the mouse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonTargets {
    pub button: RemapControl,
    /// Target names for `remap-<button>`, itself first. Empty if the mouse
    /// does not allow remapping it.
    pub targets: Vec<String>,
}

/// What the connected mouse accepts, for the app to build its controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub dpi: Vec<DpiRange>,
    pub threshold_min: u8,
    pub threshold_max: u8,
    pub buttons: Vec<ButtonTargets>,
    /// What `hid-lab reset` writes.
    pub defaults: Vec<SettingValue>,
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
    /// The threshold kept in the config file, also while SmartShift is off
    /// (`desired` then holds 255).
    pub saved_threshold: Option<u8>,
    /// `None` without a device or when the kernel lists no battery.
    pub battery: Option<Battery>,
    /// `None` without a device or when it could not be read.
    pub capabilities: Option<Capabilities>,
    /// GNOME's pointer settings.
    pub desktop: DesktopSettings,
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

/// Result of applying one setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemResult {
    /// Already at the target; nothing was written.
    Unchanged,
    /// Written and verified.
    Written,
    /// Written, but the read-back does not match.
    Mismatch,
    /// Refused before sending because the device cannot take this value
    /// (for example a DPI outside its list): see `error`.
    Rejected,
    /// Not applied: see `error`.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckJson {
    pub field: String,
    pub expected: String,
    pub actual: String,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyItem {
    pub setting: SettingKind,
    pub target: SettingValue,
    pub before: Option<SettingValue>,
    pub result: ItemResult,
    pub checks: Vec<CheckJson>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyReport {
    pub items: Vec<ApplyItem>,
    /// True when every item is `unchanged` or `written`.
    pub ok: bool,
    pub elapsed_ms: u64,
    /// Unrelated reports (mouse input, notifications) skipped while waiting.
    pub ignored_reports: u64,
}

/// Another program that may configure the mouse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub program: String,
    pub pid: u32,
}

impl Conflict {
    pub fn advice(&self) -> String {
        let stop = match self.program.as_str() {
            "logid" => "sudo systemctl stop logid",
            _ => "quit Solaar from its tray icon, or run `pkill solaar`",
        };
        format!(
            "{} (pid {}) is running and may also configure the mouse; settings can fight. \
             To stop it: {stop}",
            self.program, self.pid
        )
    }
}
