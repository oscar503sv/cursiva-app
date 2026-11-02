//! GNOME pointer settings the app shows next to the mouse settings. They
//! belong to GNOME (`org.gnome.desktop.peripherals.mouse`), which stores
//! them and applies them to every mouse; the daemon only reads and writes
//! them for the app, so a sandboxed app needs no access to dconf
//! (requirements, decision log 2026-11-02).
//!
//! The daemon runs the `gsettings` command rather than linking GIO.

use std::process::Command;

use crate::api::{DesktopSetting, DesktopSettings};

pub const SCHEMA: &str = "org.gnome.desktop.peripherals.mouse";
const PROGRAM: &str = "gsettings";

/// GSettings key in [`SCHEMA`].
pub fn key(setting: DesktopSetting) -> &'static str {
    match setting {
        DesktopSetting::NaturalScroll => "natural-scroll",
        DesktopSetting::Acceleration => "accel-profile",
    }
}

/// The GVariant text to write for a user value.
pub fn to_gvariant(setting: DesktopSetting, value: &str) -> Result<String, String> {
    setting.check_value(value)?;
    Ok(match setting {
        DesktopSetting::NaturalScroll => (value == "on").to_string(),
        DesktopSetting::Acceleration => format!("'{value}'"),
    })
}

fn gsettings(args: &[&str]) -> Result<String, String> {
    let out = Command::new(PROGRAM).args(args).output().map_err(|e| {
        format!(
            "cannot run `{PROGRAM}`: {e}\n\
             It comes with GLib; on Fedora: sudo dnf install glib2"
        )
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(format!(
            "`{PROGRAM} {}` failed: {err}\n\
             These settings need GNOME (schema {SCHEMA}).",
            args.join(" ")
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// `gsettings get` output for a boolean key.
pub fn parse_bool(text: &str) -> Option<bool> {
    match text {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `gsettings get` output for an enum key: `'adaptive'` -> `adaptive`.
pub fn parse_enum(text: &str) -> Option<String> {
    text.strip_prefix('\'')
        .and_then(|t| t.strip_suffix('\''))
        .filter(|t| !t.is_empty() && !t.contains('\''))
        .map(str::to_owned)
}

/// Reads the GNOME pointer settings. Problems are reported in `error`.
pub fn read() -> DesktopSettings {
    let get = |s: DesktopSetting| gsettings(&["get", SCHEMA, key(s)]);
    let mut error = None;
    let natural_scroll = match get(DesktopSetting::NaturalScroll) {
        Ok(t) => parse_bool(&t),
        Err(e) => {
            error = Some(e);
            None
        }
    };
    let acceleration = match get(DesktopSetting::Acceleration) {
        Ok(t) => parse_enum(&t),
        Err(e) => {
            error.get_or_insert(e);
            None
        }
    };
    DesktopSettings {
        natural_scroll,
        acceleration,
        error,
    }
}

/// Writes one GNOME pointer setting; GNOME applies it at once.
pub fn set(setting: DesktopSetting, value: &str) -> Result<(), String> {
    let v = to_gvariant(setting, value)?;
    gsettings(&["set", SCHEMA, key(setting), &v]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gsettings_output() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("maybe"), None);
        assert_eq!(parse_enum("'adaptive'").as_deref(), Some("adaptive"));
        assert_eq!(parse_enum("adaptive"), None);
        assert_eq!(parse_enum("''"), None);
    }

    #[test]
    fn checks_values_before_running_anything() {
        let n = DesktopSetting::NaturalScroll;
        assert_eq!(to_gvariant(n, "on").unwrap(), "true");
        assert!(to_gvariant(n, "true").is_err());
        let a = DesktopSetting::Acceleration;
        assert_eq!(to_gvariant(a, "flat").unwrap(), "'flat'");
        assert!(to_gvariant(a, "'flat'; rm").is_err());
        assert_eq!(DesktopSetting::from_name("natural-scroll"), Some(n));
        assert_eq!(DesktopSetting::from_name("accel-profile"), None);
    }
}
