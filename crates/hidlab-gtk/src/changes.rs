//! What the app asks the daemon to do for each control, and how to undo
//! it. Pure: computed from the last status, tested without a display.

use gettextrs::gettext;
use hidlab_api::daemon::{DesktopSetting, Status};
use hidlab_api::settings::{SettingKind, SettingValue, THRESHOLD_OFF};

use crate::model::fill;

/// One daemon call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// `Set`: write and save one mouse setting.
    Set(SettingValue),
    /// `Reset`: forget one mouse setting and write its default.
    Reset(SettingKind),
    /// `SetDesktop`: one GNOME pointer setting.
    Desktop(DesktopSetting, String),
}

impl Op {
    /// The D-Bus method and its string arguments.
    pub fn call(&self) -> (&'static str, Vec<String>) {
        match self {
            Op::Set(v) => ("Set", vec![v.kind().name().to_owned(), arg(*v)]),
            Op::Reset(k) => ("Reset", vec![k.name().to_owned()]),
            Op::Desktop(s, v) => ("SetDesktop", vec![s.name().to_owned(), v.clone()]),
        }
    }
}

/// The value as `hid-lab set` and the daemon's `Set` read it.
fn arg(v: SettingValue) -> String {
    match v {
        // `display` marks the default (`back (default)`); send the name.
        SettingValue::ButtonRemap(c, t) => hidlab_api::remap::target_name(t)
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if t == c.cid() {
                    "default".into()
                } else {
                    format!("0x{t:04X}")
                }
            }),
        other => other.display(),
    }
}

/// A user's change: the calls, the calls that undo it, and the toast text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub ops: Vec<Op>,
    pub undo: Vec<Op>,
    pub label: String,
}

/// The settings a list of calls touches, sorted: two changes with the same
/// targets in a row are one change for Undo.
pub fn targets(ops: &[Op]) -> Vec<String> {
    let mut t: Vec<String> = ops.iter().map(|o| o.call().1[0].clone()).collect();
    t.sort_unstable();
    t.dedup();
    t
}

/// The Undo of `next`. When the previous change's toast is still shown and
/// touched the same settings, Undo goes back to before the previous one, so
/// several quick moves of one control undo together.
pub fn merged_undo(previous: Option<(&[String], &[Op])>, next: &Change) -> Vec<Op> {
    match previous {
        Some((targets_before, undo)) if targets_before == targets(&next.ops).as_slice() => {
            undo.to_vec()
        }
        _ => next.undo.clone(),
    }
}

/// The three wheel modes of the mockup, over SmartShift mode and threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelMode {
    /// Ratchet, never switching (threshold `off`).
    Clicks,
    /// Free spin.
    Free,
    /// Ratchet that switches to free spin with a push (threshold 1..=50).
    Auto,
}

impl WheelMode {
    pub const ALL: [WheelMode; 3] = [WheelMode::Clicks, WheelMode::Free, WheelMode::Auto];

    pub fn id(self) -> &'static str {
        match self {
            WheelMode::Clicks => "clicks",
            WheelMode::Free => "free",
            WheelMode::Auto => "auto",
        }
    }

    pub fn from_id(id: &str) -> Option<WheelMode> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    pub fn label(self) -> String {
        match self {
            WheelMode::Clicks => gettext("Clicks"),
            WheelMode::Free => gettext("Free spin"),
            WheelMode::Auto => gettext("Automatic"),
        }
    }

    pub fn description(self) -> String {
        match self {
            WheelMode::Clicks => gettext("Always clicks, even when you push it."),
            WheelMode::Free => gettext("Spins freely; the mode button brings the clicks back."),
            WheelMode::Auto => {
                gettext("Clicks when you turn it slowly and spins freely when you push it.")
            }
        }
    }
}

/// The value of `kind` read from the mouse.
pub fn current(status: &Status, kind: SettingKind) -> Option<SettingValue> {
    status
        .current
        .iter()
        .find(|c| c.setting == kind)
        .and_then(|c| c.value)
}

/// The value of `kind` the daemon keeps (config file or state).
pub fn desired(status: &Status, kind: SettingKind) -> Option<SettingValue> {
    status.desired.iter().copied().find(|v| v.kind() == kind)
}

pub fn wheel_mode(status: &Status) -> Option<WheelMode> {
    match current(status, SettingKind::SmartshiftMode)? {
        SettingValue::SmartshiftMode(1) => Some(WheelMode::Free),
        SettingValue::SmartshiftMode(2) => {
            match current(status, SettingKind::SmartshiftThreshold) {
                Some(SettingValue::SmartshiftThreshold(THRESHOLD_OFF)) => Some(WheelMode::Clicks),
                Some(SettingValue::SmartshiftThreshold(_)) => Some(WheelMode::Auto),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The threshold the push slider shows: the mouse's, or the one kept
/// while switching is off, or the profile default.
pub fn push_threshold(status: &Status) -> u8 {
    match current(status, SettingKind::SmartshiftThreshold) {
        Some(SettingValue::SmartshiftThreshold(t)) if t != THRESHOLD_OFF => t,
        _ => status
            .saved_threshold
            .or_else(|| default_of(status, SettingKind::SmartshiftThreshold))
            .unwrap_or(10),
    }
}

fn default_of(status: &Status, kind: SettingKind) -> Option<u8> {
    status
        .capabilities
        .as_ref()?
        .defaults
        .iter()
        .find(|v| v.kind() == kind)
        .and_then(|v| match *v {
            SettingValue::SmartshiftThreshold(t) => Some(t),
            _ => None,
        })
}

/// Calls that put `kind` back the way the daemon keeps it now.
fn restore(status: &Status, kind: SettingKind) -> Vec<Op> {
    match desired(status, kind) {
        Some(SettingValue::SmartshiftThreshold(THRESHOLD_OFF)) => {
            // `off` keeps the saved threshold; write it first so a reset
            // in between does not lose it.
            let mut ops: Vec<Op> = status
                .saved_threshold
                .map(|t| Op::Set(SettingValue::SmartshiftThreshold(t)))
                .into_iter()
                .collect();
            ops.push(Op::Set(SettingValue::SmartshiftThreshold(THRESHOLD_OFF)));
            ops
        }
        Some(v) => vec![Op::Set(v)],
        None => vec![Op::Reset(kind)],
    }
}

/// Text for a value in a toast, for example `Pointer speed: 1600 DPI`.
pub fn value_label(v: SettingValue) -> String {
    match v {
        SettingValue::Dpi(d) => fill(
            // TRANSLATORS: toast after a change; {dpi} is a number.
            &gettext("Pointer speed: {dpi} DPI"),
            &[("dpi", &d.to_string())],
        ),
        SettingValue::SmartshiftThreshold(THRESHOLD_OFF) => fill(
            &gettext("Wheel: {mode}"),
            &[("mode", &WheelMode::Clicks.label())],
        ),
        SettingValue::SmartshiftThreshold(t) => fill(
            // TRANSLATORS: toast; {value} is a number from 1 to 50.
            &gettext("Push to switch to free spin: {value}"),
            &[("value", &t.to_string())],
        ),
        SettingValue::SmartshiftMode(1) => fill(
            &gettext("Wheel: {mode}"),
            &[("mode", &WheelMode::Free.label())],
        ),
        SettingValue::SmartshiftMode(_) => fill(
            &gettext("Wheel: {mode}"),
            &[("mode", &WheelMode::Auto.label())],
        ),
        SettingValue::ThumbwheelInvert(true) => gettext("Thumb wheel: reversed"),
        SettingValue::ThumbwheelInvert(false) => gettext("Thumb wheel: normal direction"),
        SettingValue::ButtonRemap(c, t) => crate::remaps::remap_label(c, t),
    }
}

/// Writes one mouse setting; `None` when the mouse already has it.
pub fn set(status: &Status, v: SettingValue) -> Option<Change> {
    if current(status, v.kind()) == Some(v) && desired(status, v.kind()) == Some(v) {
        return None;
    }
    Some(Change {
        ops: vec![Op::Set(v)],
        undo: restore(status, v.kind()),
        label: value_label(v),
    })
}

/// Switches the wheel mode; `None` when it is already in that mode.
pub fn set_wheel_mode(status: &Status, mode: WheelMode) -> Option<Change> {
    if wheel_mode(status) == Some(mode) {
        return None;
    }
    let ratchet = SettingValue::SmartshiftMode(2);
    let mut ops = Vec::new();
    let mut kinds = Vec::new();
    if mode == WheelMode::Free {
        ops.push(Op::Set(SettingValue::SmartshiftMode(1)));
        kinds.push(SettingKind::SmartshiftMode);
    } else {
        if current(status, SettingKind::SmartshiftMode) != Some(ratchet) {
            ops.push(Op::Set(ratchet));
            kinds.push(SettingKind::SmartshiftMode);
        }
        let t = match mode {
            WheelMode::Clicks => THRESHOLD_OFF,
            _ => push_threshold(status),
        };
        if current(status, SettingKind::SmartshiftThreshold)
            != Some(SettingValue::SmartshiftThreshold(t))
        {
            ops.push(Op::Set(SettingValue::SmartshiftThreshold(t)));
            kinds.push(SettingKind::SmartshiftThreshold);
        }
    }
    let undo = kinds
        .iter()
        .rev()
        .flat_map(|&k| restore(status, k))
        .collect();
    Some(Change {
        ops,
        undo,
        label: fill(&gettext("Wheel: {mode}"), &[("mode", &mode.label())]),
    })
}

/// Puts settings back to the mouse's defaults (a page section's "Restore
/// Default"); `None` when none of them is kept.
pub fn reset(status: &Status, kinds: &[SettingKind], label: String) -> Option<Change> {
    let kept: Vec<SettingKind> = kinds
        .iter()
        .copied()
        .filter(|&k| desired(status, k).is_some() || current(status, k) != default(status, k))
        .collect();
    if kept.is_empty() {
        return None;
    }
    Some(Change {
        ops: kept.iter().map(|&k| Op::Reset(k)).collect(),
        undo: kept
            .iter()
            .rev()
            .flat_map(|&k| restore(status, k))
            .collect(),
        label,
    })
}

fn default(status: &Status, kind: SettingKind) -> Option<SettingValue> {
    status
        .capabilities
        .as_ref()?
        .defaults
        .iter()
        .copied()
        .find(|v| v.kind() == kind)
}

/// Writes a GNOME setting; undone with GNOME's value from the status.
pub fn set_desktop(status: &Status, setting: DesktopSetting, value: &str) -> Option<Change> {
    let before = match setting {
        DesktopSetting::NaturalScroll => status
            .desktop
            .natural_scroll
            .map(|b| if b { "on" } else { "off" }.to_owned()),
        DesktopSetting::Acceleration => status.desktop.acceleration.clone(),
    };
    if before.as_deref() == Some(value) {
        return None;
    }
    let label = match (setting, value) {
        (DesktopSetting::NaturalScroll, "on") => gettext("Natural scrolling on"),
        (DesktopSetting::NaturalScroll, _) => gettext("Natural scrolling off"),
        (DesktopSetting::Acceleration, v) => fill(
            &gettext("Acceleration: {profile}"),
            &[("profile", &acceleration_label(v))],
        ),
    };
    Some(Change {
        ops: vec![Op::Desktop(setting, value.to_owned())],
        undo: before
            .map(|b| Op::Desktop(setting, b))
            .into_iter()
            .collect(),
        label,
    })
}

/// GNOME's `accel-profile` values, in the order the app lists them.
pub const ACCELERATION: [&str; 3] = ["default", "flat", "adaptive"];

pub fn acceleration_label(value: &str) -> String {
    match value {
        "flat" => gettext("Flat"),
        "adaptive" => gettext("Adaptive"),
        _ => gettext("Default"),
    }
}

/// DPI slider: range, step and the marks to show (value, is default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpiScale {
    pub min: u16,
    pub max: u16,
    pub step: u16,
    pub marks: Vec<(u16, bool)>,
}

/// Marks under the slider, where the mouse supports them. Doublings sit
/// evenly on the logarithmic slider; the default gets its own mark above.
const DPI_MARKS: [u16; 5] = [400, 800, 1600, 3200, 6400];

pub fn dpi_scale(status: &Status) -> Option<DpiScale> {
    let caps = status.capabilities.as_ref()?;
    let min = caps.dpi.iter().map(|r| r.from).min()?;
    let max = caps.dpi.iter().map(|r| r.to).max()?;
    let step = caps.dpi.first().map_or(1, |r| r.step.max(1));
    let default = caps.defaults.iter().find_map(|v| match *v {
        SettingValue::Dpi(d) => Some(d),
        _ => None,
    });
    let mut marks: Vec<(u16, bool)> = DPI_MARKS
        .iter()
        .copied()
        .filter(|&d| Some(d) != default)
        .chain(default)
        .filter(|&d| (min..=max).contains(&d))
        .map(|d| (d, Some(d) == default))
        .collect();
    marks.sort_unstable();
    Some(DpiScale {
        min,
        max,
        step,
        marks,
    })
}

impl DpiScale {
    /// Slider position of a DPI value. The slider is logarithmic, so equal
    /// distances are equal ratios, as the pointer speed feels.
    pub fn position(dpi: u16) -> f64 {
        f64::from(dpi.max(1)).ln()
    }

    /// The DPI value the mouse accepts nearest to a slider position.
    pub fn at(&self, position: f64) -> u16 {
        self.snap(position.exp())
    }

    /// The nearest value the mouse accepts.
    pub fn snap(&self, value: f64) -> u16 {
        let steps = ((value - f64::from(self.min)) / f64::from(self.step)).round();
        let v = f64::from(self.min) + steps.max(0.0) * f64::from(self.step);
        (v.min(f64::from(self.max))) as u16
    }
}

#[cfg(test)]
mod tests {
    //! Status documents here are SYNTHETIC.
    use super::*;
    use hidlab_api::daemon::{Capabilities, CurrentValue, DesktopSettings, DpiRange};

    fn status(current: &[SettingValue], desired: &[SettingValue], saved: Option<u8>) -> Status {
        Status {
            schema: "hid-lab.daemon-status/3".into(),
            daemon_version: "0.1.0".into(),
            config_file: "~/.config/hid-lab/config.toml".into(),
            config_error: None,
            state_error: None,
            device: None,
            desired: desired.to_vec(),
            current: current
                .iter()
                .map(|&v| CurrentValue {
                    setting: v.kind(),
                    value: Some(v),
                    error: None,
                })
                .collect(),
            saved_threshold: saved,
            battery: None,
            capabilities: Some(Capabilities {
                dpi: vec![DpiRange {
                    from: 200,
                    to: 8000,
                    step: 50,
                }],
                threshold_min: 1,
                threshold_max: 50,
                buttons: vec![],
                defaults: vec![
                    SettingValue::Dpi(1000),
                    SettingValue::SmartshiftMode(2),
                    SettingValue::SmartshiftThreshold(10),
                    SettingValue::ThumbwheelInvert(false),
                ],
            }),
            desktop: DesktopSettings {
                natural_scroll: Some(false),
                acceleration: Some("adaptive".into()),
                error: None,
            },
            last_apply: None,
            conflicts: vec![],
        }
    }

    use SettingValue::{
        Dpi, SmartshiftMode as Mode, SmartshiftThreshold as Thr, ThumbwheelInvert as Inv,
    };

    #[test]
    fn op_calls() {
        assert_eq!(
            Op::Set(Thr(255)).call(),
            ("Set", vec!["smartshift-threshold".into(), "off".into()])
        );
        assert_eq!(Op::Set(Mode(1)).call().1[1], "free-spin");
        assert_eq!(Op::Set(Inv(true)).call().1[1], "on");
        use hidlab_api::remap::RemapControl;
        assert_eq!(
            Op::Set(SettingValue::ButtonRemap(RemapControl::Back, 0x53))
                .call()
                .1,
            ["remap-back", "back"]
        );
        assert_eq!(
            Op::Set(SettingValue::ButtonRemap(RemapControl::Gesture, 0x52))
                .call()
                .1,
            ["remap-gesture", "middle"]
        );
        assert_eq!(
            Op::Reset(SettingKind::Dpi).call(),
            ("Reset", vec!["dpi".into()])
        );
        assert_eq!(
            Op::Desktop(DesktopSetting::NaturalScroll, "on".into()).call(),
            ("SetDesktop", vec!["natural-scroll".into(), "on".into()])
        );
    }

    #[test]
    fn wheel_modes_from_the_mouse() {
        assert_eq!(
            wheel_mode(&status(&[Mode(1), Thr(10)], &[], None)),
            Some(WheelMode::Free)
        );
        assert_eq!(
            wheel_mode(&status(&[Mode(2), Thr(255)], &[], None)),
            Some(WheelMode::Clicks)
        );
        assert_eq!(
            wheel_mode(&status(&[Mode(2), Thr(30)], &[], None)),
            Some(WheelMode::Auto)
        );
        assert_eq!(wheel_mode(&status(&[], &[], None)), None);
    }

    #[test]
    fn dpi_change_undoes_to_the_kept_value_or_a_reset() {
        let kept = status(&[Dpi(1600)], &[Dpi(1600)], None);
        let c = set(&kept, Dpi(2000)).unwrap();
        assert_eq!(c.ops, [Op::Set(Dpi(2000))]);
        assert_eq!(c.undo, [Op::Set(Dpi(1600))]);
        assert_eq!(c.label, "Pointer speed: 2000 DPI");

        let unset = status(&[Dpi(1000)], &[], None);
        assert_eq!(
            set(&unset, Dpi(2000)).unwrap().undo,
            [Op::Reset(SettingKind::Dpi)]
        );
        // Already there and kept: nothing to do.
        assert_eq!(set(&kept, Dpi(1600)), None);
        // On the mouse but not kept: saving it is a change.
        assert!(set(&unset, Dpi(1000)).is_some());
    }

    #[test]
    fn clicks_from_free_spin_sets_ratchet_then_off() {
        let st = status(&[Mode(1), Thr(30)], &[Mode(1), Thr(30)], Some(30));
        let c = set_wheel_mode(&st, WheelMode::Clicks).unwrap();
        assert_eq!(c.ops, [Op::Set(Mode(2)), Op::Set(Thr(255))]);
        assert_eq!(c.undo, [Op::Set(Thr(30)), Op::Set(Mode(1))]);
        assert_eq!(c.label, "Wheel: Clicks");
    }

    #[test]
    fn automatic_turns_the_saved_threshold_back_on() {
        let st = status(&[Mode(2), Thr(255)], &[Mode(2), Thr(255)], Some(30));
        let c = set_wheel_mode(&st, WheelMode::Auto).unwrap();
        assert_eq!(c.ops, [Op::Set(Thr(30))]);
        // Undo writes the saved threshold, then `off`.
        assert_eq!(c.undo, [Op::Set(Thr(30)), Op::Set(Thr(255))]);

        let fresh = status(&[Mode(2), Thr(255)], &[], None);
        let c = set_wheel_mode(&fresh, WheelMode::Auto).unwrap();
        assert_eq!(c.ops, [Op::Set(Thr(10))]);
        assert_eq!(c.undo, [Op::Reset(SettingKind::SmartshiftThreshold)]);
    }

    #[test]
    fn free_spin_writes_only_the_mode() {
        let st = status(&[Mode(2), Thr(30)], &[Mode(2), Thr(30)], Some(30));
        let c = set_wheel_mode(&st, WheelMode::Free).unwrap();
        assert_eq!(c.ops, [Op::Set(Mode(1))]);
        assert_eq!(c.undo, [Op::Set(Mode(2))]);
        assert_eq!(set_wheel_mode(&st, WheelMode::Auto), None);
    }

    #[test]
    fn push_slider_shows_the_kept_threshold_while_off() {
        assert_eq!(push_threshold(&status(&[Mode(2), Thr(42)], &[], None)), 42);
        assert_eq!(
            push_threshold(&status(&[Mode(2), Thr(255)], &[], Some(30))),
            30
        );
        assert_eq!(push_threshold(&status(&[Mode(2), Thr(255)], &[], None)), 10);
    }

    #[test]
    fn reset_skips_what_is_already_default() {
        let st = status(&[Dpi(1000), Inv(true)], &[Inv(true)], None);
        let c = reset(
            &st,
            &[SettingKind::Dpi, SettingKind::ThumbwheelInvert],
            "r".into(),
        )
        .unwrap();
        assert_eq!(c.ops, [Op::Reset(SettingKind::ThumbwheelInvert)]);
        assert_eq!(c.undo, [Op::Set(Inv(true))]);
        let clean = status(&[Dpi(1000)], &[], None);
        assert_eq!(reset(&clean, &[SettingKind::Dpi], "r".into()), None);
    }

    #[test]
    fn desktop_changes_undo_to_gnome_value() {
        let st = status(&[], &[], None);
        let c = set_desktop(&st, DesktopSetting::Acceleration, "flat").unwrap();
        assert_eq!(
            c.undo,
            [Op::Desktop(DesktopSetting::Acceleration, "adaptive".into())]
        );
        assert_eq!(c.label, "Acceleration: Flat");
        assert_eq!(set_desktop(&st, DesktopSetting::NaturalScroll, "off"), None);
    }

    #[test]
    fn quick_changes_of_one_setting_undo_together() {
        let first = set(&status(&[Dpi(1000)], &[Dpi(1000)], None), Dpi(1300)).unwrap();
        let second = set(&status(&[Dpi(1300)], &[Dpi(1300)], None), Dpi(1600)).unwrap();
        let t = targets(&first.ops);
        assert_eq!(t, ["dpi"]);
        assert_eq!(
            merged_undo(Some((&t, &first.undo)), &second),
            [Op::Set(Dpi(1000))]
        );
        // Another setting, or no toast shown: its own undo.
        let invert = set(&status(&[Inv(false)], &[], None), Inv(true)).unwrap();
        assert_eq!(merged_undo(Some((&t, &first.undo)), &invert), invert.undo);
        assert_eq!(merged_undo(None, &second), [Op::Set(Dpi(1300))]);
    }

    #[test]
    fn dpi_scale_marks_and_snapping() {
        let s = dpi_scale(&status(&[], &[], None)).unwrap();
        assert_eq!((s.min, s.max, s.step), (200, 8000, 50));
        assert_eq!(
            s.marks,
            [
                (400, false),
                (800, false),
                (1000, true),
                (1600, false),
                (3200, false),
                (6400, false)
            ]
        );
        assert_eq!(s.at(DpiScale::position(1600)), 1600);
        assert_eq!(s.at(DpiScale::position(200) - 0.5), 200);
        assert_eq!(s.snap(1624.0), 1600);
        assert_eq!(s.snap(1626.0), 1650);
        assert_eq!(s.snap(10.0), 200);
        assert_eq!(s.snap(9000.0), 8000);
    }
}
