//! Controlled write experiments for `hid-lab lab write` (requirements §13).
//!
//! Each setting knows how to read its current value, build the write bytes
//! (from Solaar's documented behavior, status LIKELY), and verify the result
//! by reading back the written field and every field that must not change.
//!
//! Sources (R-2, Solaar at the pinned commit):
//! - DPI: `AdjustableDpi` writes function 0x30 with prefix `00` and the
//!   value as 2 bytes big-endian.
//! - SmartShift: `ScrollRatchet` writes function 0x10 with `[mode]`;
//!   `SmartShift` writes `[0, threshold]`. A 0 in the other field is
//!   assumed to mean "no change" (HYPOTHESIS, checked by the read-back).
//! - Thumb wheel: `ThumbInvert` writes function 0x20 with byte 1 bit 0 =
//!   invert, keeping the other bits of the current value.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::{Deserialize, Serialize};

use crate::reader::{ReadError, Reader, WriteIntent};
use crate::{dpi, smartshift, thumbwheel};

/// A setting that `lab write` can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingKind {
    Dpi,
    SmartshiftMode,
    SmartshiftThreshold,
    ThumbwheelInvert,
}

impl SettingKind {
    pub const ALL: [SettingKind; 4] = [
        SettingKind::Dpi,
        SettingKind::SmartshiftMode,
        SettingKind::SmartshiftThreshold,
        SettingKind::ThumbwheelInvert,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SettingKind::Dpi => "dpi",
            SettingKind::SmartshiftMode => "smartshift-mode",
            SettingKind::SmartshiftThreshold => "smartshift-threshold",
            SettingKind::ThumbwheelInvert => "thumbwheel-invert",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub fn feature(self) -> FeatureId {
        match self {
            SettingKind::Dpi => dpi::FEATURE,
            SettingKind::SmartshiftMode | SettingKind::SmartshiftThreshold => smartshift::FEATURE,
            SettingKind::ThumbwheelInvert => thumbwheel::FEATURE,
        }
    }

    pub fn write_function(self) -> u8 {
        match self {
            SettingKind::Dpi => 3,
            SettingKind::SmartshiftMode | SettingKind::SmartshiftThreshold => 1,
            SettingKind::ThumbwheelInvert => 2,
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
            SettingKind::SmartshiftThreshold => v
                .parse::<u8>()
                .ok()
                .filter(|t| (1..=50).contains(t))
                .map(SettingValue::SmartshiftThreshold)
                .ok_or_else(|| format!("threshold must be 1..=50, got `{v}`")),
            SettingKind::ThumbwheelInvert => match v {
                "on" => Ok(SettingValue::ThumbwheelInvert(true)),
                "off" => Ok(SettingValue::ThumbwheelInvert(false)),
                _ => Err(format!("value must be `on` or `off`, got `{v}`")),
            },
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
}

impl SettingValue {
    pub fn kind(self) -> SettingKind {
        match self {
            SettingValue::Dpi(_) => SettingKind::Dpi,
            SettingValue::SmartshiftMode(_) => SettingKind::SmartshiftMode,
            SettingValue::SmartshiftThreshold(_) => SettingKind::SmartshiftThreshold,
            SettingValue::ThumbwheelInvert(_) => SettingKind::ThumbwheelInvert,
        }
    }

    pub fn display(self) -> String {
        match self {
            SettingValue::Dpi(d) => d.to_string(),
            SettingValue::SmartshiftMode(1) => "free-spin".into(),
            SettingValue::SmartshiftMode(2) => "ratchet".into(),
            SettingValue::SmartshiftMode(m) => format!("mode {m}"),
            SettingValue::SmartshiftThreshold(t) => t.to_string(),
            SettingValue::ThumbwheelInvert(true) => "on".into(),
            SettingValue::ThumbwheelInvert(false) => "off".into(),
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

/// Sends `plan` and verifies it. Requires an explicit [`WriteIntent`].
pub fn execute<C: RawChannel>(
    r: &mut Reader<'_, C>,
    intent: &WriteIntent,
    before: &Snapshot,
    plan: WritePlan,
) -> Result<WriteOutcome, ReadError> {
    let response = r.write(intent, plan.feature, plan.function, &plan.params)?;
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
        // Threshold changed by a mode write: the "0 = no change" hypothesis failed.
        let bad = verify(&ss(2, 10), &ss(1, 0), SettingValue::SmartshiftMode(1));
        assert!(!bad.iter().all(|c| c.ok));
        assert_eq!(bad[1].field, "threshold (unchanged)");
    }
}
