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
//! - SmartShift threshold `off` (255): Solaar's `SmartShift` writes 255 for
//!   "always ratcheted" (its setting caps at 50 and maps 50 to 255);
//!   CONFIRMED by H5 (P-035).
//! - Button remapping with 0x1B04 setCidReporting (see `reprog_controls`);
//!   CONFIRMED by H5 (P-036).

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader, WriteAuth};
use crate::reprog_controls;
use crate::{dpi, smartshift, thumbwheel};

pub use hidlab_api::settings::{SettingKind, SettingValue, THRESHOLD_OFF};

/// Where each setting is written on the device.
pub trait SettingKindExt {
    fn feature(self) -> FeatureId;
    fn write_function(self) -> u8;
}

impl SettingKindExt for SettingKind {
    fn feature(self) -> FeatureId {
        match self {
            SettingKind::Dpi => dpi::FEATURE,
            SettingKind::SmartshiftMode | SettingKind::SmartshiftThreshold => smartshift::FEATURE,
            SettingKind::ThumbwheelInvert => thumbwheel::FEATURE,
            SettingKind::ButtonRemap(_) => reprog_controls::FEATURE,
        }
    }

    fn write_function(self) -> u8 {
        match self {
            SettingKind::Dpi => 3,
            SettingKind::SmartshiftMode | SettingKind::SmartshiftThreshold => 1,
            SettingKind::ThumbwheelInvert => 2,
            SettingKind::ButtonRemap(_) => 3,
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
    use crate::reprog_controls::RemapControl;

    fn ss(mode: u8, thr: u8) -> Snapshot {
        Snapshot::Smartshift(smartshift::decode(&[mode, thr, thr]).unwrap())
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
