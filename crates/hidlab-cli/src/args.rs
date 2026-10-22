//! Hand-written argument parser (no external dependency needed yet).

use std::time::Duration;

pub const TOP_HELP: &str = "\
hid-lab — configure Logitech HID++ mice on Linux

Usage:
  hid-lab lab devices [--json] [--show-unique]
  hid-lab lab features [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
  hid-lab --help | --version

Run `hid-lab lab <command> --help` for details.
Lab commands only send read-only queries. No device setting is changed.
";

pub const LAB_HELP: &str = "\
hid-lab lab — developer tools (read-only)

Commands:
  devices    List hidraw devices, Logitech first
  features   List the HID++ features a device reports

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    Top,
    Lab,
    Devices,
    Features,
}

impl HelpTopic {
    pub fn text(self) -> &'static str {
        match self {
            HelpTopic::Top => TOP_HELP,
            HelpTopic::Lab => LAB_HELP,
            HelpTopic::Devices => DEVICES_HELP,
            HelpTopic::Features => FEATURES_HELP,
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
pub enum Command {
    Help(HelpTopic),
    Version,
    Devices(DevicesArgs),
    Features(FeaturesArgs),
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
            Some("features") => parse_features(it.collect()),
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

fn parse_features(rest: Vec<&str>) -> Result<Command, String> {
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
                .ok_or_else(|| format!("{name} needs a value. Try: hid-lab lab features --help"))
        };
        match flag {
            "-h" | "--help" => return Ok(Command::Help(HelpTopic::Features)),
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
                    "unknown option `{other}`. Try: hid-lab lab features --help"
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
    Ok(Command::Features(out))
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
    fn features_errors() {
        assert!(parse(["lab", "features", "--index"]).is_err());
        assert!(parse(["lab", "features", "--index", "300"]).is_err());
        assert!(parse(["lab", "features", "--timeout-ms", "0"]).is_err());
        assert!(parse(["lab", "features", "a", "b"]).is_err());
        assert!(parse(["lab", "features", "--bogus"]).is_err());
        assert!(parse(["nope"]).is_err());
    }
}
