//! Writable settings, shared by `hid-lab lab write` and the daemon.
//!
//! Each setting knows how to read its current value, build the write bytes
//! (from Solaar's documented behavior, CONFIRMED by the hardware tests H4,
//! P-030 to P-032), and verify the result by reading back the written field
//! and every field that must not change.
//!
//! Sources (R-2, Solaar at the pinned commit):
//! - DPI: `AdjustableDpi` writes function 0x30 with prefix `00` and the
//!   value as 2 bytes big-endian.
//! - SmartShift: `ScrollRatchet` writes function 0x10 with `[mode]`;
//!   `SmartShift` writes `[0, threshold]`. A 0 in the other field means
//!   "no change" (CONFIRMED, P-031).
//! - Thumb wheel: `ThumbInvert` writes function 0x20 with byte 1 bit 0 =
//!   invert, keeping the other bits of the current value.
//!
//! Values the daemon may not send yet (see [`SettingValue::confirmed`]):
//! - SmartShift threshold `off` (255): Solaar's `SmartShift` writes 255 for
//!   "always ratcheted" (its setting caps at 50 and maps 50 to 255).
//! - Button remapping with 0x1B04 setCidReporting (see `reprog_controls`).

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::reader::{ReadError, Reader, WriteAuth};
use crate::reprog_controls::{self, RemapControl};
use crate::{dpi, smartshift, thumbwheel};

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
    /// What one control does (0x1B04 remap). `hid-lab lab write` only.
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
    pub const ALL: [SettingKind; 4] = [
        SettingKind::Dpi,
        SettingKind::SmartshiftMode,
        SettingKind::SmartshiftThreshold,
        SettingKind::ThumbwheelInvert,
    ];

    /// Settings only `hid-lab lab write` accepts, until their writes are
    /// confirmed on the mouse.
    pub const EXPERIMENTAL: [SettingKind; 5] = [
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
        Self::ALL
            .into_iter()
            .chain(Self::EXPERIMENTAL)
            .find(|k| k.name() == name)
    }

    pub fn feature(self) -> FeatureId {
        match self {
            SettingKind::Dpi => dpi::FEATURE,
            SettingKind::SmartshiftMode | SettingKind::SmartshiftThreshold => smartshift::FEATURE,
            SettingKind::ThumbwheelInvert => thumbwheel::FEATURE,
            SettingKind::ButtonRemap(_) => reprog_controls::FEATURE,
        }
    }

    pub fn write_function(self) -> u8 {
        match self {
            SettingKind::Dpi => 3,
            SettingKind::SmartshiftMode | SettingKind::SmartshiftThreshold => 1,
            SettingKind::ThumbwheelInvert => 2,
            SettingKind::ButtonRemap(_) => 3,
        }
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
            SettingKind::ButtonRemap(c) => reprog_controls::target_cid(v)
                .map(|t| SettingValue::ButtonRemap(c, t))
                .ok_or_else(|| {
                    let names: Vec<&str> = reprog_controls::TARGET_NAMES
                        .iter()
                        .map(|(n, _)| *n)
                        .collect();
                    format!(
                        "target must be `default` or one of {}, got `{v}`",
                        names.join(", ")
                    )
                }),
        }
    }
}

/// A value of one setting. SmartShift mode uses the wire codes 1 and 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "setting", content = "value", rename_all = "kebab-case")]
pub enum SettingValue {
    Dpi(u16),
    SmartshiftMode(u8),
    SmartshiftThreshold(u8),
    ThumbwheelInvert(bool),
    /// Control and the control ID whose native task it performs.
    ButtonRemap(RemapControl, u16),
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
    /// Everything else is for `hid-lab lab write` only.
    pub fn confirmed(self) -> bool {
        match self {
            SettingValue::SmartshiftThreshold(t) => t != THRESHOLD_OFF,
            SettingValue::ButtonRemap(..) => false,
            _ => true,
        }
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
                reprog_controls::target_name(t).map_or_else(|| format!("0x{t:04X}"), str::to_owned)
            }
        }
    }
}

/// Everything read from the setting's feature, used to plan and verify.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "feature", rename_all = "kebab-case")]
pub enum Snapshot {
    Dpi(dpi::Dpi),
    Smartshift(smartshift::SmartShift),
    Thumbwheel(thumbwheel::Thumbwheel),
    Remap(reprog_controls::RemapState),
}

impl Snapshot {
    /// The current value of `kind` in this snapshot.
    pub fn value(&self, kind: SettingKind) -> Option<SettingValue> {
        match (self, kind) {
            (Snapshot::Dpi(d), SettingKind::Dpi) => {
                Some(SettingValue::Dpi(d.current.effective_dpi))
            }
            (Snapshot::Smartshift(s), SettingKind::SmartshiftMode) => {
                Some(SettingValue::SmartshiftMode(s.mode_code))
            }
            (Snapshot::Smartshift(s), SettingKind::SmartshiftThreshold) => {
                Some(SettingValue::SmartshiftThreshold(s.threshold))
            }
            (Snapshot::Thumbwheel(t), SettingKind::ThumbwheelInvert) => {
                Some(SettingValue::ThumbwheelInvert(t.inverted))
            }
            (Snapshot::Remap(s), SettingKind::ButtonRemap(c)) if s.control == c.cid() => {
                Some(SettingValue::ButtonRemap(c, s.reporting.mapped_to))
            }
            _ => None,
        }
    }
}

/// Reads the feature that holds `kind`.
pub fn snapshot<C: RawChannel>(
    r: &mut Reader<'_, C>,
    kind: SettingKind,
) -> Result<Snapshot, ReadError> {
    Ok(match kind {
        SettingKind::Dpi => Snapshot::Dpi(dpi::read(r)?),
        SettingKind::SmartshiftMode | SettingKind::SmartshiftThreshold => {
            Snapshot::Smartshift(smartshift::read(r)?)
        }
        SettingKind::ThumbwheelInvert => Snapshot::Thumbwheel(thumbwheel::read(r)?),
        SettingKind::ButtonRemap(c) => Snapshot::Remap(reprog_controls::read_remap(r, c.cid())?),
    })
}

/// Bytes to send for one write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WritePlan {
    pub target: SettingValue,
    pub feature: FeatureId,
    pub function: u8,
    pub params: Vec<u8>,
    pub params_hex: String,
}

/// Validates `target` against the device's current state and builds the bytes.
pub fn plan(before: &Snapshot, target: SettingValue) -> Result<WritePlan, String> {
    let kind = target.kind();
    let params = match (before, target) {
        (Snapshot::Dpi(d), SettingValue::Dpi(v)) => {
            if !d.list.contains(v) {
                return Err(format!(
                    "{v} DPI is not in the list the mouse reports; run `hid-lab lab read` to see it"
                ));
            }
            let [hi, lo] = v.to_be_bytes();
            vec![d.current.sensor, hi, lo]
        }
        (Snapshot::Smartshift(_), SettingValue::SmartshiftMode(m)) => vec![m, 0x00],
        (Snapshot::Smartshift(_), SettingValue::SmartshiftThreshold(t)) => vec![0x00, t],
        (Snapshot::Thumbwheel(t), SettingValue::ThumbwheelInvert(inv)) => {
            let raw = raw_thumb(t);
            vec![raw[0], (raw[1] & !0x01) | u8::from(inv)]
        }
        (Snapshot::Remap(s), SettingValue::ButtonRemap(c, to)) if s.control == c.cid() => {
            if s.targets.is_empty() {
                return Err(format!("the mouse does not allow remapping {}", c.name()));
            }
            if !s.targets.contains(&to) {
                let allowed: Vec<String> = s
                    .targets
                    .iter()
                    .map(|&t| {
                        reprog_controls::target_name(t)
                            .map_or_else(|| format!("0x{t:04X}"), str::to_owned)
                    })
                    .collect();
                return Err(format!(
                    "the mouse does not allow {} -> 0x{to:04X}; allowed: {}",
                    c.name(),
                    allowed.join(", ")
                ));
            }
            // Flags byte 0: no reporting flag changes, so nothing is diverted.
            let [ch, cl] = c.cid().to_be_bytes();
            let [th, tl] = to.to_be_bytes();
            vec![ch, cl, 0x00, th, tl]
        }
        _ => return Err(format!("snapshot does not hold {}", kind.name())),
    };
    Ok(WritePlan {
        target,
        feature: kind.feature(),
        function: kind.write_function(),
        params_hex: hidlab_hidpp::hex(&params),
        params,
    })
}

fn raw_thumb(t: &thumbwheel::Thumbwheel) -> [u8; 2] {
    let mut out = [0u8; 2];
    for (i, b) in t.raw_hex.split(' ').take(2).enumerate() {
        out[i] = u8::from_str_radix(b, 16).unwrap_or(0);
    }
    out
}

/// One field compared after the write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldCheck {
    pub field: &'static str,
    pub expected: String,
    pub actual: String,
    pub ok: bool,
}

fn field(field: &'static str, expected: impl ToString, actual: impl ToString) -> FieldCheck {
    let (expected, actual) = (expected.to_string(), actual.to_string());
    FieldCheck {
        ok: expected == actual,
        field,
        expected,
        actual,
    }
}

/// Compares the read-back with the target and with fields that must not change.
pub fn verify(before: &Snapshot, after: &Snapshot, target: SettingValue) -> Vec<FieldCheck> {
    match (before, after, target) {
        (Snapshot::Dpi(b), Snapshot::Dpi(a), SettingValue::Dpi(v)) => vec![
            field("dpi", v, a.current.effective_dpi),
            field(
                "default_dpi (unchanged)",
                b.current.default_dpi,
                a.current.default_dpi,
            ),
        ],
        (Snapshot::Smartshift(b), Snapshot::Smartshift(a), SettingValue::SmartshiftMode(m)) => {
            vec![
                field("mode", m, a.mode_code),
                field("threshold (unchanged)", b.threshold, a.threshold),
            ]
        }
        (
            Snapshot::Smartshift(b),
            Snapshot::Smartshift(a),
            SettingValue::SmartshiftThreshold(t),
        ) => {
            vec![
                field("threshold", t, a.threshold),
                field("mode (unchanged)", b.mode_code, a.mode_code),
            ]
        }
        (Snapshot::Thumbwheel(b), Snapshot::Thumbwheel(a), SettingValue::ThumbwheelInvert(i)) => {
            vec![
                field("inverted", i, a.inverted),
                field("diverted (unchanged)", b.diverted, a.diverted),
                field(
                    "other bits (unchanged)",
                    b.unknown_bits.clone().unwrap_or_else(|| "none".into()),
                    a.unknown_bits.clone().unwrap_or_else(|| "none".into()),
                ),
            ]
        }
        (Snapshot::Remap(b), Snapshot::Remap(a), SettingValue::ButtonRemap(_, to)) => vec![
            field(
                "mapped_to",
                format!("0x{to:04X}"),
                format!("0x{:04X}", a.reporting.mapped_to),
            ),
            field(
                "reporting flags (unchanged)",
                format!("0x{:04X}", b.reporting.flags),
                format!("0x{:04X}", a.reporting.flags),
            ),
        ],
        _ => vec![FieldCheck {
            field: "snapshot",
            expected: "same feature".into(),
            actual: "different feature".into(),
            ok: false,
        }],
    }
}

/// Result of sending a write and reading back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteOutcome {
    pub plan: WritePlan,
    /// Parameters of the device's answer to the write, trailing zeros removed.
    pub response_hex: String,
    pub after: Snapshot,
    pub checks: Vec<FieldCheck>,
    pub pass: bool,
}

/// Sends `plan` through the write list `auth` grants and verifies it.
pub fn execute<C: RawChannel>(
    r: &mut Reader<'_, C>,
    auth: WriteAuth<'_>,
    before: &Snapshot,
    plan: WritePlan,
) -> Result<WriteOutcome, ReadError> {
    let response = r.write(auth, plan.feature, plan.function, &plan.params)?;
    let after = snapshot(r, plan.target.kind())?;
    let checks = verify(before, &after, plan.target);
    let end = response.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    Ok(WriteOutcome {
        response_hex: hidlab_hidpp::hex(&response[..end]),
        pass: checks.iter().all(|c| c.ok),
        plan,
        after,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ss(mode: u8, thr: u8) -> Snapshot {
        Snapshot::Smartshift(smartshift::decode(&[mode, thr, thr]).unwrap())
    }

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
        for k in SettingKind::EXPERIMENTAL {
            assert_eq!(SettingKind::from_name(k.name()), Some(k));
        }
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
    fn only_confirmed_values_reach_the_daemon() {
        assert!(SettingValue::SmartshiftThreshold(50).confirmed());
        assert!(!SettingValue::SmartshiftThreshold(THRESHOLD_OFF).confirmed());
        assert!(!SettingValue::ButtonRemap(RemapControl::Back, 0x52).confirmed());
        assert!(SettingValue::Dpi(1000).confirmed());
    }

    #[test]
    fn plans_remaps_within_the_groups() {
        // SYNTHETIC state: back may become left, right, middle or itself.
        let s = Snapshot::Remap(reprog_controls::RemapState {
            control: 0x53,
            group_mask: 3,
            targets: vec![0x53, 0x50, 0x51, 0x52],
            reporting: reprog_controls::decode_reporting(&[0, 0x53, 0, 0, 0, 0])
                .unwrap()
                .1,
        });
        let p = plan(&s, SettingValue::ButtonRemap(RemapControl::Back, 0x52)).unwrap();
        assert_eq!(p.params, [0x00, 0x53, 0x00, 0x00, 0x52]);
        assert_eq!((p.feature, p.function), (FeatureId(0x1B04), 3));
        assert!(plan(&s, SettingValue::ButtonRemap(RemapControl::Back, 0xC3)).is_err());
        assert_eq!(
            s.value(SettingKind::ButtonRemap(RemapControl::Back)),
            Some(SettingValue::ButtonRemap(RemapControl::Back, 0x53))
        );
        assert_eq!(
            s.value(SettingKind::ButtonRemap(RemapControl::Forward)),
            None
        );
    }

    #[test]
    fn plans_bytes() {
        // SYNTHETIC snapshots.
        let d = Snapshot::Dpi(dpi::Dpi {
            list: dpi::decode_list(&[0, 0x00, 0xC8, 0xE0, 0x32, 0x1F, 0x40, 0, 0]).unwrap(),
            current: dpi::decode_current(&[0, 0x03, 0xE8, 0x03, 0xE8]).unwrap(),
            current_in_list: true,
        });
        assert_eq!(
            plan(&d, SettingValue::Dpi(1600)).unwrap().params,
            [0x00, 0x06, 0x40]
        );
        assert!(plan(&d, SettingValue::Dpi(1625)).is_err());

        assert_eq!(
            plan(&ss(2, 10), SettingValue::SmartshiftMode(1))
                .unwrap()
                .params,
            [1, 0]
        );
        assert_eq!(
            plan(&ss(2, 10), SettingValue::SmartshiftThreshold(40))
                .unwrap()
                .params,
            [0, 40]
        );

        let t = Snapshot::Thumbwheel(thumbwheel::decode(&[0x00, 0x02]).unwrap());
        let p = plan(&t, SettingValue::ThumbwheelInvert(true)).unwrap();
        assert_eq!(p.params, [0x00, 0x03]);
        assert_eq!((p.feature, p.function), (FeatureId(0x2150), 2));
    }

    #[test]
    fn verify_catches_side_effects() {
        let ok = verify(&ss(2, 10), &ss(1, 10), SettingValue::SmartshiftMode(1));
        assert!(ok.iter().all(|c| c.ok));
        // Threshold changed by a mode write: a side effect.
        let bad = verify(&ss(2, 10), &ss(1, 0), SettingValue::SmartshiftMode(1));
        assert!(!bad.iter().all(|c| c.ok));
        assert_eq!(bad[1].field, "threshold (unchanged)");
    }
}
