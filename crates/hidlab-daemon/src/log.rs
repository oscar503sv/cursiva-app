//! Minimal logging to stderr. Under systemd (`JOURNAL_STREAM` set) each line
//! starts with an sd-daemon priority prefix such as `<4>`, which journald
//! turns into the message priority. Raw HID bytes are logged only at debug
//! (requirements §17).

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 3,
    Warn = 4,
    Info = 6,
    Debug = 7,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warning",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "error" => Some(Level::Error),
            "warn" | "warning" => Some(Level::Warn),
            "info" => Some(Level::Info),
            "debug" => Some(Level::Debug),
            _ => None,
        }
    }
}

static MAX: AtomicU8 = AtomicU8::new(Level::Info as u8);
static JOURNAL: AtomicBool = AtomicBool::new(false);

/// Reads `HIDLAB_LOG` (error, warn, info, debug; default info) and whether
/// stderr goes to the journal. Returns a warning for an unknown level.
pub fn init() -> Option<String> {
    JOURNAL.store(
        std::env::var_os("JOURNAL_STREAM").is_some(),
        Ordering::Relaxed,
    );
    let raw = std::env::var("HIDLAB_LOG").ok()?;
    match Level::parse(&raw) {
        Some(l) => {
            MAX.store(l as u8, Ordering::Relaxed);
            None
        }
        None => Some(format!(
            "HIDLAB_LOG={raw} is not a level (error, warn, info, debug); using info"
        )),
    }
}

pub fn enabled(level: Level) -> bool {
    level as u8 <= MAX.load(Ordering::Relaxed)
}

pub fn format_line(level: Level, journal: bool, msg: fmt::Arguments<'_>) -> String {
    let text = msg.to_string();
    // One record per line; continuation lines are indented so they stay readable.
    let text = text.replace('\n', "\n    ");
    if journal {
        text.lines()
            .map(|l| format!("<{}>{l}", level as u8))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        format!("hid-labd {}: {text}", level.label())
    }
}

pub fn write(level: Level, msg: fmt::Arguments<'_>) {
    if enabled(level) {
        eprintln!(
            "{}",
            format_line(level, JOURNAL.load(Ordering::Relaxed), msg)
        );
    }
}

#[macro_export]
macro_rules! error { ($($t:tt)*) => { $crate::log::write($crate::log::Level::Error, format_args!($($t)*)) } }
#[macro_export]
macro_rules! warn { ($($t:tt)*) => { $crate::log::write($crate::log::Level::Warn, format_args!($($t)*)) } }
#[macro_export]
macro_rules! info { ($($t:tt)*) => { $crate::log::write($crate::log::Level::Info, format_args!($($t)*)) } }
#[macro_export]
macro_rules! debug { ($($t:tt)*) => { $crate::log::write($crate::log::Level::Debug, format_args!($($t)*)) } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_every_journal_line() {
        assert_eq!(
            format_line(Level::Warn, true, format_args!("a\nb")),
            "<4>a\n<4>    b"
        );
        assert_eq!(
            format_line(Level::Info, false, format_args!("x")),
            "hid-labd info: x"
        );
        assert_eq!(Level::parse("warning"), Some(Level::Warn));
        assert_eq!(Level::parse("loud"), None);
    }
}
