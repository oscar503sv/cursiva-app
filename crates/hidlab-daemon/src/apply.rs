//! Applying desired settings to a connected device.
//!
//! For each setting: read the current value, write only if it differs (with
//! [`WriteAuth::Confirmed`]), read back and compare the written field and
//! the fields that must not change.

use std::time::Instant;

use hidlab_devices::DeviceProfile;
use hidlab_features::dpi::DpiEntry;
use hidlab_features::reprog_controls;
use hidlab_features::settings::{self, FieldCheck, SettingKind, SettingValue, Snapshot};
use hidlab_features::{ReadError, Reader, WriteAuth};
use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::{Deserialize, Serialize};

use crate::api::{ButtonTargets, Capabilities, DpiRange};
use crate::config::THRESHOLD_RANGE;

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

impl From<&FieldCheck> for CheckJson {
    fn from(c: &FieldCheck) -> Self {
        Self {
            field: c.field.to_owned(),
            expected: c.expected.clone(),
            actual: c.actual.clone(),
            ok: c.ok,
        }
    }
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

/// Features the daemon needs resolved for `profile`.
pub fn features_for(profile: &DeviceProfile) -> Vec<FeatureId> {
    let mut out: Vec<FeatureId> = profile.settings.iter().map(|k| k.feature()).collect();
    out.sort();
    out.dedup();
    out
}

fn not_applied(
    result: ItemResult,
    target: SettingValue,
    before: Option<SettingValue>,
    error: String,
) -> ApplyItem {
    ApplyItem {
        setting: target.kind(),
        target,
        before,
        result,
        checks: Vec::new(),
        error: Some(error),
    }
}

fn apply_one<C: RawChannel>(
    r: &mut Reader<'_, C>,
    target: SettingValue,
) -> Result<ApplyItem, ReadError> {
    let before_snap = settings::snapshot(r, target.kind())?;
    let before = before_snap.value(target.kind());
    if before == Some(target) {
        return Ok(ApplyItem {
            setting: target.kind(),
            target,
            before,
            result: ItemResult::Unchanged,
            checks: Vec::new(),
            error: None,
        });
    }
    if !target.confirmed() {
        return Ok(not_applied(
            ItemResult::Rejected,
            target,
            before,
            format!(
                "{} {} has not been confirmed on the mouse; not sent",
                target.kind().name(),
                target.display()
            ),
        ));
    }
    let plan = match settings::plan(&before_snap, target) {
        Ok(p) => p,
        Err(e) => return Ok(not_applied(ItemResult::Rejected, target, before, e)),
    };
    let outcome = settings::execute(r, WriteAuth::Confirmed, &before_snap, plan)?;
    Ok(ApplyItem {
        setting: target.kind(),
        target,
        before,
        result: if outcome.pass {
            ItemResult::Written
        } else {
            ItemResult::Mismatch
        },
        checks: outcome.checks.iter().map(CheckJson::from).collect(),
        error: None,
    })
}

/// Applies `desired` in order. Settings the profile does not list are not
/// sent. After a timeout or disconnect the remaining items fail unsent.
pub fn apply<C: RawChannel>(
    r: &mut Reader<'_, C>,
    profile: &DeviceProfile,
    desired: &[SettingValue],
) -> ApplyReport {
    let start = Instant::now();
    let ignored_at_start = r.session().ignored();
    let mut items = Vec::with_capacity(desired.len());
    let mut fatal: Option<String> = None;
    for &target in desired {
        if let Some(e) = &fatal {
            items.push(not_applied(
                ItemResult::Failed,
                target,
                None,
                format!("not sent: {e}"),
            ));
            continue;
        }
        if !profile.supports(target.kind()) {
            items.push(not_applied(
                ItemResult::Rejected,
                target,
                None,
                format!("{} does not support {}", profile.name, target.kind().name()),
            ));
            continue;
        }
        match apply_one(r, target) {
            Ok(item) => items.push(item),
            Err(e) => {
                if e.is_fatal() {
                    fatal = Some(e.to_string());
                }
                items.push(not_applied(ItemResult::Failed, target, None, e.to_string()));
            }
        }
    }
    ApplyReport {
        ok: items
            .iter()
            .all(|i| matches!(i.result, ItemResult::Unchanged | ItemResult::Written)),
        items,
        elapsed_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        ignored_reports: r.session().ignored() - ignored_at_start,
    }
}

/// Reads the feature of every setting the profile lists (reads only), in
/// the profile's order.
pub fn read_snapshots<C: RawChannel>(
    r: &mut Reader<'_, C>,
    profile: &DeviceProfile,
) -> Vec<Result<Snapshot, String>> {
    profile
        .settings
        .iter()
        .map(|&k| settings::snapshot(r, k).map_err(|e| e.to_string()))
        .collect()
}

/// The current value of each setting in `snaps` (from [`read_snapshots`]).
pub fn values_of(
    profile: &DeviceProfile,
    snaps: &[Result<Snapshot, String>],
) -> Vec<Result<SettingValue, String>> {
    profile
        .settings
        .iter()
        .zip(snaps)
        .map(|(&k, snap)| {
            snap.as_ref().map_err(Clone::clone).and_then(|s| {
                s.value(k)
                    .ok_or_else(|| format!("{} not in the read answer", k.name()))
            })
        })
        .collect()
}

/// Reads the current value of every setting the profile lists (reads only).
pub fn read_current<C: RawChannel>(
    r: &mut Reader<'_, C>,
    profile: &DeviceProfile,
) -> Vec<Result<SettingValue, String>> {
    values_of(profile, &read_snapshots(r, profile))
}

/// What the mouse accepts, from the snapshots of [`read_snapshots`]. `None`
/// if the DPI list could not be read.
pub fn capabilities(
    profile: &DeviceProfile,
    snaps: &[Result<Snapshot, String>],
) -> Option<Capabilities> {
    let mut dpi = None;
    let mut buttons = Vec::new();
    for (&k, snap) in profile.settings.iter().zip(snaps) {
        match (k, snap) {
            (SettingKind::Dpi, Ok(Snapshot::Dpi(d))) => {
                dpi = Some(
                    d.list
                        .entries
                        .iter()
                        .map(|e| match *e {
                            DpiEntry::Value { dpi } => DpiRange {
                                from: dpi,
                                to: dpi,
                                step: 1,
                            },
                            DpiEntry::Range { from, to, step } => DpiRange { from, to, step },
                        })
                        .collect(),
                );
            }
            (SettingKind::ButtonRemap(c), snap) => buttons.push(ButtonTargets {
                button: c,
                targets: match snap {
                    Ok(Snapshot::Remap(s)) => s
                        .targets
                        .iter()
                        .map(|&t| {
                            reprog_controls::target_name(t)
                                .map_or_else(|| format!("0x{t:04X}"), str::to_owned)
                        })
                        .collect(),
                    _ => Vec::new(),
                },
            }),
            _ => {}
        }
    }
    Some(Capabilities {
        dpi: dpi?,
        threshold_min: *THRESHOLD_RANGE.start(),
        threshold_max: *THRESHOLD_RANGE.end(),
        buttons,
        defaults: profile.defaults.to_vec(),
    })
}
