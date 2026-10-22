//! Device selection shared by lab commands.

use std::path::Path;

use hidlab_transport::{Discovery, HidrawDevice};

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
