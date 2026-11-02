//! The settings vocabulary shared by the daemon, the CLI and the app:
//! names, values and how they are written in JSON and on the command line.
//! HID++ details (which feature and function write each setting) live in
//! `hidlab-features`.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::remap::{self, RemapControl};

/// SmartShift threshold that keeps the wheel ratcheted (Solaar `SmartShift`).
pub const THRESHOLD_OFF: u8 = 255;

/// A setting hid-lab can change. Serialized as its name, e.g. `dpi` or
/// `remap-back`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Dpi,
    SmartshiftMode,
    SmartshiftThreshold,
    ThumbwheelInvert,
    /// What one control does (0x1B04 remap).
    ButtonRemap(RemapControl),
}

impl Serialize for SettingKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for SettingKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        SettingKind::from_name(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown setting `{name}`")))
    }
}

impl SettingKind {
    pub const ALL: [SettingKind; 9] = [
        SettingKind::Dpi,
        SettingKind::SmartshiftMode,
        SettingKind::SmartshiftThreshold,
        SettingKind::ThumbwheelInvert,
        SettingKind::ButtonRemap(RemapControl::Middle),
        SettingKind::ButtonRemap(RemapControl::Back),
        SettingKind::ButtonRemap(RemapControl::Forward),
        SettingKind::ButtonRemap(RemapControl::Gesture),
        SettingKind::ButtonRemap(RemapControl::ModeButton),
    ];

    pub fn name(self) -> &'static str {
        match self {
            SettingKind::Dpi => "dpi",
            SettingKind::SmartshiftMode => "smartshift-mode",
            SettingKind::SmartshiftThreshold => "smartshift-threshold",
            SettingKind::ThumbwheelInvert => "thumbwheel-invert",
            SettingKind::ButtonRemap(c) => match c {
                RemapControl::Middle => "remap-middle",
                RemapControl::Back => "remap-back",
                RemapControl::Forward => "remap-forward",
                RemapControl::Gesture => "remap-gesture",
                RemapControl::ModeButton => "remap-mode",
            },
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Parses a user-supplied value for this setting.
    pub fn parse_value(self, v: &str) -> Result<SettingValue, String> {
        match self {
            SettingKind::Dpi => v
                .parse::<u16>()
                .ok()
                .filter(|&d| d > 0)
                .map(SettingValue::Dpi)
                .ok_or_else(|| format!("DPI must be a positive number, got `{v}`")),
            SettingKind::SmartshiftMode => match v {
                "ratchet" => Ok(SettingValue::SmartshiftMode(2)),
                "free-spin" => Ok(SettingValue::SmartshiftMode(1)),
                _ => Err(format!("mode must be `ratchet` or `free-spin`, got `{v}`")),
            },
            SettingKind::SmartshiftThreshold if v == "off" => {
                Ok(SettingValue::SmartshiftThreshold(THRESHOLD_OFF))
            }
            SettingKind::SmartshiftThreshold => v
                .parse::<u8>()
                .ok()
                .filter(|t| (1..=50).contains(t))
                .map(SettingValue::SmartshiftThreshold)
                .ok_or_else(|| format!("threshold must be 1..=50 or `off`, got `{v}`")),
            SettingKind::ThumbwheelInvert => match v {
                "on" => Ok(SettingValue::ThumbwheelInvert(true)),
                "off" => Ok(SettingValue::ThumbwheelInvert(false)),
                _ => Err(format!("value must be `on` or `off`, got `{v}`")),
            },
            SettingKind::ButtonRemap(c) if v == "default" => {
                Ok(SettingValue::ButtonRemap(c, c.cid()))
            }
            SettingKind::ButtonRemap(c) => remap::target_cid(v)
                .map(|t| SettingValue::ButtonRemap(c, t))
                .ok_or_else(|| {
                    let names: Vec<&str> = remap::TARGET_NAMES.iter().map(|(n, _)| *n).collect();
                    format!(
                        "target must be `default` or one of {}, got `{v}`",
                        names.join(", ")
                    )
                }),
        }
    }
}

/// A value of one setting. SmartShift mode uses the wire codes 1 and 2.
///
/// Serialized as `{"setting": <name>, "value": <value>}` with the names of
/// [`SettingKind::name`]; a remap's value is the target name, e.g.
/// `{"setting": "remap-back", "value": "middle"}` (or `"0x00NN"` for a
/// control without a name). Reading also accepts the earlier remap form
/// `{"setting": "button-remap", "value": ["back", 82]}` (phase 5b files).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingValue {
    Dpi(u16),
    SmartshiftMode(u8),
    SmartshiftThreshold(u8),
    ThumbwheelInvert(bool),
    /// Control and the control ID whose native task it performs.
    ButtonRemap(RemapControl, u16),
}

#[derive(Serialize)]
struct ValueOut<T: Serialize> {
    setting: &'static str,
    value: T,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawValue {
    Bool(bool),
    Number(u16),
    Text(String),
    LegacyRemap(RemapControl, u16),
}

#[derive(Deserialize)]
struct ValueIn {
    setting: String,
    value: RawValue,
}

impl Serialize for SettingValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let setting = self.kind().name();
        match *self {
            SettingValue::Dpi(v) => ValueOut { setting, value: v }.serialize(s),
            SettingValue::SmartshiftMode(v) | SettingValue::SmartshiftThreshold(v) => {
                ValueOut { setting, value: v }.serialize(s)
            }
            SettingValue::ThumbwheelInvert(v) => ValueOut { setting, value: v }.serialize(s),
            SettingValue::ButtonRemap(_, t) => ValueOut {
                setting,
                value: remap::target_name(t).map_or_else(|| format!("0x{t:04X}"), str::to_owned),
            }
            .serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for SettingValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let raw = ValueIn::deserialize(d)?;
        let bad = || D::Error::custom(format!("invalid value for `{}`", raw.setting));
        let byte = |n: u16| u8::try_from(n).map_err(|_| bad());
        if raw.setting == "button-remap" {
            return match raw.value {
                RawValue::LegacyRemap(c, t) => Ok(SettingValue::ButtonRemap(c, t)),
                _ => Err(bad()),
            };
        }
        let kind = SettingKind::from_name(&raw.setting)
            .ok_or_else(|| D::Error::custom(format!("unknown setting `{}`", raw.setting)))?;
        match (kind, &raw.value) {
            (SettingKind::Dpi, RawValue::Number(n)) => Ok(SettingValue::Dpi(*n)),
            (SettingKind::SmartshiftMode, RawValue::Number(n)) => {
                Ok(SettingValue::SmartshiftMode(byte(*n)?))
            }
            (SettingKind::SmartshiftThreshold, RawValue::Number(n)) => {
                Ok(SettingValue::SmartshiftThreshold(byte(*n)?))
            }
            (SettingKind::ThumbwheelInvert, RawValue::Bool(b)) => {
                Ok(SettingValue::ThumbwheelInvert(*b))
            }
            (SettingKind::ButtonRemap(c), RawValue::Text(t)) => {
                let cid = remap::target_cid(t)
                    .or_else(|| {
                        t.strip_prefix("0x")
                            .and_then(|h| u16::from_str_radix(h, 16).ok())
                    })
                    .ok_or_else(bad)?;
                Ok(SettingValue::ButtonRemap(c, cid))
            }
            _ => Err(bad()),
        }
    }
}

impl SettingValue {
    pub fn kind(self) -> SettingKind {
        match self {
            SettingValue::Dpi(_) => SettingKind::Dpi,
            SettingValue::SmartshiftMode(_) => SettingKind::SmartshiftMode,
            SettingValue::SmartshiftThreshold(_) => SettingKind::SmartshiftThreshold,
            SettingValue::ThumbwheelInvert(_) => SettingKind::ThumbwheelInvert,
            SettingValue::ButtonRemap(c, _) => SettingKind::ButtonRemap(c),
        }
    }

    /// True when the daemon may send this value: its write function and
    /// this kind of value were confirmed on the mouse (requirements §13).
    /// A value added by a new write experiment returns false here until its
    /// hardware test passes; today every value is confirmed (H4, H5).
    pub fn confirmed(self) -> bool {
        true
    }

    pub fn display(self) -> String {
        match self {
            SettingValue::Dpi(d) => d.to_string(),
            SettingValue::SmartshiftMode(1) => "free-spin".into(),
            SettingValue::SmartshiftMode(2) => "ratchet".into(),
            SettingValue::SmartshiftMode(m) => format!("mode {m}"),
            SettingValue::SmartshiftThreshold(THRESHOLD_OFF) => "off".into(),
            SettingValue::SmartshiftThreshold(t) => t.to_string(),
            SettingValue::ThumbwheelInvert(true) => "on".into(),
            SettingValue::ThumbwheelInvert(false) => "off".into(),
            SettingValue::ButtonRemap(c, t) if t == c.cid() => format!("{} (default)", c.name()),
            SettingValue::ButtonRemap(_, t) => {
                remap::target_name(t).map_or_else(|| format!("0x{t:04X}"), str::to_owned)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_roundtrip_and_parse() {
        for k in SettingKind::ALL {
            assert_eq!(SettingKind::from_name(k.name()), Some(k));
        }
        assert_eq!(
            SettingKind::SmartshiftMode.parse_value("free-spin"),
            Ok(SettingValue::SmartshiftMode(1))
        );
        assert!(SettingKind::SmartshiftThreshold.parse_value("51").is_err());
        assert!(SettingKind::SmartshiftThreshold.parse_value("0").is_err());
        assert!(SettingKind::Dpi.parse_value("0").is_err());
        assert!(SettingKind::ThumbwheelInvert.parse_value("yes").is_err());
        assert_eq!(
            SettingKind::SmartshiftThreshold.parse_value("off"),
            Ok(SettingValue::SmartshiftThreshold(THRESHOLD_OFF))
        );
        let back = SettingKind::ButtonRemap(RemapControl::Back);
        assert_eq!(back.name(), "remap-back");
        assert_eq!(
            back.parse_value("middle"),
            Ok(SettingValue::ButtonRemap(RemapControl::Back, 0x52))
        );
        assert_eq!(
            back.parse_value("default"),
            Ok(SettingValue::ButtonRemap(RemapControl::Back, 0x53))
        );
        assert!(back.parse_value("bogus").is_err());
        assert_eq!(serde_json::to_string(&back).unwrap(), "\"remap-back\"");
        assert_eq!(
            serde_json::from_str::<SettingKind>("\"dpi\"").unwrap(),
            SettingKind::Dpi
        );
    }

    #[test]
    fn values_serialize_with_setting_names() {
        let cases = [
            (SettingValue::Dpi(1600), r#"{"setting":"dpi","value":1600}"#),
            (
                SettingValue::SmartshiftMode(2),
                r#"{"setting":"smartshift-mode","value":2}"#,
            ),
            (
                SettingValue::ThumbwheelInvert(true),
                r#"{"setting":"thumbwheel-invert","value":true}"#,
            ),
            (
                SettingValue::ButtonRemap(RemapControl::Back, 0x52),
                r#"{"setting":"remap-back","value":"middle"}"#,
            ),
            (
                SettingValue::ButtonRemap(RemapControl::Back, 0xD7),
                r#"{"setting":"remap-back","value":"0x00D7"}"#,
            ),
        ];
        for (v, json) in cases {
            assert_eq!(serde_json::to_string(&v).unwrap(), json);
            assert_eq!(serde_json::from_str::<SettingValue>(json).unwrap(), v);
        }
        // Phase 5b files (docs/protocol/evidence/m5b) still read.
        assert_eq!(
            serde_json::from_str::<SettingValue>(
                r#"{"setting":"button-remap","value":["back",82]}"#
            )
            .unwrap(),
            SettingValue::ButtonRemap(RemapControl::Back, 0x52)
        );
        assert!(serde_json::from_str::<SettingValue>(r#"{"setting":"dpi","value":"x"}"#).is_err());
        assert!(
            serde_json::from_str::<SettingValue>(r#"{"setting":"smartshift-mode","value":300}"#)
                .is_err()
        );
    }

    #[test]
    fn values_confirmed_by_h5_reach_the_daemon() {
        assert!(SettingValue::SmartshiftThreshold(THRESHOLD_OFF).confirmed());
        assert!(SettingValue::ButtonRemap(RemapControl::Back, 0x52).confirmed());
    }
}
