//! Device selection and session setup shared by lab commands.

use std::path::Path;
use std::time::Duration;

use hidlab_hidpp::{DEVICE_INDEX_DIRECT, ReportKind, SessionConfig};
use hidlab_transport::{Discovery, HidrawDevice};

/// Session parameters for `device`: long reports when the descriptor
/// declares `0x11` (or cannot be read), short only if it declares only `0x10`.
pub fn session_config(
    device: &HidrawDevice,
    index: Option<u8>,
    timeout: Duration,
) -> SessionConfig {
    let report = match &device.descriptor {
        Some(d) if !d.has_long_report() && d.has_short_report() => ReportKind::Short,
        _ => ReportKind::Long,
    };
    SessionConfig {
        device_index: index.unwrap_or(DEVICE_INDEX_DIRECT),
        report,
        timeout,
        ..SessionConfig::default()
    }
}

/// Picks the target device from the user's selector.
pub fn select<'a>(
    d: &'a Discovery,
    selector: Option<&str>,
) -> Result<(usize, &'a HidrawDevice), String> {
    let numbered = || d.devices.iter().enumerate().map(|(i, dev)| (i + 1, dev));

    let Some(sel) = selector else {
        let candidates: Vec<_> = numbered()
            .filter(|(_, dev)| dev.is_logitech() && dev.looks_like_hidpp())
            .collect();
        return match candidates.as_slice() {
            [one] => Ok(*one),
            [] => Err("No Logitech HID++ device found.\n\
                 Likely cause: the mouse is off, asleep, or not connected.\n\
                 Try: move the mouse, then run `hid-lab lab devices` to see what is visible."
                .into()),
            many => {
                let list: Vec<String> = many
                    .iter()
                    .map(|(n, dev)| {
                        format!(
                            "  #{n} {} {}",
                            dev.node.display(),
                            dev.uevent.name.as_deref().unwrap_or("")
                        )
                    })
                    .collect();
                Err(format!(
                    "Several Logitech HID++ devices found; choose one:\n{}\n\
                     Try: hid-lab lab features <#|hidrawN>",
                    list.join("\n")
                ))
            }
        };
    };

    let found = if let Ok(n) = sel.parse::<usize>() {
        numbered().find(|(i, _)| *i == n)
    } else {
        let name = Path::new(sel)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        numbered().find(|(_, dev)| dev.hidraw_name == name)
    };
    found.ok_or_else(|| {
        format!(
            "No device matches `{sel}`.\n\
             The hidraw number may have changed after a reconnect.\n\
             Try: `hid-lab lab devices` and use the `#` or node it shows."
        )
    })
}
