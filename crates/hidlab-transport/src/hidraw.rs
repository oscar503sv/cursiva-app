//! Raw report I/O on `/dev/hidraw*`.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::TransportError;

/// A bidirectional raw HID report channel.
///
/// Reports include the report ID as their first byte, as with hidraw.
pub trait RawChannel {
    /// Sends one complete report.
    fn send(&mut self, report: &[u8]) -> Result<(), TransportError>;

    /// Waits up to `timeout` for one report. Returns `Ok(None)` on timeout.
    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> Result<Option<usize>, TransportError>;
}

/// An open `/dev/hidrawN` node.
#[derive(Debug)]
pub struct Hidraw {
    file: File,
    node: PathBuf,
}

impl Hidraw {
    /// Opens `node` for reading and writing.
    pub fn open(node: &Path) -> Result<Self, TransportError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(node)
            .map_err(|e| TransportError::from_io(node, e))?;
        Ok(Self {
            file,
            node: node.to_path_buf(),
        })
    }

    /// The path this channel was opened from.
    pub fn node(&self) -> &Path {
        &self.node
    }

    fn map_rw_error(&self, err: io::Error) -> TransportError {
        // After the device goes away, the kernel hidraw driver fails reads
        // and writes with EIO or ENODEV; both mean "disconnected" here.
        match err.raw_os_error() {
            Some(libc::EIO) | Some(libc::ENODEV) | Some(libc::EPIPE) => {
                TransportError::Disconnected {
                    node: self.node.clone(),
                }
            }
            _ => TransportError::from_io(&self.node, err),
        }
    }
}

impl RawChannel for Hidraw {
    fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
        loop {
            match self.file.write(report) {
                Ok(n) if n == report.len() => return Ok(()),
                Ok(n) => {
                    return Err(TransportError::Io {
                        node: self.node.clone(),
                        source: io::Error::new(
                            io::ErrorKind::WriteZero,
                            format!("short write: {n} of {} bytes", report.len()),
                        ),
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(self.map_rw_error(e)),
            }
        }
    }

    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> Result<Option<usize>, TransportError> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            // Round up so a sub-millisecond remainder still waits.
            let ms = remaining.as_micros().div_ceil(1000).min(i32::MAX as u128) as i32;
            let mut pfd = libc::pollfd {
                fd: self.file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: `pfd` is a valid, initialised pollfd and we pass a count of 1;
            // the fd stays open for the duration of the call because `self.file` is borrowed.
            let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
            if rc < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(self.map_rw_error(err));
            }
            if rc == 0 {
                return Ok(None);
            }
            if pfd.revents & libc::POLLIN == 0
                && pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
            {
                return Err(TransportError::Disconnected {
                    node: self.node.clone(),
                });
            }
            match self.file.read(buf) {
                Ok(0) => {
                    return Err(TransportError::Disconnected {
                        node: self.node.clone(),
                    });
                }
                Ok(n) => return Ok(Some(n)),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(self.map_rw_error(e)),
            }
        }
    }
}

/// Outcome of trying to open a node for read+write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Ok,
    PermissionDenied,
    Missing,
    Disconnected,
    Error,
}

/// Result of [`check_access`], with an optional detail for `Error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccessCheck {
    pub status: Access,
    pub detail: Option<String>,
}

/// Tries to open `node` read+write and immediately closes it.
pub fn check_access(node: &Path) -> AccessCheck {
    match Hidraw::open(node) {
        Ok(_) => AccessCheck {
            status: Access::Ok,
            detail: None,
        },
        Err(TransportError::PermissionDenied { .. }) => AccessCheck {
            status: Access::PermissionDenied,
            detail: None,
        },
        Err(TransportError::NotFound { .. }) => AccessCheck {
            status: Access::Missing,
            detail: None,
        },
        Err(TransportError::Disconnected { .. }) => AccessCheck {
            status: Access::Disconnected,
            detail: None,
        },
        Err(e) => AccessCheck {
            status: Access::Error,
            detail: Some(e.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_node_is_typed() {
        let p = Path::new("/nonexistent/hid-lab/hidraw99");
        assert!(matches!(
            Hidraw::open(p),
            Err(TransportError::NotFound { .. })
        ));
        assert_eq!(check_access(p).status, Access::Missing);
    }

    #[test]
    fn poll_timeout_and_read_on_a_pipe_like_file() {
        // /dev/null: readable immediately with 0 bytes -> reported as disconnected,
        // which exercises the poll/read path without hardware.
        let mut h = Hidraw::open(Path::new("/dev/null")).unwrap();
        let mut buf = [0u8; 20];
        let r = h.recv(&mut buf, Duration::from_millis(10));
        assert!(matches!(r, Err(TransportError::Disconnected { .. })));
        h.send(&[0x10, 0xFF, 0x00, 0x1B, 0, 0, 0]).unwrap();
    }
}
