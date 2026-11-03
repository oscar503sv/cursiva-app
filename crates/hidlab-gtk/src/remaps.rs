//! Words and lookups for the Buttons page: button and action names, the
//! actions a button allows, the search filter, and which parts to light for
//! a reported press. Pure, tested without a display.

use gettextrs::gettext;
use hidlab_api::daemon::Status;
use hidlab_api::remap::{self, RemapControl};
use hidlab_api::settings::{SettingKind, SettingValue};

use crate::changes;
use crate::geometry::Hotspot;
use crate::model::fill;

/// Rows of the Buttons page, in the mockup's order.
pub const ROWS: [RemapControl; 5] = [
    RemapControl::Middle,
    RemapControl::ModeButton,
    RemapControl::Forward,
    RemapControl::Back,
    RemapControl::Gesture,
];

pub fn button_name(c: RemapControl) -> String {
    match c {
        RemapControl::Middle => gettext("Wheel button"),
        RemapControl::ModeButton => gettext("Mode button"),
        RemapControl::Forward => gettext("Forward"),
        RemapControl::Back => gettext("Back"),
        RemapControl::Gesture => gettext("Gesture button"),
    }
}

/// Where the button is, for the row's subtitle.
pub fn button_place(c: RemapControl) -> String {
    match c {
        RemapControl::Middle => gettext("Press the wheel"),
        RemapControl::ModeButton => gettext("Behind the wheel"),
        RemapControl::Forward => gettext("Upper side button"),
        RemapControl::Back => gettext("Lower side button"),
        RemapControl::Gesture => gettext("Under the thumb"),
    }
}

/// The task of a control, by its control ID (a remap target).
pub fn action_name(target: u16) -> String {
    match remap::target_name(target) {
        Some("left") => gettext("Left click"),
        Some("right") => gettext("Right click"),
        Some("middle") => gettext("Middle click"),
        Some("back") => gettext("Back"),
        Some("forward") => gettext("Forward"),
        Some("gesture") => gettext("Gesture button"),
        Some("mode") => gettext("Switch wheel mode"),
        _ => fill(
            // TRANSLATORS: a control the app has no name for; {id} is hex.
            &gettext("Control {id}"),
            &[("id", &format!("0x{target:04X}"))],
        ),
    }
}

pub fn action_description(target: u16) -> String {
    match remap::target_name(target) {
        Some("left") => gettext("Select and open"),
        Some("right") => gettext("Open the context menu"),
        Some("middle") => gettext("Open links in a new tab, paste the selection"),
        Some("back") => gettext("Go to the previous page"),
        Some("forward") => gettext("Go to the next page"),
        Some("gesture") => gettext("What the gesture button does by itself"),
        Some("mode") => gettext("Between clicks and free spin"),
        _ => String::new(),
    }
}

/// The part of the drawing for a control.
pub fn hotspot(c: RemapControl) -> Hotspot {
    match c {
        RemapControl::Middle => Hotspot::Wheel,
        RemapControl::ModeButton => Hotspot::Mode,
        RemapControl::Forward => Hotspot::Forward,
        RemapControl::Back => Hotspot::Back,
        RemapControl::Gesture => Hotspot::Gesture,
    }
}

/// The control for a part of the drawing; `None` for parts that are not
/// remappable buttons.
pub fn control(h: Hotspot) -> Option<RemapControl> {
    ROWS.into_iter().find(|&c| hotspot(c) == h)
}

/// The task the button does now, by control ID.
pub fn current_target(status: &Status, c: RemapControl) -> Option<u16> {
    match changes::current(status, SettingKind::ButtonRemap(c))? {
        SettingValue::ButtonRemap(_, t) => Some(t),
        _ => None,
    }
}

/// Tasks the mouse allows for `c`, its own first (the daemon's order).
pub fn targets(status: &Status, c: RemapControl) -> Vec<u16> {
    status
        .capabilities
        .as_ref()
        .and_then(|caps| caps.buttons.iter().find(|b| b.button == c))
        .map(|b| {
            b.targets
                .iter()
                .filter_map(|n| remap::target_cid(n))
                .collect()
        })
        .unwrap_or_default()
}

/// Toast text after a remap.
pub fn remap_label(c: RemapControl, target: u16) -> String {
    if target == c.cid() {
        fill(
            // TRANSLATORS: {button} is a button name such as "Back".
            &gettext("{button} does its own task again"),
            &[("button", &button_name(c))],
        )
    } else {
        fill(
            // TRANSLATORS: {button} and {action} are names, such as
            // "Back" and "Middle click".
            &gettext("{button} now does {action}"),
            &[
                ("button", &button_name(c)),
                ("action", &action_name(target)),
            ],
        )
    }
}

/// Lowercase without accents, for searching.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// True when every word of `query` is in the action's name or description.
pub fn matches(query: &str, target: u16) -> bool {
    let haystack = fold(&format!(
        "{} {}",
        action_name(target),
        action_description(target)
    ));
    fold(query).split_whitespace().all(|w| haystack.contains(w))
}

/// Parts to light for a press the daemon reports. The mouse reports the
/// task, not the physical button, so every button doing that task lights.
pub fn pressed_parts(status: &Status, task: &str) -> Vec<Hotspot> {
    let Some(cid) = remap::target_cid(task) else {
        return Vec::new();
    };
    let mut parts: Vec<Hotspot> = match task {
        // Left and right are never remapped.
        "left" => vec![Hotspot::Left],
        "right" => vec![Hotspot::Right],
        _ => Vec::new(),
    };
    parts.extend(
        ROWS.into_iter()
            .filter(|&c| current_target(status, c).unwrap_or(c.cid()) == cid)
            .map(hotspot),
    );
    parts
}

#[cfg(test)]
mod tests {
    //! Status documents here are SYNTHETIC.
    use super::*;
    use hidlab_api::daemon::{ButtonTargets, Capabilities, CurrentValue, DesktopSettings};

    fn status(remaps: &[(RemapControl, u16)]) -> Status {
        Status {
            schema: "hid-lab.daemon-status/3".into(),
            daemon_version: "0.1.0".into(),
            config_file: "~/.config/hid-lab/config.toml".into(),
            config_error: None,
            state_error: None,
            device: None,
            desired: vec![],
            current: ROWS
                .iter()
                .map(|&c| {
                    let t = remaps.iter().find(|r| r.0 == c).map_or(c.cid(), |r| r.1);
                    CurrentValue {
                        setting: SettingKind::ButtonRemap(c),
                        value: Some(SettingValue::ButtonRemap(c, t)),
                        error: None,
                    }
                })
                .collect(),
            saved_threshold: None,
            battery: None,
            capabilities: Some(Capabilities {
                dpi: vec![],
                threshold_min: 1,
                threshold_max: 50,
                buttons: vec![ButtonTargets {
                    button: RemapControl::Back,
                    targets: vec!["back".into(), "left".into(), "middle".into(), "nope".into()],
                }],
                defaults: vec![],
            }),
            desktop: DesktopSettings {
                natural_scroll: None,
                acceleration: None,
                error: None,
            },
            last_apply: None,
            conflicts: vec![],
        }
    }

    #[test]
    fn targets_come_from_the_mouse_in_its_order() {
        let st = status(&[]);
        assert_eq!(targets(&st, RemapControl::Back), [0x53, 0x50, 0x52]);
        assert!(targets(&st, RemapControl::Gesture).is_empty());
    }

    #[test]
    fn a_remapped_press_lights_every_button_doing_that_task() {
        let st = status(&[(RemapControl::Back, 0x52)]);
        assert_eq!(
            pressed_parts(&st, "middle"),
            [Hotspot::Wheel, Hotspot::Back]
        );
        // Back's own task is done by no button now.
        assert!(pressed_parts(&st, "back").is_empty());
        assert_eq!(pressed_parts(&st, "left"), [Hotspot::Left]);
        assert_eq!(pressed_parts(&st, "mode"), [Hotspot::Mode]);
        assert!(pressed_parts(&st, "unknown").is_empty());
    }

    #[test]
    fn search_ignores_case_and_accents() {
        assert!(matches("MIDDLE", 0x52));
        assert!(matches("new tab", 0x52));
        assert!(!matches("tab", 0x53));
        assert!(matches("", 0x53));
        assert_eq!(fold("Clic Central Pestaña"), "clic central pestana");
    }

    #[test]
    fn labels() {
        assert_eq!(
            remap_label(RemapControl::Back, 0x52),
            "Back now does Middle click"
        );
        assert_eq!(
            remap_label(RemapControl::Back, 0x53),
            "Back does its own task again"
        );
        assert_eq!(action_name(0x1234), "Control 0x1234");
        assert_eq!(control(Hotspot::Wheel), Some(RemapControl::Middle));
        assert_eq!(control(Hotspot::Thumb), None);
    }
}
