//! Discovery of hidraw nodes through sysfs.
//!
//! Layout used (relative to a root, normally `/`):
//!
//! ```text
//! sys/class/hidraw/hidrawN/device/uevent             HID_ID, HID_NAME, HID_UNIQ
//! sys/class/hidraw/hidrawN/device/report_descriptor  raw descriptor bytes
//! sys/class/hidraw/hidrawN/device/driver -> .../<driver name>
//! dev/hidrawN                                        character device node
//! ```
//!
//! Devices are identified by bus/vendor/product, never by the hidraw number,
//! which changes across reconnects.

use std::fs;
use std::path::{Path, PathBuf};

use crate::descriptor::{DescriptorSummary, walk_descriptor};
use crate::error::TransportError;
use crate::hidraw::{AccessCheck, check_access};
use crate::uevent::{Uevent, parse_uevent};

/// Logitech USB vendor ID.
pub const LOGITECH_VENDOR_ID: u16 = 0x046D;

/// One hidraw node and what sysfs says about it.
#[derive(Debug, Clone)]
pub struct HidrawDevice {
    /// Kernel name, e.g. `hidraw2`.
    pub hidraw_name: String,
    /// Device node, e.g. `/dev/hidraw2`.
    pub node: PathBuf,
    /// HID device directory name, e.g. `0005:046D:B034.0003`, if resolvable.
    pub hid_device: Option<String>,
    pub uevent: Uevent,
    /// Bound kernel driver, e.g. `logitech-hidpp-device`.
    pub driver: Option<String>,
    /// Descriptor walk result, or `None` if it could not be read.
    pub descriptor: Option<DescriptorSummary>,
    /// Why the descriptor could not be read, if it could not.
    pub descriptor_error: Option<String>,
    pub access: AccessCheck,
}

impl HidrawDevice {
    /// True for Logitech vendor ID.
    pub fn is_logitech(&self) -> bool {
        self.uevent.vendor_id == LOGITECH_VENDOR_ID
    }

    /// True if the descriptor declares HID++ report IDs.
    pub fn looks_like_hidpp(&self) -> bool {
        self.descriptor
            .as_ref()
            .is_some_and(DescriptorSummary::looks_like_hidpp)
    }

    /// Numeric suffix of `hidrawN`, for ordering.
    pub fn hidraw_number(&self) -> Option<u32> {
        hidraw_number(&self.hidraw_name)
    }
}

/// A sysfs entry that could not be turned into a [`HidrawDevice`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedEntry {
    pub hidraw_name: String,
    pub reason: String,
}

/// Everything discovery found.
#[derive(Debug, Clone, Default)]
pub struct Discovery {
    /// Logitech first, then by hidraw number.
    pub devices: Vec<HidrawDevice>,
    pub skipped: Vec<SkippedEntry>,
}

/// Discovers hidraw devices on the running system.
pub fn discover() -> Result<Discovery, TransportError> {
    discover_in(Path::new("/"))
}

/// Discovers hidraw devices under `root` (used by tests with a fake tree).
pub fn discover_in(root: &Path) -> Result<Discovery, TransportError> {
    let class_dir = root.join("sys/class/hidraw");
    let entries = fs::read_dir(&class_dir).map_err(|e| TransportError::Sysfs {
        path: class_dir.clone(),
        reason: e.to_string(),
    })?;

    let mut discovery = Discovery::default();
    for entry in entries {
        let entry = entry.map_err(|e| TransportError::Sysfs {
            path: class_dir.clone(),
            reason: e.to_string(),
        })?;
        let hidraw_name = entry.file_name().to_string_lossy().into_owned();
        if hidraw_number(&hidraw_name).is_none() {
            continue;
        }
        match read_device(root, &entry.path(), &hidraw_name) {
            Ok(dev) => discovery.devices.push(dev),
            Err(reason) => discovery.skipped.push(SkippedEntry {
                hidraw_name,
                reason,
            }),
        }
    }

    discovery.devices.sort_by_key(|d| {
        (
            !d.is_logitech(),
            d.hidraw_number().unwrap_or(u32::MAX),
            d.hidraw_name.clone(),
        )
    });
    discovery
        .skipped
        .sort_by(|a, b| a.hidraw_name.cmp(&b.hidraw_name));
    Ok(discovery)
}

fn read_device(root: &Path, class_entry: &Path, hidraw_name: &str) -> Result<HidrawDevice, String> {
    let device_dir = class_entry.join("device");
    let uevent_path = device_dir.join("uevent");
    let uevent_text = fs::read_to_string(&uevent_path)
        .map_err(|e| format!("cannot read {}: {e}", uevent_path.display()))?;
    let uevent = parse_uevent(&uevent_text).map_err(|e| e.to_string())?;

    let hid_device = fs::canonicalize(&device_dir)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));

    let driver = fs::read_link(device_dir.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));

    let (descriptor, descriptor_error) = match fs::read(device_dir.join("report_descriptor")) {
        Ok(bytes) => (Some(walk_descriptor(&bytes)), None),
        Err(e) => (None, Some(e.to_string())),
    };

    let node = root.join("dev").join(hidraw_name);
    let access = check_access(&node);

    Ok(HidrawDevice {
        hidraw_name: hidraw_name.to_owned(),
        node,
        hid_device,
        uevent,
        driver,
        descriptor,
        descriptor_error,
        access,
    })
}

fn hidraw_number(name: &str) -> Option<u32> {
    name.strip_prefix("hidraw")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hidraw::Access;
    use crate::uevent::Bus;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A temporary directory removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let p = std::env::temp_dir().join(format!(
                "hidlab-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // SYNTHETIC descriptor with HID++ report IDs 0x10 and 0x11.
    const HIDPP_DESC: &[u8] = &[0x06, 0x00, 0xFF, 0x85, 0x10, 0x85, 0x11];
    // SYNTHETIC descriptor of a plain keyboard (report ID 1).
    const KBD_DESC: &[u8] = &[0x05, 0x01, 0x09, 0x06, 0x85, 0x01];

    fn add_device(
        root: &Path,
        hidraw: &str,
        hid_dir: &str,
        uevent: &str,
        driver: Option<&str>,
        descriptor: Option<&[u8]>,
        create_node: bool,
    ) {
        let real = root.join("sys/devices/virtual").join(hid_dir);
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("uevent"), uevent).unwrap();
        if let Some(d) = descriptor {
            fs::write(real.join("report_descriptor"), d).unwrap();
        }
        if let Some(drv) = driver {
            let drv_dir = root.join("sys/bus/hid/drivers").join(drv);
            fs::create_dir_all(&drv_dir).unwrap();
            symlink(&drv_dir, real.join("driver")).unwrap();
        }
        let class = root.join("sys/class/hidraw").join(hidraw);
        fs::create_dir_all(&class).unwrap();
        symlink(&real, class.join("device")).unwrap();
        if create_node {
            fs::create_dir_all(root.join("dev")).unwrap();
            fs::write(root.join("dev").join(hidraw), b"").unwrap();
        }
    }

    #[test]
    fn discovers_fake_tree_logitech_first() {
        let tmp = TempDir::new();
        let root = &tmp.0;
        add_device(
            root,
            "hidraw0",
            "0003:1234:5678.0001",
            "HID_ID=0003:00001234:00005678\nHID_NAME=Other Keyboard\n",
            Some("hid-generic"),
            Some(KBD_DESC),
            true,
        );
        add_device(
            root,
            "hidraw2",
            "0005:046D:B034.0003",
            "HID_ID=0005:0000046D:0000B034\nHID_NAME=Logitech MX Master 3S\nHID_UNIQ=aa:bb:cc:dd:ee:ff\n",
            Some("logitech-hidpp-device"),
            Some(HIDPP_DESC),
            true,
        );
        add_device(
            root,
            "hidraw7",
            "0003:046D:C548.0007",
            "HID_ID=0003:0000046D:0000C548\nHID_NAME=Logitech USB Receiver\n",
            None,
            None,
            false,
        );
        // Broken entry: uevent without HID_ID.
        add_device(root, "hidraw9", "x", "FOO=bar\n", None, None, false);
        // Not a hidraw entry at all.
        fs::create_dir_all(root.join("sys/class/hidraw/README")).unwrap();

        let d = discover_in(root).unwrap();
        let names: Vec<_> = d.devices.iter().map(|d| d.hidraw_name.as_str()).collect();
        assert_eq!(names, ["hidraw2", "hidraw7", "hidraw0"]);

        let mx = &d.devices[0];
        assert_eq!(mx.uevent.bus, Bus::Bluetooth);
        assert_eq!(mx.hid_device.as_deref(), Some("0005:046D:B034.0003"));
        assert_eq!(mx.driver.as_deref(), Some("logitech-hidpp-device"));
        assert!(mx.looks_like_hidpp());
        assert_eq!(mx.access.status, Access::Ok);
        assert_eq!(mx.node, root.join("dev/hidraw2"));

        let rx = &d.devices[1];
        assert!(rx.descriptor.is_none());
        assert!(rx.descriptor_error.is_some());
        assert!(!rx.looks_like_hidpp());
        assert_eq!(rx.access.status, Access::Missing);
        assert_eq!(rx.driver, None);

        assert!(!d.devices[2].looks_like_hidpp());

        assert_eq!(d.skipped.len(), 1);
        assert_eq!(d.skipped[0].hidraw_name, "hidraw9");
    }

    #[test]
    fn missing_sysfs_class_is_a_typed_error() {
        let tmp = TempDir::new();
        assert!(matches!(
            discover_in(&tmp.0),
            Err(TransportError::Sysfs { .. })
        ));
    }

    #[test]
    fn hidraw_number_parsing() {
        assert_eq!(hidraw_number("hidraw12"), Some(12));
        assert_eq!(hidraw_number("hidrawX"), None);
        assert_eq!(hidraw_number("event3"), None);
    }
}
