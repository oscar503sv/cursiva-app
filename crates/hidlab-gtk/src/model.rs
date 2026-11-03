//! What the window shows, computed from the daemon's JSON. Pure, so it is
//! tested without a display.

use gettextrs::gettext;
use hidlab_api::daemon::{Battery, Status};
use hidlab_api::settings::{SettingKind, SettingValue, THRESHOLD_OFF};

/// The window's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    /// Waiting for the first answer from the daemon.
    Connecting,
    /// The daemon could not be reached; the text says why.
    NoDaemon(String),
    /// The daemon runs but the mouse is not connected.
    NoMouse {
        config_error: Option<String>,
    },
    Connected(Summary),
}

/// How well the user's settings are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Good,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub name: String,
    pub connection: String,
    pub battery: Option<BatteryText>,
    pub health: Health,
    pub headline: String,
    /// Extra explanation under the headline, if any.
    pub detail: Option<String>,
    pub dpi: Option<String>,
    pub wheel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryText {
    /// For example `85 %`, or a level name when the percentage is unknown.
    pub value: String,
    pub status: String,
    /// Icon name from the icon theme.
    pub icon: String,
}

/// Replaces `{name}` in a translated template.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    args.iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

impl View {
    /// Title and text for a settings page that cannot be shown in this
    /// state.
    pub fn unavailable(&self) -> (String, String) {
        match self {
            View::Connecting => (gettext("Connecting to the Cursiva service…"), String::new()),
            View::NoDaemon(e) => (gettext("Cannot reach the Cursiva service"), e.clone()),
            _ => (
                gettext("Connect your mouse"),
                gettext(
                    "Turn the mouse on and connect it over Bluetooth. Cursiva finds it by itself.",
                ),
            ),
        }
    }

    pub fn from_status(status: &Status) -> View {
        let Some(device) = &status.device else {
            return View::NoMouse {
                config_error: status.config_error.clone(),
            };
        };
        let (health, headline, detail) = health(status);
        View::Connected(Summary {
            name: device.name.clone(),
            connection: connection(&device.bus),
            battery: status.battery.as_ref().map(battery_text),
            health,
            headline,
            detail,
            dpi: current(status, SettingKind::Dpi).map(|v| match v {
                // TRANSLATORS: {dpi} is a number such as 1600.
                SettingValue::Dpi(d) => fill(&gettext("{dpi} DPI"), &[("dpi", &d.to_string())]),
                other => other.display(),
            }),
            wheel: wheel_text(
                current(status, SettingKind::SmartshiftMode),
                current(status, SettingKind::SmartshiftThreshold),
            ),
        })
    }
}

fn current(status: &Status, kind: SettingKind) -> Option<SettingValue> {
    status
        .current
        .iter()
        .find(|c| c.setting == kind)
        .and_then(|c| c.value)
}

fn connection(bus: &str) -> String {
    match bus {
        "bluetooth" => gettext("Bluetooth"),
        "usb" => gettext("USB"),
        other => other.to_owned(),
    }
}

fn health(status: &Status) -> (Health, String, Option<String>) {
    if let Some(err) = &status.config_error {
        return (
            Health::Warning,
            gettext("Your settings file has an error"),
            Some(err.clone()),
        );
    }
    if let Some(c) = status.conflicts.first() {
        return (
            Health::Warning,
            gettext("Another program also configures the mouse"),
            Some(c.advice()),
        );
    }
    match &status.last_apply {
        Some(record) if !record.ok() => (
            Health::Warning,
            gettext("Some settings were not applied"),
            None,
        ),
        _ if status.desired.is_empty() => (
            Health::Good,
            gettext("The mouse uses its own settings"),
            None,
        ),
        _ => (Health::Good, gettext("Your settings are active"), None),
    }
}

pub fn battery_text(b: &Battery) -> BatteryText {
    let value = match (b.percent, b.level.as_deref()) {
        // TRANSLATORS: battery charge; {percent} is a number from 0 to 100.
        (Some(p), _) => fill(&gettext("{percent} %"), &[("percent", &p.to_string())]),
        (None, Some("Critical")) => gettext("Critical"),
        (None, Some("Low")) => gettext("Low"),
        (None, Some("Full")) => gettext("Full"),
        (None, Some(_)) => gettext("Normal"),
        (None, None) => gettext("Unknown"),
    };
    let charging = matches!(b.status.as_deref(), Some("Charging"));
    let status = match b.status.as_deref() {
        Some("Charging") => gettext("Charging"),
        Some("Full") => gettext("Fully charged"),
        _ => gettext("Not charging"),
    };
    let tenth = match b.percent {
        Some(p) => (u16::from(p.min(100)) + 5) / 10 * 10,
        None => match b.level.as_deref() {
            Some("Critical") => 0,
            Some("Low") => 20,
            Some("Full") => 100,
            _ => 50,
        },
    };
    let icon = if charging {
        format!("battery-level-{tenth}-charging-symbolic")
    } else {
        format!("battery-level-{tenth}-symbolic")
    };
    BatteryText {
        value,
        status,
        icon,
    }
}

/// Main-wheel behavior in words: SmartShift mode plus threshold.
pub fn wheel_text(mode: Option<SettingValue>, threshold: Option<SettingValue>) -> Option<String> {
    match (mode?, threshold) {
        (SettingValue::SmartshiftMode(1), _) => Some(gettext("Free spin")),
        (SettingValue::SmartshiftMode(2), Some(SettingValue::SmartshiftThreshold(t))) => {
            Some(match t {
                THRESHOLD_OFF => gettext("Clicks"),
                1..=16 => gettext("Automatic, switches with a light push"),
                17..=33 => gettext("Automatic, switches with a medium push"),
                _ => gettext("Automatic, switches with a hard push"),
            })
        }
        (SettingValue::SmartshiftMode(2), _) => Some(gettext("Clicks")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Status documents here are SYNTHETIC.
    use super::*;

    fn status(json: &str) -> Status {
        serde_json::from_str(json).unwrap()
    }

    const BASE: &str = r#"{
        "schema": "hid-lab.daemon-status/3",
        "daemon_version": "0.1.0",
        "config_file": "~/.config/hid-lab/config.toml",
        "config_error": null,
        "state_error": null,
        "device": DEVICE,
        "desired": DESIRED,
        "current": [
            {"setting": "dpi", "value": {"setting": "dpi", "value": 1600}, "error": null},
            {"setting": "smartshift-mode", "value": {"setting": "smartshift-mode", "value": 2}, "error": null},
            {"setting": "smartshift-threshold", "value": {"setting": "smartshift-threshold", "value": 30}, "error": null}
        ],
        "saved_threshold": null,
        "battery": {"percent": 84, "level": "Normal", "status": "Discharging"},
        "capabilities": null,
        "desktop": {"natural_scroll": null, "acceleration": null, "error": null},
        "last_apply": null,
        "conflicts": []
    }"#;

    const DEVICE: &str = r#"{
        "profile": "mx-master-3s", "name": "MX Master 3S", "node": "/dev/hidraw5",
        "hid_device": null, "bus": "bluetooth", "vendor_id": 1133, "product_id": 45108,
        "access": "ok"
    }"#;

    fn doc(device: &str, desired: &str) -> Status {
        status(&BASE.replace("DEVICE", device).replace("DESIRED", desired))
    }

    #[test]
    fn no_device_is_no_mouse() {
        assert_eq!(
            View::from_status(&doc("null", "[]")),
            View::NoMouse { config_error: None }
        );
    }

    #[test]
    fn connected_summary() {
        let View::Connected(s) =
            View::from_status(&doc(DEVICE, r#"[{"setting": "dpi", "value": 1600}]"#))
        else {
            panic!("not connected");
        };
        assert_eq!(s.name, "MX Master 3S");
        assert_eq!(s.connection, "Bluetooth");
        assert_eq!(s.health, Health::Good);
        assert_eq!(s.headline, "Your settings are active");
        assert_eq!(s.dpi.as_deref(), Some("1600 DPI"));
        assert_eq!(
            s.wheel.as_deref(),
            Some("Automatic, switches with a medium push")
        );
        let b = s.battery.unwrap();
        assert_eq!(
            (b.value.as_str(), b.status.as_str()),
            ("84 %", "Not charging")
        );
        assert_eq!(b.icon, "battery-level-80-symbolic");
    }

    #[test]
    fn empty_config_uses_the_mouse_settings() {
        let View::Connected(s) = View::from_status(&doc(DEVICE, "[]")) else {
            panic!("not connected");
        };
        assert_eq!(s.headline, "The mouse uses its own settings");
    }

    #[test]
    fn config_error_is_a_warning() {
        let mut st = doc(DEVICE, "[]");
        st.config_error = Some("line 3: unknown key `dpix`".into());
        let View::Connected(s) = View::from_status(&st) else {
            panic!("not connected");
        };
        assert_eq!(s.health, Health::Warning);
        assert_eq!(s.detail.as_deref(), Some("line 3: unknown key `dpix`"));
    }

    #[test]
    fn wheel_words() {
        let mode = |m| Some(SettingValue::SmartshiftMode(m));
        let thr = |t| Some(SettingValue::SmartshiftThreshold(t));
        assert_eq!(wheel_text(mode(1), thr(10)).as_deref(), Some("Free spin"));
        assert_eq!(wheel_text(mode(2), thr(255)).as_deref(), Some("Clicks"));
        assert_eq!(
            wheel_text(mode(2), thr(5)).as_deref(),
            Some("Automatic, switches with a light push")
        );
        assert_eq!(
            wheel_text(mode(2), thr(50)).as_deref(),
            Some("Automatic, switches with a hard push")
        );
        assert_eq!(wheel_text(None, thr(5)), None);
    }

    #[test]
    fn battery_without_percent_uses_the_level() {
        let b = battery_text(&Battery {
            percent: None,
            level: Some("Low".into()),
            status: Some("Charging".into()),
        });
        assert_eq!(b.value, "Low");
        assert_eq!(b.status, "Charging");
        assert_eq!(b.icon, "battery-level-20-charging-symbolic");
    }
}
