//! `~/.config/hid-lab/config.toml` (requirements §15, docs/config-format.md).
//!
//! Every field is optional: a setting that is not in the file is never
//! written. Only settings with a confirmed write function exist here.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use hidlab_devices::{DeviceProfile, profile_by_key};
use hidlab_features::reprog_controls::{self, RemapControl};
use hidlab_features::settings::{SettingKind, SettingValue, THRESHOLD_OFF};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FORMAT_VERSION: u32 = 1;

/// Threshold range accepted by Solaar's `SmartShift` setting (R-2). Turning
/// automatic switching off is `auto = false`, which sends 255 (P-035).
pub const THRESHOLD_RANGE: std::ops::RangeInclusive<u8> = 1..=50;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("cannot read {path}: {reason}")]
    Io { path: String, reason: String },
    #[error("{path} is not valid TOML or has an unknown key:\n{reason}")]
    Parse { path: String, reason: String },
    #[error("{path}: {reason}")]
    Invalid { path: String, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SmartShiftMode {
    Ratchet,
    FreeSpin,
}

impl SmartShiftMode {
    /// Wire code of 0x2110 (P-026): 2 ratchet, 1 free spin.
    pub fn code(self) -> u8 {
        match self {
            SmartShiftMode::Ratchet => 2,
            SmartShiftMode::FreeSpin => 1,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            2 => Some(SmartShiftMode::Ratchet),
            1 => Some(SmartShiftMode::FreeSpin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmartShiftConfig {
    /// Mode used only until the daemon has seen the user choose one
    /// (requirements, Milestone 4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<SmartShiftMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u8>,
    /// `false` keeps the wheel ratcheted in ratchet mode: the daemon sends
    /// threshold 255 and keeps `threshold` for when it is turned back on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
}

/// Value in `[device."<key>".buttons]`: `default` or a target name.
pub const BUTTON_DEFAULT: &str = "default";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThumbwheelConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invert: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smartshift: Option<SmartShiftConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbwheel: Option<ThumbwheelConfig>,
    /// What each button does, by control name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub buttons: BTreeMap<RemapControl, String>,
}

fn button_target(control: RemapControl, value: &str) -> Option<u16> {
    if value == BUTTON_DEFAULT {
        Some(control.cid())
    } else {
        reprog_controls::target_cid(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub format_version: u32,
    #[serde(rename = "device", default, skip_serializing_if = "BTreeMap::is_empty")]
    pub devices: BTreeMap<String, DeviceConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            devices: BTreeMap::new(),
        }
    }
}

impl DeviceConfig {
    /// Settings to apply, except the SmartShift mode, which comes from
    /// [`crate::state`] when the daemon has one.
    pub fn values(&self) -> Vec<SettingValue> {
        let mut out = Vec::new();
        if let Some(d) = self.dpi {
            out.push(SettingValue::Dpi(d));
        }
        if self.smartshift_auto() == Some(false) {
            out.push(SettingValue::SmartshiftThreshold(THRESHOLD_OFF));
        } else if let Some(t) = self.saved_threshold() {
            out.push(SettingValue::SmartshiftThreshold(t));
        }
        if let Some(i) = self.thumbwheel.as_ref().and_then(|t| t.invert) {
            out.push(SettingValue::ThumbwheelInvert(i));
        }
        for c in RemapControl::ALL {
            if let Some(t) = self.buttons.get(&c).and_then(|v| button_target(c, v)) {
                out.push(SettingValue::ButtonRemap(c, t));
            }
        }
        out
    }

    /// The threshold in the file, also while automatic switching is off.
    pub fn saved_threshold(&self) -> Option<u8> {
        self.smartshift.as_ref().and_then(|s| s.threshold)
    }

    pub fn smartshift_auto(&self) -> Option<bool> {
        self.smartshift.as_ref().and_then(|s| s.auto)
    }

    pub fn initial_mode(&self) -> Option<SmartShiftMode> {
        self.smartshift.as_ref().and_then(|s| s.mode)
    }

    /// Stores `v` (any setting except the mode, which belongs to the state,
    /// and values the daemon may not send, which callers refuse first).
    /// Threshold `off` (255) sets `auto = false` and keeps the threshold;
    /// any other threshold turns automatic switching back on.
    pub fn set(&mut self, v: SettingValue) {
        match v {
            SettingValue::Dpi(d) => self.dpi = Some(d),
            SettingValue::SmartshiftThreshold(THRESHOLD_OFF) => {
                self.smartshift.get_or_insert_with(Default::default).auto = Some(false);
            }
            SettingValue::SmartshiftThreshold(t) => {
                let s = self.smartshift.get_or_insert_with(Default::default);
                s.threshold = Some(t);
                s.auto = None;
            }
            SettingValue::ThumbwheelInvert(i) => {
                self.thumbwheel.get_or_insert_with(Default::default).invert = Some(i);
            }
            SettingValue::ButtonRemap(c, t) => {
                let name = if t == c.cid() {
                    Some(BUTTON_DEFAULT)
                } else {
                    reprog_controls::target_name(t)
                };
                if let Some(n) = name {
                    self.buttons.insert(c, n.to_owned());
                }
            }
            SettingValue::SmartshiftMode(_) => {}
        }
    }

    /// Removes `kind` from the file, so the daemon stops applying it. For
    /// the SmartShift mode this removes the initial mode.
    pub fn remove(&mut self, kind: SettingKind) {
        match kind {
            SettingKind::Dpi => self.dpi = None,
            SettingKind::SmartshiftMode => {
                if let Some(s) = &mut self.smartshift {
                    s.mode = None;
                }
            }
            SettingKind::SmartshiftThreshold => {
                if let Some(s) = &mut self.smartshift {
                    s.threshold = None;
                    s.auto = None;
                }
            }
            SettingKind::ThumbwheelInvert => self.thumbwheel = None,
            SettingKind::ButtonRemap(c) => {
                self.buttons.remove(&c);
            }
        }
        if self.smartshift.as_ref() == Some(&SmartShiftConfig::default()) {
            self.smartshift = None;
        }
    }
}

impl Config {
    /// Parses and validates `text`; `path` is only used in messages.
    pub fn parse(text: &str, path: &str) -> Result<Self, ConfigError> {
        let cfg: Config = toml::from_str(text).map_err(|e| ConfigError::Parse {
            path: path.to_owned(),
            reason: e.to_string(),
        })?;
        cfg.validate(path)?;
        Ok(cfg)
    }

    fn validate(&self, path: &str) -> Result<(), ConfigError> {
        let invalid = |reason: String| ConfigError::Invalid {
            path: path.to_owned(),
            reason,
        };
        if self.format_version != FORMAT_VERSION {
            return Err(invalid(format!(
                "format_version {} is not supported; this hid-lab reads format_version = {FORMAT_VERSION}",
                self.format_version
            )));
        }
        for (key, dev) in &self.devices {
            if profile_by_key(key).is_none() {
                let known: Vec<&str> = hidlab_devices::PROFILES.iter().map(|p| p.key).collect();
                return Err(invalid(format!(
                    "unknown device \"{key}\"; known devices: {}",
                    known.join(", ")
                )));
            }
            if dev.dpi == Some(0) {
                return Err(invalid(format!("device \"{key}\": dpi must be positive")));
            }
            match dev.smartshift.as_ref().and_then(|s| s.threshold) {
                Some(t) if !THRESHOLD_RANGE.contains(&t) => {
                    return Err(invalid(format!(
                        "device \"{key}\": smartshift.threshold must be 1..=50, got {t}"
                    )));
                }
                _ => {}
            }
            for (c, v) in &dev.buttons {
                if button_target(*c, v).is_none() {
                    let names: Vec<&str> = reprog_controls::TARGET_NAMES
                        .iter()
                        .map(|(n, _)| *n)
                        .collect();
                    return Err(invalid(format!(
                        "device \"{key}\": buttons.{} = \"{v}\" is not `{BUTTON_DEFAULT}` or one of {}",
                        c.name(),
                        names.join(", ")
                    )));
                }
            }
        }
        Ok(())
    }

    /// Reads `path`. A missing file is an empty configuration.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let shown = crate::paths::tilde(path);
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, &shown),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ConfigError::Io {
                path: shown,
                reason: e.to_string(),
            }),
        }
    }

    /// Writes the file atomically. Comments in the old file are not kept.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let body = toml::to_string(self).map_err(|e| e.to_string())?;
        let text = format!(
            "# hid-lab configuration. See docs/config-format.md.\n\
             # Rewritten by hid-labd on `hid-lab set`; comments are not kept.\n\n{body}"
        );
        crate::paths::write_atomic(path, &text)
    }

    pub fn device(&self, profile: &DeviceProfile) -> Option<&DeviceConfig> {
        self.devices.get(profile.key)
    }

    pub fn device_mut(&mut self, profile: &DeviceProfile) -> &mut DeviceConfig {
        self.devices.entry(profile.key.to_owned()).or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"
format_version = 1

[device."mx-master-3s"]
dpi = 1600
smartshift = { mode = "free-spin", threshold = 30 }
thumbwheel = { invert = true }

[device."mx-master-3s".buttons]
gesture = "middle"
back = "default"
"#;

    #[test]
    fn parses_every_field() {
        let c = Config::parse(FULL, "t").unwrap();
        let d = &c.devices["mx-master-3s"];
        assert_eq!(d.initial_mode(), Some(SmartShiftMode::FreeSpin));
        assert_eq!(
            d.values(),
            [
                SettingValue::Dpi(1600),
                SettingValue::SmartshiftThreshold(30),
                SettingValue::ThumbwheelInvert(true),
                SettingValue::ButtonRemap(RemapControl::Back, 0x53),
                SettingValue::ButtonRemap(RemapControl::Gesture, 0x52),
            ]
        );
    }

    #[test]
    fn auto_off_sends_255_and_keeps_the_threshold() {
        let p = &hidlab_devices::MX_MASTER_3S;
        let mut c = Config::default();
        c.device_mut(p).set(SettingValue::SmartshiftThreshold(30));
        c.device_mut(p)
            .set(SettingValue::SmartshiftThreshold(THRESHOLD_OFF));
        let d = c.device(p).unwrap();
        assert_eq!(d.values(), [SettingValue::SmartshiftThreshold(255)]);
        assert_eq!(
            (d.saved_threshold(), d.smartshift_auto()),
            (Some(30), Some(false))
        );
        let body = toml::to_string(&c).unwrap();
        assert!(body.contains("auto = false"), "{body}");
        assert_eq!(Config::parse(&body, "t").unwrap(), c);
        // A threshold turns it back on.
        c.device_mut(p).set(SettingValue::SmartshiftThreshold(12));
        assert_eq!(c.device(p).unwrap().smartshift_auto(), None);
        assert_eq!(
            c.device(p).unwrap().values(),
            [SettingValue::SmartshiftThreshold(12)]
        );
    }

    #[test]
    fn remove_forgets_a_setting() {
        let p = &hidlab_devices::MX_MASTER_3S;
        let mut c = Config::parse(FULL, "t").unwrap();
        let d = c.device_mut(p);
        d.remove(SettingKind::SmartshiftMode);
        d.remove(SettingKind::SmartshiftThreshold);
        d.remove(SettingKind::ButtonRemap(RemapControl::Gesture));
        assert_eq!(d.smartshift, None);
        assert_eq!(
            d.values(),
            [
                SettingValue::Dpi(1600),
                SettingValue::ThumbwheelInvert(true),
                SettingValue::ButtonRemap(RemapControl::Back, 0x53),
            ]
        );
        d.set(SettingValue::ButtonRemap(RemapControl::Forward, 0x50));
        assert_eq!(d.buttons[&RemapControl::Forward], "left");
    }

    #[test]
    fn empty_and_partial_files_are_valid() {
        let c = Config::parse("format_version = 1\n", "t").unwrap();
        assert!(c.devices.is_empty());
        let c = Config::parse(
            "format_version = 1\n[device.\"mx-master-3s\"]\ndpi = 800\n",
            "t",
        )
        .unwrap();
        assert_eq!(c.devices["mx-master-3s"].values(), [SettingValue::Dpi(800)]);
    }

    #[test]
    fn rejects_mistakes_with_a_message() {
        let cases = [
            ("", "format_version"),
            ("format_version = 2\n", "format_version 2"),
            (
                "format_version = 1\n[device.\"mx\"]\n",
                "unknown device \"mx\"",
            ),
            (
                "format_version = 1\n[device.\"mx-master-3s\"]\ndpy = 800\n",
                "dpy",
            ),
            (
                "format_version = 1\n[device.\"mx-master-3s\"]\nsmartshift = { threshold = 0 }\n",
                "1..=50",
            ),
            (
                "format_version = 1\n[device.\"mx-master-3s\"]\nsmartshift = { mode = \"auto\" }\n",
                "auto",
            ),
            (
                "format_version = 1\n[device.\"mx-master-3s\"]\ndpi = 0\n",
                "positive",
            ),
            (
                "format_version = 1\n[device.\"mx-master-3s\".buttons]\nback = \"jump\"\n",
                "buttons.back = \"jump\"",
            ),
            (
                "format_version = 1\n[device.\"mx-master-3s\".buttons]\nleft = \"back\"\n",
                "left",
            ),
        ];
        for (text, needle) in cases {
            let err = Config::parse(text, "cfg").unwrap_err().to_string();
            assert!(err.contains(needle), "{text:?} -> {err}");
        }
    }

    #[test]
    fn set_then_save_roundtrips() {
        let mut c = Config::default();
        let p = &hidlab_devices::MX_MASTER_3S;
        c.device_mut(p).set(SettingValue::Dpi(1200));
        c.device_mut(p).set(SettingValue::SmartshiftThreshold(12));
        let body = toml::to_string(&c).unwrap();
        assert_eq!(Config::parse(&body, "t").unwrap(), c);
    }
}
