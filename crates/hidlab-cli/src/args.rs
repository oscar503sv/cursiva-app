//! Hand-written argument parser (no external dependency needed yet).

use std::time::Duration;

pub const TOP_HELP: &str = "\
hid-lab — configure Logitech HID++ mice on Linux

Usage:
  hid-lab status [--json]
  hid-lab set <setting> <value> [--json]
  hid-lab reset (<setting> | all) [--json]
  hid-lab reapply [--json]
  hid-lab lab devices [--json] [--show-unique]
  hid-lab lab features [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
  hid-lab lab read [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
  hid-lab lab watch [<device>] [--duration <s>] [--json] [--features-from <file>]
  hid-lab lab hotplug [--duration <s>] [--json]
  hid-lab lab write <setting> <value> [<device>] (--dry-run | --write) [--json]
  hid-lab lab write <setting> --restore [<device>] (--dry-run | --write) [--json]
  hid-lab --help | --version

status, set, reset and reapply talk to the daemon hid-labd (docs/daemon.md).
Settings: dpi <n>, smartshift-mode <ratchet|free-spin>,
smartshift-threshold <1..50|off|on>, thumbwheel-invert <on|off>,
remap-<button> <target|default>; GNOME: natural-scroll <on|off>,
acceleration <default|flat|adaptive>. See `hid-lab set --help`.

Run `hid-lab <command> --help` or `hid-lab lab <command> --help` for details.
All lab commands are read-only except `lab write`, which needs --write.
";

pub const STATUS_HELP: &str = "\
hid-lab status — show what the daemon applies and what the mouse has

Usage: hid-lab status [--json]

Asks hid-labd over D-Bus. The current values are read from the mouse for
this call; nothing is written.
";

pub const SET_HELP: &str = "\
hid-lab set — change one setting through the daemon

Usage: hid-lab set <setting> <value> [--json]

Settings:
  dpi <n>                       a value from the mouse's DPI list
  smartshift-mode <ratchet|free-spin>
  smartshift-threshold <1..50>  lower switches to free spin more easily
  smartshift-threshold off      never switch by itself: ratchet stays ratchet
  smartshift-threshold on       switch by itself again, with the saved threshold
  thumbwheel-invert <on|off>
  remap-<button> <target|default>
      <button>: middle, back, forward, gesture, mode (the button behind the wheel)
      <target>: left, right, middle, back, forward, gesture, mode; the mouse
                decides which targets each button allows

hid-labd writes the value to the mouse if it is connected, reads it back,
and saves it: the mode in its state file, everything else in
~/.config/hid-lab/config.toml. A value the mouse rejects is not saved.

GNOME pointer settings (stored by GNOME, for every mouse):
  natural-scroll <on|off>       content follows the fingers
  acceleration <default|flat|adaptive>
";

pub const RESET_HELP: &str = "\
hid-lab reset — put settings back to the mouse's defaults

Usage: hid-lab reset (<setting> | all) [--json]

Removes the setting from ~/.config/hid-lab/config.toml, so hid-labd stops
applying it, and writes the default to the mouse if it is connected:
DPI 1000, SmartShift threshold 10 (switching on), ratchet mode, thumb wheel
not inverted, and every button doing its own task. Settings are the same
as for `hid-lab set`.
";

pub const REAPPLY_HELP: &str = "\
hid-lab reapply — reload the config file and apply it now

Usage: hid-lab reapply [--json]

Use it after editing ~/.config/hid-lab/config.toml. If the file has an
error, the daemon keeps the last valid configuration and shows the error.
";

pub const LAB_HELP: &str = "\
hid-lab lab — developer tools (read-only)

Commands:
  devices    List hidraw devices, Logitech first
  features   List the HID++ features a device reports
  read       Read battery, DPI, wheels, controls, host, firmware and name
  watch      Listen passively and show HID++ notifications
  hotplug    Show hidraw devices appearing and disappearing (udev)
  write      Write one setting to confirm a write function (needs --write)

All lab commands are read-only except `lab write`, which needs --write.
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
(for example the kernel driver) is counted. With --features-from it is also
kept in --json output, with the firmware unit ID (a serial number) masked;
without it, its bytes are not kept, because the unit ID cannot be located.

Options:
  --duration <s>          How long to listen, 1..=3600 seconds (default 30)
  --json                  Machine-readable output at the end
  --features-from <file>  Output of `hid-lab lab features --json` for the same
                          device, used to name feature indices. Without it,
                          only indices are shown, because resolving names
                          would require sending requests.

Passive: this command never writes to the device.
";

pub const HOTPLUG_HELP: &str = "\
hid-lab lab hotplug — show hidraw devices appearing and disappearing

Usage: hid-lab lab hotplug [--duration <s>] [--json]

Listens to udev for hidraw add, remove and change events and prints each
one with bus, vendor and product. After an `add`, it reports whether the
new node can be opened (the udev rule's access grant is already applied
when udev delivers the event).

Options:
  --duration <s>  How long to listen, 1..=3600 seconds (default 60)
  --json          Machine-readable output at the end

Passive: this command opens nodes only to check access and sends nothing.
";

pub const WRITE_HELP: &str = "\
hid-lab lab write — controlled write experiment for one setting

Usage:
  hid-lab lab write <setting> <value> [<device>] (--dry-run | --write) [--json]
  hid-lab lab write <setting> --restore [<device>] (--dry-run | --write) [--json]

Settings and values:
  dpi <n>                         a value from the list `hid-lab lab read` shows
  smartshift-mode <ratchet|free-spin>
  smartshift-threshold <1..50|off>
  thumbwheel-invert <on|off>
  remap-<button> <target|default>
      <button>: middle, back, forward, gesture, mode (the button behind the wheel)
      <target>: left, right, middle, back, forward, gesture, mode; the mouse
                decides which targets each button allows

What it does:
  1. Shows the target device and reads the current value.
  2. With --write: saves the current value as the original (once; a saved
     original is kept until restored), sends the write, logs every byte,
     reads back and compares the written field and the fields that must not
     change. Prints PASS or MISMATCH.
  3. With --restore: writes the saved original back and verifies it.
  With --dry-run nothing is written: it shows the exact bytes it would send.

<device> (`hidrawN`, `/dev/hidrawN` or the `#` from `hid-lab lab devices`) is
required with --write. --dry-run may auto-select the only Logitech HID++ device.

Originals and a log of every write are kept in
$XDG_STATE_HOME/hid-lab/lab-write/ (default ~/.local/state/hid-lab/lab-write/).

This command exists to confirm write functions on the real device, one at a
time (requirements §13). dpi, smartshift-mode, smartshift-threshold 1..50
and thumbwheel-invert were confirmed by the hardware tests H4. Threshold
`off` and remap-* are being tested (H5); the daemon does not send them. The
output shows each function's status.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    Top,
    Lab,
    Devices,
    Features,
    Read,
    Watch,
    Hotplug,
    Write,
    Status,
    Set,
    Reset,
    Reapply,
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
            HelpTopic::Hotplug => HOTPLUG_HELP,
            HelpTopic::Write => WRITE_HELP,
            HelpTopic::Status => STATUS_HELP,
            HelpTopic::Set => SET_HELP,
            HelpTopic::Reset => RESET_HELP,
            HelpTopic::Reapply => REAPPLY_HELP,
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
pub struct HotplugArgs {
    pub duration: Duration,
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    DryRun,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteArgs {
    pub setting: hidlab_features::settings::SettingKind,
    /// `None` with --restore.
    pub value: Option<hidlab_features::settings::SettingValue>,
    pub device: Option<String>,
    pub mode: WriteMode,
    pub json: bool,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetTarget {
    /// Written to the mouse and saved by hid-labd.
    Device(hidlab_features::settings::SettingKind),
    /// A GNOME setting.
    Desktop(hidlab_daemon::api::DesktopSetting),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetArgs {
    pub setting: SetTarget,
    /// The value as typed, checked here and sent to the daemon.
    pub raw_value: String,
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetArgs {
    /// `None` resets every setting.
    pub setting: Option<hidlab_features::settings::SettingKind>,
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help(HelpTopic),
    Version,
    Devices(DevicesArgs),
    Features(FeaturesArgs),
    Read(FeaturesArgs),
    Watch(WatchArgs),
    Hotplug(HotplugArgs),
    Write(WriteArgs),
    Status { json: bool },
    Set(SetArgs),
    Reset(ResetArgs),
    Reapply { json: bool },
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
            Some("hotplug") => parse_hotplug(it.collect()),
            Some("write") => parse_write(it.collect()),
            Some(other) => Err(format!(
                "unknown lab command `{other}`. Try: hid-lab lab --help"
            )),
        },
        Some("status") => parse_json_only(it.collect(), HelpTopic::Status, |json| {
            Command::Status { json }
        }),
        Some("reapply") => parse_json_only(it.collect(), HelpTopic::Reapply, |json| {
            Command::Reapply { json }
        }),
        Some("set") => parse_set(it.collect()),
        Some("reset") => parse_reset(it.collect()),
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

fn parse_duration(v: &str) -> Result<Duration, String> {
    v.parse::<u64>()
        .ok()
        .filter(|s| (1..=3600).contains(s))
        .map(Duration::from_secs)
        .ok_or_else(|| format!("--duration must be 1..=3600 seconds, got `{v}`"))
}

fn parse_hotplug(rest: Vec<&str>) -> Result<Command, String> {
    let mut out = HotplugArgs {
        duration: Duration::from_secs(60),
        json: false,
    };
    let mut it = rest.into_iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = split_flag(arg);
        match flag {
            "-h" | "--help" => return Ok(Command::Help(HelpTopic::Hotplug)),
            "--json" => out.json = true,
            "--duration" => {
                let v = inline
                    .map(str::to_owned)
                    .or_else(|| it.next().map(str::to_owned))
                    .ok_or("--duration needs a value. Try: hid-lab lab hotplug --help")?;
                out.duration = parse_duration(&v)?;
            }
            other => {
                return Err(format!(
                    "unexpected argument `{other}`. Try: hid-lab lab hotplug --help"
                ));
            }
        }
    }
    Ok(Command::Hotplug(out))
}

fn parse_write(rest: Vec<&str>) -> Result<Command, String> {
    use hidlab_features::settings::SettingKind;
    const TRY: &str = "Try: hid-lab lab write --help";
    let mut positionals: Vec<&str> = Vec::new();
    let (mut dry, mut write, mut restore, mut json) = (false, false, false, false);
    let mut timeout = Duration::from_millis(2000);
    let mut it = rest.into_iter();
    while let Some(arg) = it.next() {
        let (flag, inline) = split_flag(arg);
        match flag {
            "-h" | "--help" => return Ok(Command::Help(HelpTopic::Write)),
            "--dry-run" => dry = true,
            "--write" => write = true,
            "--restore" => restore = true,
            "--json" => json = true,
            "--timeout-ms" => {
                let v = inline
                    .map(str::to_owned)
                    .or_else(|| it.next().map(str::to_owned))
                    .ok_or(format!("--timeout-ms needs a value. {TRY}"))?;
                let ms: u64 = v
                    .parse()
                    .ok()
                    .filter(|&ms| (1..=60_000).contains(&ms))
                    .ok_or_else(|| format!("--timeout-ms must be 1..=60000, got `{v}`"))?;
                timeout = Duration::from_millis(ms);
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}`. {TRY}"));
            }
            other => positionals.push(other),
        }
    }
    let mode = match (dry, write) {
        (true, false) => WriteMode::DryRun,
        (false, true) => WriteMode::Write,
        (false, false) => {
            return Err(format!(
                "choose --dry-run to preview the bytes or --write to send them. {TRY}"
            ));
        }
        (true, true) => return Err(format!("use either --dry-run or --write, not both. {TRY}")),
    };
    let mut pos = positionals.into_iter();
    let name = pos.next().ok_or(format!("missing setting. {TRY}"))?;
    let setting =
        SettingKind::from_name(name).ok_or_else(|| format!("unknown setting `{name}`. {TRY}"))?;
    let value = if restore {
        None
    } else {
        let v = pos
            .next()
            .ok_or(format!("missing value for {name}. {TRY}"))?;
        Some(setting.parse_value(v)?)
    };
    let device = pos.next().map(str::to_owned);
    if let Some(extra) = pos.next() {
        return Err(format!("unexpected argument `{extra}`. {TRY}"));
    }
    if mode == WriteMode::Write && device.is_none() {
        return Err(format!(
            "--write needs an explicit device, for example `hidraw2` or the `#` from `hid-lab lab devices`. {TRY}"
        ));
    }
    Ok(Command::Write(WriteArgs {
        setting,
        value,
        device,
        mode,
        json,
        timeout,
    }))
}

/// Commands whose only option is `--json`.
fn parse_json_only(
    rest: Vec<&str>,
    topic: HelpTopic,
    make: fn(bool) -> Command,
) -> Result<Command, String> {
    let mut json = false;
    for arg in rest {
        match arg {
            "-h" | "--help" => return Ok(Command::Help(topic)),
            "--json" => json = true,
            other => {
                return Err(format!(
                    "unexpected argument `{other}`. Try: hid-lab --help"
                ));
            }
        }
    }
    Ok(make(json))
}

fn parse_set(rest: Vec<&str>) -> Result<Command, String> {
    use hidlab_features::settings::SettingKind;
    const TRY: &str = "Try: hid-lab set --help";
    let mut json = false;
    let mut positionals = Vec::new();
    for arg in rest {
        match arg {
            "-h" | "--help" => return Ok(Command::Help(HelpTopic::Set)),
            "--json" => json = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}`. {TRY}"));
            }
            other => positionals.push(other),
        }
    }
    let [name, raw] = positionals[..] else {
        return Err(format!("expected <setting> <value>. {TRY}"));
    };
    let setting = if let Some(d) = hidlab_daemon::api::DesktopSetting::from_name(name) {
        d.to_gvariant(raw)?;
        SetTarget::Desktop(d)
    } else {
        let kind = SettingKind::from_name(name)
            .ok_or_else(|| format!("unknown setting `{name}`. {TRY}"))?;
        // `on` needs the saved threshold, which only the daemon knows.
        if !(kind == SettingKind::SmartshiftThreshold && raw == "on") {
            kind.parse_value(raw)?;
        }
        SetTarget::Device(kind)
    };
    Ok(Command::Set(SetArgs {
        setting,
        raw_value: raw.to_owned(),
        json,
    }))
}

fn parse_reset(rest: Vec<&str>) -> Result<Command, String> {
    use hidlab_features::settings::SettingKind;
    const TRY: &str = "Try: hid-lab reset --help";
    let mut json = false;
    let mut positionals = Vec::new();
    for arg in rest {
        match arg {
            "-h" | "--help" => return Ok(Command::Help(HelpTopic::Reset)),
            "--json" => json = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}`. {TRY}"));
            }
            other => positionals.push(other),
        }
    }
    let [name] = positionals[..] else {
        return Err(format!("expected one <setting> or `all`. {TRY}"));
    };
    let setting = match name {
        "all" => None,
        _ => Some(
            SettingKind::from_name(name)
                .ok_or_else(|| format!("unknown setting `{name}`. {TRY}"))?,
        ),
    };
    Ok(Command::Reset(ResetArgs { setting, json }))
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
    fn hotplug_args() {
        let Ok(Command::Hotplug(h)) = parse(["lab", "hotplug", "--duration=90", "--json"]) else {
            panic!()
        };
        assert_eq!(h.duration, Duration::from_secs(90));
        assert!(h.json);
        assert!(parse(["lab", "hotplug", "hidraw2"]).is_err());
        assert!(parse(["lab", "hotplug", "--duration", "0"]).is_err());
    }

    #[test]
    fn write_args() {
        use hidlab_features::settings::{SettingKind, SettingValue};
        let Ok(Command::Write(w)) = parse(["lab", "write", "dpi", "1600", "hidraw2", "--write"])
        else {
            panic!()
        };
        assert_eq!(w.setting, SettingKind::Dpi);
        assert_eq!(w.value, Some(SettingValue::Dpi(1600)));
        assert_eq!(w.device.as_deref(), Some("hidraw2"));
        assert_eq!(w.mode, WriteMode::Write);

        let Ok(Command::Write(w)) = parse([
            "lab",
            "write",
            "smartshift-mode",
            "--restore",
            "1",
            "--dry-run",
        ]) else {
            panic!()
        };
        assert_eq!(w.value, None);
        assert_eq!(w.device.as_deref(), Some("1"));

        let Ok(Command::Write(w)) = parse(["lab", "write", "thumbwheel-invert", "on", "--dry-run"])
        else {
            panic!()
        };
        assert_eq!(w.device, None);

        // --write without a device, no mode, both modes, bad setting, bad value.
        assert!(
            parse(["lab", "write", "dpi", "1600", "--write"])
                .unwrap_err()
                .contains("explicit device")
        );
        assert!(parse(["lab", "write", "dpi", "1600", "hidraw2"]).is_err());
        assert!(
            parse([
                "lab",
                "write",
                "dpi",
                "1600",
                "hidraw2",
                "--write",
                "--dry-run"
            ])
            .is_err()
        );
        assert!(parse(["lab", "write", "wheel-mode", "1", "hidraw2", "--write"]).is_err());
        assert!(parse(["lab", "write", "smartshift-threshold", "99", "--dry-run"]).is_err());
        assert!(parse(["lab", "write", "dpi", "1600", "2", "3", "--dry-run"]).is_err());
    }

    #[test]
    fn user_commands() {
        use hidlab_features::settings::SettingKind;
        assert_eq!(parse(["status"]), Ok(Command::Status { json: false }));
        assert_eq!(
            parse(["reapply", "--json"]),
            Ok(Command::Reapply { json: true })
        );
        assert_eq!(parse(["set", "--help"]), Ok(Command::Help(HelpTopic::Set)));
        let Ok(Command::Set(s)) = parse(["set", "smartshift-mode", "free-spin"]) else {
            panic!()
        };
        assert_eq!(s.setting, SetTarget::Device(SettingKind::SmartshiftMode));
        assert_eq!(s.raw_value, "free-spin");
        for (k, v) in [
            ("smartshift-threshold", "on"),
            ("smartshift-threshold", "off"),
            ("remap-gesture", "middle"),
            ("remap-back", "default"),
            ("natural-scroll", "on"),
            ("acceleration", "flat"),
        ] {
            assert!(parse(["set", k, v]).is_ok(), "{k} {v}");
        }
        assert!(parse(["set", "thumbwheel-invert", "maybe"]).is_err());
        assert!(parse(["set", "remap-left", "back"]).is_err());
        assert!(parse(["set", "natural-scroll", "true"]).is_err());
        assert!(parse(["set", "acceleration", "fast"]).is_err());
        assert_eq!(
            parse(["reset", "all", "--json"]),
            Ok(Command::Reset(ResetArgs {
                setting: None,
                json: true
            }))
        );
        assert_eq!(
            parse(["reset", "dpi"]),
            Ok(Command::Reset(ResetArgs {
                setting: Some(SettingKind::Dpi),
                json: false
            }))
        );
        assert!(parse(["reset"]).is_err());
        assert!(parse(["reset", "wheel-mode"]).is_err());
        assert!(parse(["set", "dpi"]).is_err());
        assert!(parse(["set", "dpi", "fast"]).is_err());
        assert!(parse(["set", "wheel-mode", "on"]).is_err());
        assert!(parse(["status", "extra"]).is_err());
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
