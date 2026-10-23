//! hidraw hotplug events from udev.
//!
//! udev delivers an event after its rules ran, so on `add` the `uaccess`
//! ACL is already applied and the node can be opened. Bus, vendor and
//! product are taken from the HID device directory name in the event's
//! sysfs path (`.../0005:046D:B034.0003/hidraw/hidraw2`), which works for
//! `remove` too, when sysfs is already gone.

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::TransportError;
use crate::hidraw::{AccessCheck, check_access};
use crate::uevent::Bus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HotplugAction {
    Add,
    Remove,
    Change,
    Other,
}

/// Bus, vendor and product parsed from a HID device directory name
/// `BBBB:VVVV:PPPP.NNNN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HidIds {
    pub bus: Bus,
    pub vendor_id: u16,
    pub product_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HotplugEvent {
    pub action: HotplugAction,
    /// e.g. `hidraw2`
    pub hidraw_name: String,
    /// e.g. `/dev/hidraw2`, if udev reported it.
    pub node: Option<PathBuf>,
    /// e.g. `0005:046D:B034.0003`
    pub hid_device: Option<String>,
    pub ids: Option<HidIds>,
    /// Access check right after an `add`; `None` for other actions.
    pub access: Option<AccessCheck>,
}

impl HotplugEvent {
    pub fn is_logitech(&self) -> bool {
        self.ids
            .is_some_and(|i| i.vendor_id == crate::discover::LOGITECH_VENDOR_ID)
    }
}

/// Finds the HID device directory in a hidraw sysfs path and parses it.
pub fn parse_hid_device_dir(syspath: &Path) -> Option<(String, HidIds)> {
    // Kernel HID device names are exactly `%04X:%04X:%04X.%04X`.
    fn hex4(s: &str) -> Option<u16> {
        (s.len() == 4 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| u16::from_str_radix(s, 16).ok())
            .flatten()
    }
    syspath.iter().find_map(|c| {
        let name = c.to_str()?;
        let (ids, instance) = name.split_once('.')?;
        hex4(instance)?;
        let mut it = ids.split(':');
        let bus = hex4(it.next()?)?;
        let vendor_id = hex4(it.next()?)?;
        let product_id = hex4(it.next()?)?;
        if it.next().is_some() {
            return None;
        }
        Some((
            name.to_owned(),
            HidIds {
                bus: Bus::from_code(bus),
                vendor_id,
                product_id,
            },
        ))
    })
}

/// Listens to udev for hidraw add/remove/change events.
pub struct HotplugMonitor {
    socket: udev::MonitorSocket,
}

fn udev_err(e: std::io::Error) -> TransportError {
    TransportError::Udev {
        reason: e.to_string(),
    }
}

impl HotplugMonitor {
    pub fn new() -> Result<Self, TransportError> {
        let socket = udev::MonitorBuilder::new()
            .and_then(|b| b.match_subsystem("hidraw"))
            .and_then(|b| b.listen())
            .map_err(udev_err)?;
        Ok(Self { socket })
    }

    /// The next event already received, without waiting. Use with
    /// [`AsRawFd`] in a caller's own `poll`.
    pub fn try_next(&mut self) -> Option<HotplugEvent> {
        self.socket.iter().next().map(|ev| convert(&ev))
    }

    /// Waits up to `timeout` for the next hidraw event.
    pub fn next_event(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<HotplugEvent>, TransportError> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(ev) = self.socket.iter().next() {
                return Ok(Some(convert(&ev)));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let ms = remaining.as_micros().div_ceil(1000).min(i32::MAX as u128) as i32;
            let mut pfd = libc::pollfd {
                fd: self.socket.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: valid pollfd, count 1; the socket outlives the call.
            let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
            if rc < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(udev_err(err));
            }
            if rc == 0 {
                return Ok(None);
            }
        }
    }
}

impl AsRawFd for HotplugMonitor {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.socket.as_raw_fd()
    }
}

fn convert(ev: &udev::Event) -> HotplugEvent {
    let action = match ev.event_type() {
        udev::EventType::Add => HotplugAction::Add,
        udev::EventType::Remove => HotplugAction::Remove,
        udev::EventType::Change => HotplugAction::Change,
        _ => HotplugAction::Other,
    };
    let parsed = parse_hid_device_dir(ev.syspath());
    let node = ev.devnode().map(Path::to_path_buf);
    let access = match (action, &node) {
        (HotplugAction::Add, Some(n)) => Some(check_access(n)),
        _ => None,
    };
    HotplugEvent {
        action,
        hidraw_name: ev.sysname().to_string_lossy().into_owned(),
        node,
        hid_device: parsed.as_ref().map(|(n, _)| n.clone()),
        ids: parsed.map(|(_, i)| i),
        access,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bluetooth_and_usb_paths() {
        let (name, ids) = parse_hid_device_dir(Path::new(
            "/sys/devices/virtual/misc/uhid/0005:046D:B034.0003/hidraw/hidraw2",
        ))
        .unwrap();
        assert_eq!(name, "0005:046D:B034.0003");
        assert_eq!(ids.bus, Bus::Bluetooth);
        assert_eq!((ids.vendor_id, ids.product_id), (0x046D, 0xB034));

        let (_, ids) = parse_hid_device_dir(Path::new(
            "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.2/0003:046D:C548.000A/hidraw/hidraw5",
        ))
        .unwrap();
        assert_eq!(ids.bus, Bus::Usb);
        assert_eq!(ids.product_id, 0xC548);
    }

    #[test]
    fn rejects_paths_without_hid_dir() {
        assert!(parse_hid_device_dir(Path::new("/sys/devices/pci0000:00/0000:00:14.0")).is_none());
        assert!(parse_hid_device_dir(Path::new("/sys/class/hidraw/hidraw2")).is_none());
    }
}
