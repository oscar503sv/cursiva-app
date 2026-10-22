//! Transport errors with actionable messages.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Commands that install the hid-lab udev rule from a source checkout.
pub const UDEV_INSTALL_HINT: &str = "\
  sudo cp packaging/udev/70-hid-lab-logitech.rules /etc/udev/rules.d/
  sudo udevadm control --reload
  sudo udevadm trigger --subsystem-match=hidraw --action=change
Then turn the mouse off and on, and run the same hid-lab command again
as your normal user (hid-lab itself never needs sudo).";

/// Errors raised by discovery and raw I/O.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error(
        "Cannot open {} (permission denied).\n\
         The hid-lab udev rule is probably not installed or not yet applied.\n\
         Try, from the hid-lab source directory:\n{UDEV_INSTALL_HINT}",
        node.display()
    )]
    PermissionDenied { node: PathBuf },

    #[error(
        "The device at {} disconnected.\n\
         This is normal when a Bluetooth mouse sleeps or switches channel.\n\
         Try: move the mouse to wake it, then run `hid-lab lab devices` to find its new node.",
        node.display()
    )]
    Disconnected { node: PathBuf },

    #[error(
        "{} does not exist.\n\
         The device may have reconnected under a different hidraw number.\n\
         Try: `hid-lab lab devices` to list the current nodes.",
        node.display()
    )]
    NotFound { node: PathBuf },

    #[error("I/O error on {}: {source}", node.display())]
    Io {
        node: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error(
        "Cannot read {}: {reason}.\n\
         Try: check that sysfs is mounted at /sys and that the hidraw module is loaded (`lsmod | grep hid`).",
        path.display()
    )]
    Sysfs { path: PathBuf, reason: String },
}

impl TransportError {
    /// Maps an `io::Error` from opening or using `node` to a typed error.
    pub fn from_io(node: impl Into<PathBuf>, err: io::Error) -> Self {
        let node = node.into();
        match err.raw_os_error() {
            Some(libc::EACCES) | Some(libc::EPERM) => TransportError::PermissionDenied { node },
            Some(libc::ENOENT) => TransportError::NotFound { node },
            Some(libc::ENODEV) | Some(libc::ENXIO) => TransportError::Disconnected { node },
            _ if err.kind() == io::ErrorKind::PermissionDenied => {
                TransportError::PermissionDenied { node }
            }
            _ if err.kind() == io::ErrorKind::NotFound => TransportError::NotFound { node },
            _ => TransportError::Io { node, source: err },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_errno_values() {
        let p = "/dev/hidraw9";
        assert!(matches!(
            TransportError::from_io(p, io::Error::from_raw_os_error(libc::EACCES)),
            TransportError::PermissionDenied { .. }
        ));
        assert!(matches!(
            TransportError::from_io(p, io::Error::from_raw_os_error(libc::ENOENT)),
            TransportError::NotFound { .. }
        ));
        assert!(matches!(
            TransportError::from_io(p, io::Error::from_raw_os_error(libc::ENODEV)),
            TransportError::Disconnected { .. }
        ));
        assert!(matches!(
            TransportError::from_io(p, io::Error::from_raw_os_error(libc::EINVAL)),
            TransportError::Io { .. }
        ));
    }

    #[test]
    fn permission_message_is_actionable() {
        let msg = TransportError::PermissionDenied {
            node: "/dev/hidraw2".into(),
        }
        .to_string();
        assert!(msg.contains("/dev/hidraw2"));
        assert!(msg.contains("udevadm trigger"));
        assert!(msg.contains("never needs sudo"));
    }
}
