//! Hand-written argument parser (no external dependency needed yet).

use std::time::Duration;

pub const TOP_HELP: &str = "\
hid-lab — configure Logitech HID++ mice on Linux

Usage:
  hid-lab lab devices [--json] [--show-unique]
  hid-lab lab features [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
  hid-lab lab read [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
  hid-lab lab watch [<device>] [--duration <s>] [--json] [--features-from <file>]
  hid-lab --help | --version

Run `hid-lab lab <command> --help` for details.
Lab commands only send read-only queries. No device setting is changed.
";

pub const LAB_HELP: &str = "\
hid-lab lab — developer tools (read-only)

Commands:
  devices    List hidraw devices, Logitech first
  features   List the HID++ features a device reports
  read       Read battery, DPI, wheels, controls, host, firmware and name
  watch      Listen passively and show HID++ notifications

Lab commands only send read-only queries. No device setting is changed.
";

pub const DEVICES_HELP: &str = "\
hid-lab lab devices — list hidraw devices

Usage: hid-lab lab devices [--json] [--show-unique]

Reads /sys/class/hidraw and shows, for each node: bus, vendor/product ID,
kernel driver, whether its report descriptor declares HID++ reports
(0x10/0x11), and whether you can open it.

Options:
  --json          Machine-readable output with stable field names
  --show-unique   Show HID_UNIQ (Bluetooth address / serial); redacted by default

This command sends nothing to any device.
";

pub const FEATURES_HELP: &str = "\
hid-lab lab features — list HID++ 2.0 features of a device

Usage: hid-lab lab features [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]

<device> may be `hidrawN`, `/dev/hidrawN`, or the `#` shown by
`hid-lab lab devices`. If omitted, the only Logitech HID++ device is used.

Options:
  --json            Machine-readable output, including the raw TX/RX transcript
  --index <n>       HID++ device index: 255 (0xff, default) for Bluetooth or
                    wired devices; 1..6 for a device paired to a receiver
  --timeout-ms <ms> Per-request timeout (default 2000)
  --show-unique     Show HID_UNIQ in --json output; redacted by default

Read-only: sends only getProtocolVersion, getFeature, getCount and
getFeatureID queries. No device setting is changed.
";

pub const READ_HELP: &str = "\
hid-lab lab read — read the current state of known HID++ features

Usage: hid-lab lab read [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]

Reads, if the device reports them: device name (0x0005), firmware (0x0003),
battery (0x1004, compared with the kernel's power_supply capacity), host
channel (0x1814), reprogrammable controls (0x1B04), DPI (0x2201),
SmartShift (0x2110), hi-res wheel (0x2121) and thumb wheel (0x2150).

<device> is selected as in `hid-lab lab features`.

Options:
  --json            Machine-readable output, including the raw TX/RX transcript
  --index <n>       HID++ device index (default 255 / 0xff)
  --timeout-ms <ms> Per-request timeout (default 2000)
  --show-unique     Show the firmware unit ID (a serial number); redacted by default,
                    also in the transcript

Read-only: only functions in a fixed allowlist of documented read
functions are sent. No device setting is changed.
";

pub const WATCH_HELP: &str = "\
hid-lab lab watch — listen for HID++ notifications without sending anything

Usage: hid-lab lab watch [<device>] [--duration <s>] [--json] [--features-from <file>]

Shows every HID++ report with software ID 0 (a device notification): time,
feature index and function, with the remaining bytes in hex. Mouse movement
and other input reports are only counted. HID++ traffic of other programs
(for example the kernel driver) is counted and kept in --json output.

Options:
  --duration <s>          How long to listen, 1..=3600 seconds (default 30)
  --json                  Machine-readable output at the end
  --features-from <file>  Output of `hid-lab lab features --json` for the same
                          device, used to name feature indices. Without it,
                          only indices are shown, because resolving names
                          would require sending requests.

Passive: this command never writes to the device.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    Top,
    Lab,
    Devices,
    Features,
    Read,
    Watch,
}

impl HelpTopic {
    pub fn text(self) -> &'static str {
        match self {
            HelpTopic::Top => TOP_HELP,
            HelpTopic::Lab => LAB_HELP,
            HelpTopic::Devices => DEVICES_HELP,
            HelpTopic::Features => FEATURES_HELP,
            HelpTopic::Read => READ_HELP,
            HelpTopic::Watch => WATCH_HELP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicesArgs {
    pub json: bool,
    pub show_unique: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeaturesArgs {
    pub device: Option<String>,
    pub json: bool,
    pub show_unique: bool,
    pub index: Option<u8>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchArgs {
    pub device: Option<String>,
    pub duration: Duration,
    pub json: bool,
    pub features_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help(HelpTopic),
    Version,
    Devices(DevicesArgs),
    Features(FeaturesArgs),
    Read(FeaturesArgs),
    Watch(WatchArgs),
}

/// Parses arguments (without the program name). `Err` is a usage message.
pub fn parse<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut it = args.iter().map(String::as_str);

    match it.next() {
        None | Some("-h" | "--help" | "help") => Ok(Command::Help(HelpTopic::Top)),
        Some("-V" | "--version") => Ok(Command::Version),
        Some("lab") => match it.next() {
            None | Some("-h" | "--help" | "help") => Ok(Command::Help(HelpTopic::Lab)),
            Some("devices") => parse_devices(it.collect()),
            Some("features") => parse_features(it.collect(), HelpTopic::Features),
            Some("read") => parse_features(it.collect(), HelpTopic::Read),
            Some("watch") => parse_watch(it.collect()),
            Some(other) => Err(format!(
                "unknown lab command `{other}`. Try: hid-lab lab --help"
            )),
        },
        Some(other) => Err(format!("unknown command `{other}`. Try: hid-lab --help")),
    }
}

/// Splits `--name=value` into `("--name", Some("value"))`.
fn split_flag(arg: &str) -> (&str, Option<&str>) {
    match arg.split_once('=') {
        Some((k, v)) if k.starts_with("--") => (k, Some(v)),
        _ => (arg, None),
    }
}

fn parse_devices(rest: Vec<&str>) -> Result<Command, String> {
    let mut out = DevicesArgs {
        json: false,
        show_unique: false,
    };
    for arg in rest {
        match arg {
            "-h" | "--help" => return Ok(Command::Help(HelpTopic::Devices)),
            "--json" => out.json = true,
            "--show-unique" => out.show_unique = true,
            other => {
                return Err(format!(
                    "unexpected argument `{other}`. Try: hid-lab lab devices --help"
                ));
            }
        }
    }
    Ok(Command::Devices(out))
}

fn parse_features(rest: Vec<&str>, topic: HelpTopic) -> Result<Command, String> {
    let cmd = if topic == HelpTopic::Read {
        "read"
    } else {
        "features"
    };
    let mut out = FeaturesArgs {
        device: None,
        json: false,
        show_unique: false,
        index: None,
        timeout: Duration::from_millis(2000),
    };
    let mut it = rest.into_iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = split_flag(arg);
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .map(str::to_owned)
                .or_else(|| it.next().map(str::to_owned))
                .ok_or_else(|| format!("{name} needs a value. Try: hid-lab lab {cmd} --help"))
        };
        match flag {
            "-h" | "--help" => return Ok(Command::Help(topic)),
            "--json" => out.json = true,
            "--show-unique" => out.show_unique = true,
            "--index" => out.index = Some(parse_u8(&value("--index")?)?),
            "--timeout-ms" => {
                let v = value("--timeout-ms")?;
                let ms: u64 = v
                    .parse()
                    .ok()
                    .filter(|&ms| (1..=60_000).contains(&ms))
                    .ok_or_else(|| format!("--timeout-ms must be 1..=60000, got `{v}`"))?;
                out.timeout = Duration::from_millis(ms);
            }
            other if other.starts_with('-') => {
                return Err(format!(
                    "unknown option `{other}`. Try: hid-lab lab {cmd} --help"
                ));
            }
            other => {
                if out.device.is_some() {
                    return Err(format!(
                        "more than one device given (`{other}`). Pass a single device."
                    ));
                }
                out.device = Some(other.to_owned());
            }
        }
    }
    Ok(if topic == HelpTopic::Read {
        Command::Read(out)
    } else {
        Command::Features(out)
    })
}

fn parse_watch(rest: Vec<&str>) -> Result<Command, String> {
    let mut out = WatchArgs {
        device: None,
        duration: Duration::from_secs(30),
        json: false,
        features_from: None,
    };
    let mut it = rest.into_iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = split_flag(arg);
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .map(str::to_owned)
                .or_else(|| it.next().map(str::to_owned))
                .ok_or_else(|| format!("{name} needs a value. Try: hid-lab lab watch --help"))
        };
        match flag {
            "-h" | "--help" => return Ok(Command::Help(HelpTopic::Watch)),
            "--json" => out.json = true,
            "--duration" => {
                let v = value("--duration")?;
                let s: u64 = v
                    .parse()
                    .ok()
                    .filter(|&s| (1..=3600).contains(&s))
                    .ok_or_else(|| format!("--duration must be 1..=3600 seconds, got `{v}`"))?;
                out.duration = Duration::from_secs(s);
            }
            "--features-from" => out.features_from = Some(value("--features-from")?),
            other if other.starts_with('-') => {
                return Err(format!(
                    "unknown option `{other}`. Try: hid-lab lab watch --help"
                ));
            }
            other => {
                if out.device.is_some() {
                    return Err(format!(
                        "more than one device given (`{other}`). Pass a single device."
                    ));
                }
                out.device = Some(other.to_owned());
            }
        }
    }
    Ok(Command::Watch(out))
}

fn parse_u8(v: &str) -> Result<u8, String> {
    let parsed = match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        Some(hex) => u8::from_str_radix(hex, 16).ok(),
        None => v.parse().ok(),
    };
    parsed.ok_or_else(|| format!("--index must be 0..=255 or 0x00..=0xff, got `{v}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_and_version() {
        assert_eq!(parse::<_, &str>([]), Ok(Command::Help(HelpTopic::Top)));
        assert_eq!(parse(["--version"]), Ok(Command::Version));
        assert_eq!(parse(["lab"]), Ok(Command::Help(HelpTopic::Lab)));
        assert_eq!(
            parse(["lab", "features", "--json", "--help"]),
            Ok(Command::Help(HelpTopic::Features))
        );
    }

    #[test]
    fn devices_flags() {
        assert_eq!(
            parse(["lab", "devices", "--json", "--show-unique"]),
            Ok(Command::Devices(DevicesArgs {
                json: true,
                show_unique: true
            }))
        );
        assert!(parse(["lab", "devices", "hidraw2"]).is_err());
    }

    #[test]
    fn features_flags() {
        let Ok(Command::Features(f)) = parse([
            "lab",
            "features",
            "/dev/hidraw2",
            "--index",
            "0x01",
            "--timeout-ms=500",
            "--json",
        ]) else {
            panic!()
        };
        assert_eq!(f.device.as_deref(), Some("/dev/hidraw2"));
        assert_eq!(f.index, Some(1));
        assert_eq!(f.timeout, Duration::from_millis(500));
        assert!(f.json);
    }

    #[test]
    fn read_and_watch() {
        let Ok(Command::Read(r)) = parse(["lab", "read", "2", "--show-unique"]) else {
            panic!()
        };
        assert_eq!(r.device.as_deref(), Some("2"));
        assert!(r.show_unique);
        assert_eq!(
            parse(["lab", "read", "-h"]),
            Ok(Command::Help(HelpTopic::Read))
        );

        let Ok(Command::Watch(w)) = parse([
            "lab",
            "watch",
            "--duration=5",
            "--features-from",
            "f.json",
            "--json",
        ]) else {
            panic!()
        };
        assert_eq!(w.duration, Duration::from_secs(5));
        assert_eq!(w.features_from.as_deref(), Some("f.json"));
        assert!(w.json);
        assert!(parse(["lab", "watch", "--duration", "0"]).is_err());
        assert!(parse(["lab", "watch", "--index", "1"]).is_err());
    }

    #[test]
    fn features_errors() {
        assert!(parse(["lab", "features", "--index"]).is_err());
        assert!(parse(["lab", "features", "--index", "300"]).is_err());
        assert!(parse(["lab", "features", "--timeout-ms", "0"]).is_err());
        assert!(parse(["lab", "features", "a", "b"]).is_err());
        assert!(parse(["lab", "features", "--bogus"]).is_err());
        assert!(parse(["nope"]).is_err());
    }
}
