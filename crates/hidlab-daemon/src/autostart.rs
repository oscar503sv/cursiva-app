//! Whether systemd starts the daemon at login: `hid-labd.service` enabled
//! in the user's systemd instance. Without it the daemon starts only when
//! the app or the CLI calls it (D-Bus activation), so settings are not
//! applied at login until then.
//!
//! The daemon runs `systemctl --user`, as the user, never as root.

use std::process::Command;

use crate::api::UNIT;

/// `systemctl --user is-enabled` output: `Some(true)` when systemd starts
/// the unit at login, `Some(false)` when it does not, `None` otherwise
/// (unit not installed, static, masked, ...).
pub fn parse_state(text: &str) -> Option<bool> {
    match text.trim() {
        "enabled" | "enabled-runtime" => Some(true),
        "disabled" => Some(false),
        _ => None,
    }
}

/// The current state; `None` when systemd could not say.
pub fn read() -> Option<bool> {
    // `is-enabled` exits non-zero for `disabled`, so read stdout either way.
    let out = Command::new("systemctl")
        .args(["--user", "is-enabled", UNIT])
        .output()
        .ok()?;
    parse_state(&String::from_utf8_lossy(&out.stdout))
}

/// Enables or disables the unit for the next logins. It does not start or
/// stop the running daemon.
pub fn set(on: bool) -> Result<(), String> {
    let verb = if on { "enable" } else { "disable" };
    let out = Command::new("systemctl")
        .args(["--user", verb, UNIT])
        .output()
        .map_err(|e| format!("cannot run `systemctl`: {e}. Is this a systemd session?"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    Err(format!(
        "`systemctl --user {verb} {UNIT}` failed: {err}\n\
         Check that the unit is installed: systemctl --user cat {UNIT}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states() {
        assert_eq!(parse_state("enabled\n"), Some(true));
        assert_eq!(parse_state("disabled\n"), Some(false));
        assert_eq!(parse_state("static\n"), None);
        assert_eq!(parse_state(""), None);
    }
}
