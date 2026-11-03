//! `GetDiagnostics`: a plain-text report to attach to a bug report
//! (requirements §17). It holds no personal data: the home directory is
//! shown as `~`, the user and host names are replaced, Bluetooth addresses
//! are removed, and it never contains the mouse's unit ID (no device reads
//! beyond those of `GetStatus`).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use hidlab_transport::discover;

use crate::api::{DIAGNOSTICS_SCHEMA, UNIT};
use crate::engine::{Engine, access_label};
use crate::log;

/// Who and where, to remove from the report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Private {
    pub home: Option<String>,
    pub user: Option<String>,
    pub host: Option<String>,
}

impl Private {
    pub fn current() -> Private {
        let var = |n: &str| std::env::var(n).ok().filter(|v| !v.is_empty());
        Private {
            home: var("HOME"),
            user: var("USER").or_else(|| var("LOGNAME")),
            host: fs::read_to_string("/proc/sys/kernel/hostname")
                .ok()
                .map(|h| h.trim().to_owned())
                .filter(|h| !h.is_empty()),
        }
    }
}

/// Removes personal data from `text`. Names shorter than three characters
/// are left alone: replacing them would mangle ordinary words.
pub fn sanitize(text: &str, private: &Private) -> String {
    let mut out = text.to_owned();
    if let Some(home) = private.home.as_deref().filter(|h| h.len() > 1) {
        out = out.replace(home.trim_end_matches('/'), "~");
    }
    // The host first: it often contains the user name.
    if let Some(host) = private.host.as_deref().filter(|h| h.len() >= 3) {
        out = out.replace(host, "<host>");
    }
    if let Some(user) = private.user.as_deref().filter(|u| u.len() >= 3) {
        out = out.replace(user, "<user>");
    }
    redact_addresses(&out)
}

/// Replaces anything shaped like a Bluetooth address (`AA:BB:CC:DD:EE:FF`).
pub fn redact_addresses(text: &str) -> String {
    const LEN: usize = 17;
    let bytes = text.as_bytes();
    let is_addr = |w: &[u8]| {
        w.iter().enumerate().all(|(i, &b)| {
            if i % 3 == 2 {
                b == b':' || b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + LEN <= bytes.len()
            && is_addr(&bytes[i..i + LEN])
            && (i == 0 || !bytes[i - 1].is_ascii_hexdigit())
            && bytes.get(i + LEN).is_none_or(|b| !b.is_ascii_hexdigit())
        {
            out.push_str("<bt-address>");
            i += LEN;
        } else {
            let ch = text[i..].chars().next().expect("index is a char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn file_or_note(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(t) if t.trim().is_empty() => "(empty)\n".into(),
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "(no file)\n".into(),
        Err(e) => format!("(cannot read: {e})\n"),
    }
}

fn os_name() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| "unknown".into())
}

/// The whole report, sanitized.
pub fn report(engine: &Engine) -> String {
    let mut r = String::new();
    let env = |n: &str| std::env::var(n).unwrap_or_else(|_| "unset".into());
    let _ = writeln!(r, "{DIAGNOSTICS_SCHEMA}");
    let _ = writeln!(r, "hid-labd {}", env!("CARGO_PKG_VERSION"));
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let _ = writeln!(r, "kernel {}", kernel.trim());
    let _ = writeln!(r, "os {}", os_name());
    let _ = writeln!(
        r,
        "desktop {} ({})",
        env("XDG_CURRENT_DESKTOP"),
        env("XDG_SESSION_TYPE")
    );
    let start = match crate::autostart::read() {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    };
    let _ = writeln!(r, "{UNIT} starts at login: {start}");

    let _ = writeln!(r, "\n== Logitech hidraw devices ==");
    match discover() {
        Ok(d) => {
            let mut any = false;
            for dev in d.devices.iter().filter(|d| d.is_logitech()) {
                any = true;
                let u = &dev.uevent;
                let _ = writeln!(
                    r,
                    "{} bus {} {:04X}:{:04X} {} driver {} access {}",
                    dev.hidraw_name,
                    u.bus.label(),
                    u.vendor_id,
                    u.product_id,
                    u.name.as_deref().unwrap_or("(no name)"),
                    dev.driver.as_deref().unwrap_or("none"),
                    access_label(dev.access.status),
                );
            }
            if !any {
                let _ = writeln!(r, "(none)");
            }
        }
        Err(e) => {
            let _ = writeln!(r, "cannot list: {e}");
        }
    }

    let _ = writeln!(r, "\n== Status ==");
    let status = serde_json::to_string_pretty(&engine.status())
        .unwrap_or_else(|e| format!("cannot serialize: {e}"));
    let _ = writeln!(r, "{status}");

    let (config, state) = engine.paths();
    let _ = writeln!(r, "\n== Config file ==");
    r.push_str(&file_or_note(config));
    let _ = writeln!(r, "\n== State file ==");
    r.push_str(&file_or_note(state));

    let recent = log::recent();
    let _ = writeln!(r, "\n== Recent log (last {}) ==", recent.len());
    for line in recent {
        let _ = writeln!(r, "{line}");
    }
    sanitize(&r, &Private::current())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SYNTHETIC names and address.
    #[test]
    fn removes_personal_data() {
        let p = Private {
            home: Some("/home/alice".into()),
            user: Some("alice".into()),
            host: Some("alice-laptop".into()),
        };
        let text = "config /home/alice/.config/hid-lab/config.toml on alice-laptop by alice, \
                    uniq 12:34:56:ab:cd:ef, id 0005:046D:B034.0005";
        assert_eq!(
            sanitize(text, &p),
            "config ~/.config/hid-lab/config.toml on <host> by <user>, \
             uniq <bt-address>, id 0005:046D:B034.0005"
        );
    }

    #[test]
    fn short_names_are_left_alone() {
        let p = Private {
            home: None,
            user: Some("al".into()),
            host: None,
        };
        assert_eq!(sanitize("total", &p), "total");
    }

    #[test]
    fn addresses_need_exact_shape() {
        assert_eq!(redact_addresses("AA-BB-CC-DD-EE-FF."), "<bt-address>.");
        assert_eq!(redact_addresses("1AA:BB:CC:DD:EE:FF"), "1AA:BB:CC:DD:EE:FF");
        assert_eq!(redact_addresses("ñ 00:11:22:33:44:55"), "ñ <bt-address>");
        assert_eq!(redact_addresses("0005:046D:B034"), "0005:046D:B034");
    }
}
