//! Text for the problem screens: the udev commands to copy and the short
//! report the app writes when the service cannot be reached. Pure, so it
//! is tested without a display.

/// The rule the README installs, built into the app so the commands work
/// from any folder.
const UDEV_RULE: &str = include_str!("../../../packaging/udev/70-hid-lab-logitech.rules");
const UDEV_PATH: &str = "/etc/udev/rules.d/70-hid-lab-logitech.rules";

/// Commands that install the udev rule and apply it to the connected
/// mouse. The user runs them in a terminal; the app never runs sudo.
pub fn udev_commands() -> String {
    let rule: Vec<&str> = UDEV_RULE
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .collect();
    format!(
        "sudo tee {UDEV_PATH} > /dev/null <<'EOF'\n{}\nEOF\n\
         sudo udevadm control --reload\n\
         sudo udevadm trigger --subsystem-match=hidraw --action=change\n",
        rule.join("\n")
    )
}

/// What the app knows about the system, for [`app_report`].
pub struct System {
    pub kernel: String,
    pub os: String,
    pub desktop: String,
    pub home: Option<String>,
}

impl System {
    pub fn current() -> System {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let os = read("/etc/os-release")
            .lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME="))
            .map(|v| v.trim_matches('"').to_owned())
            .unwrap_or_else(|| "unknown".into());
        let env = |name: &str| std::env::var(name).unwrap_or_else(|_| "unknown".into());
        System {
            kernel: read("/proc/sys/kernel/osrelease").trim().to_owned(),
            os,
            desktop: format!(
                "{} ({})",
                env("XDG_CURRENT_DESKTOP"),
                env("XDG_SESSION_TYPE")
            ),
            home: std::env::var("HOME").ok().filter(|h| h.len() > 1),
        }
    }
}

/// A short report written by the app when `GetDiagnostics` fails, so the
/// user still has something to attach to an issue. In English, like the
/// daemon's report; the home folder is replaced with `~`.
pub fn app_report(error: &str, system: &System) -> String {
    let text = format!(
        "cursiva.app-report/1\n\
         The Cursiva service did not answer, so this report comes from the app.\n\n\
         cursiva: {}\n\
         kernel: {}\n\
         os: {}\n\
         desktop: {}\n\n\
         error:\n{error}\n\n\
         Next: run `systemctl --user status hid-labd.service` and \
         `journalctl --user -u hid-labd.service -n 50`, and attach their output.\n",
        env!("CARGO_PKG_VERSION"),
        system.kernel,
        system.os,
        system.desktop,
    );
    match &system.home {
        Some(home) => text.replace(home.as_str(), "~"),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udev_commands_carry_the_rule_without_comments() {
        let c = udev_commands();
        assert!(c.starts_with("sudo tee /etc/udev/rules.d/70-hid-lab-logitech.rules"));
        assert!(c.contains(r#"KERNELS=="0005:046D:*", TAG+="uaccess""#));
        assert!(c.contains("LABEL=\"hid_lab_logitech_end\"\nEOF\n"));
        assert!(!c.contains('#'));
        assert!(c.ends_with("--action=change\n"));
    }

    #[test]
    fn app_report_hides_the_home_folder() {
        // SYNTHETIC system data.
        let system = System {
            kernel: "7.0.0-test".into(),
            os: "Test OS".into(),
            desktop: "GNOME (wayland)".into(),
            home: Some("/home/someone".into()),
        };
        let r = app_report("cannot open /home/someone/.config/x", &system);
        assert!(r.starts_with("cursiva.app-report/1\n"));
        assert!(r.contains("cannot open ~/.config/x"));
        assert!(!r.contains("/home/someone"));
        assert!(r.contains("kernel: 7.0.0-test"));
    }
}
