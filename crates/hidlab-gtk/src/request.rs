//! What the user asks for on a page. The window turns each request into
//! daemon calls with `crate::changes`.

use std::rc::Rc;

use gettextrs::gettext;
use hidlab_api::remap::RemapControl;
use hidlab_api::settings::SettingKind;

use crate::changes::WheelMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Dpi(u16),
    Threshold(u8),
    WheelMode(WheelMode),
    Invert(bool),
    NaturalScroll(bool),
    Acceleration(&'static str),
    /// Give a button another button's task, by control ID.
    Remap(RemapControl, u16),
    /// A section's "Restore Default".
    Reset(Section),
}

/// A part of a page with its own "Restore Default".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Pointer,
    Wheel,
    Thumb,
    Buttons,
}

impl Section {
    /// Mouse settings the section's "Restore Default" resets. GNOME
    /// settings are not reset: they are shared with every mouse.
    pub fn kinds(self) -> &'static [SettingKind] {
        match self {
            Section::Pointer => &[SettingKind::Dpi],
            Section::Wheel => &[
                SettingKind::SmartshiftMode,
                SettingKind::SmartshiftThreshold,
            ],
            Section::Thumb => &[SettingKind::ThumbwheelInvert],
            Section::Buttons => &[
                SettingKind::ButtonRemap(RemapControl::Middle),
                SettingKind::ButtonRemap(RemapControl::ModeButton),
                SettingKind::ButtonRemap(RemapControl::Forward),
                SettingKind::ButtonRemap(RemapControl::Back),
                SettingKind::ButtonRemap(RemapControl::Gesture),
            ],
        }
    }

    pub fn reset_label(self) -> String {
        match self {
            Section::Pointer => gettext("Pointer speed restored"),
            Section::Wheel => gettext("Main wheel restored"),
            Section::Thumb => gettext("Thumb wheel restored"),
            Section::Buttons => gettext("Every button does its own task again"),
        }
    }
}

/// Where a page sends its requests.
pub type Sink = Rc<dyn Fn(Request)>;
