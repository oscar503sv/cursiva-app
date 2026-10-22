//! `hid-lab lab watch`: passive listener. Never sends anything.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use hidlab_hidpp::{FeatureId, Frame, decode, hex, reference_name};
use hidlab_transport::{Hidraw, HidrawDevice, RawChannel, TransportError, discover};
use serde::Serialize;

use crate::args::WatchArgs;
use crate::output::DeviceJson;
use crate::target::select;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// HID++ report with software ID 0.
    Notification,
    /// HID++ traffic of other software (non-zero sw_id) or error frames.
    OtherHidpp,
    /// A report with a HID++ report ID that could not be decoded.
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Event {
    pub t_ms: u64,
    pub kind: EventKind,
    pub feature_index: Option<u8>,
    pub feature_id: Option<String>,
    pub reference_name: Option<&'static str>,
    pub function: Option<u8>,
    pub sw_id: Option<u8>,
    /// Parameter bytes with trailing zeros removed.
    pub params_hex: Option<String>,
    /// Full report. `None` when the payload was not kept (see `classify`).
    pub raw_hex: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Non-HID++ input reports (movement, buttons, wheel) by report ID.
    pub input_reports: BTreeMap<String, u64>,
    pub notifications: u64,
    pub other_hidpp: u64,
    pub malformed: u64,
}

pub type FeatureNames = BTreeMap<u8, FeatureId>;

fn trim_hex(p: &[u8]) -> String {
    let end = p.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    hex(&p[..end])
}

/// Feature whose function 0 answer carries the unit serial in params 1..5.
const DEVICE_INFO: FeatureId = FeatureId(0x0003);
const REDACTED_BYTE: &str = "**";

/// Masks hex bytes `from..to` of a space-separated hex string.
fn mask(hex_str: &str, from: usize, to: usize) -> String {
    hex_str
        .split(' ')
        .enumerate()
        .map(|(i, b)| {
            if (from..to).contains(&i) {
                REDACTED_BYTE
            } else {
                b
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Classifies one report. Returns `None` for plain input reports, which are
/// only counted.
///
/// Privacy: other programs' requests can read the unit serial (0x0003
/// function 0). With a feature map, those answers are masked. Without one,
/// the index of 0x0003 is unknown, so the payload of other programs'
/// HID++ traffic is not kept at all; only notifications keep their bytes.
pub fn classify(
    report: &[u8],
    t_ms: u64,
    names: &FeatureNames,
    counts: &mut Counts,
) -> Option<Event> {
    let base = |kind| Event {
        t_ms,
        kind,
        feature_index: None,
        feature_id: None,
        reference_name: None,
        function: None,
        sw_id: None,
        params_hex: None,
        raw_hex: Some(hex(report)),
    };
    match decode(report) {
        Err(_) => {
            counts.malformed += 1;
            Some(base(EventKind::Malformed))
        }
        Ok(Frame::NotHidpp { report_id }) => {
            *counts
                .input_reports
                .entry(format!("0x{report_id:02x}"))
                .or_default() += 1;
            None
        }
        Ok(Frame::Message {
            feature_index,
            function,
            sw_id,
            params,
            ..
        }) => {
            let kind = if sw_id == 0 {
                counts.notifications += 1;
                EventKind::Notification
            } else {
                counts.other_hidpp += 1;
                EventKind::OtherHidpp
            };
            let id = names.get(&feature_index).copied();
            let mut e = Event {
                feature_index: Some(feature_index),
                feature_id: id.map(|i| i.to_string()),
                reference_name: id.and_then(reference_name),
                function: Some(function),
                sw_id: Some(sw_id),
                params_hex: Some(trim_hex(&params)),
                ..base(kind)
            };
            if id == Some(DEVICE_INFO) && function == 0 {
                e.params_hex = e.params_hex.map(|p| mask(&p, 1, 5));
                e.raw_hex = e.raw_hex.map(|r| mask(&r, 5, 9));
            } else if kind == EventKind::OtherHidpp && names.is_empty() {
                e.params_hex = None;
                e.raw_hex = None;
            }
            Some(e)
        }
        Ok(Frame::Error20 { .. } | Frame::Error10 { .. }) => {
            counts.other_hidpp += 1;
            Some(base(EventKind::OtherHidpp))
        }
    }
}

/// Listens on `channel` for `duration`. Only ever calls `recv`.
/// Returns the events, counts and, if the device went away, a note.
pub fn listen<C: RawChannel>(
    channel: &mut C,
    duration: Duration,
    names: &FeatureNames,
    mut on_event: impl FnMut(&Event),
) -> Result<(Vec<Event>, Counts, Option<String>), TransportError> {
    let start = Instant::now();
    let deadline = start + duration;
    let mut events = Vec::new();
    let mut counts = Counts::default();
    let mut buf = [0u8; 64];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok((events, counts, None));
        }
        match channel.recv(&mut buf, remaining) {
            Ok(None) => return Ok((events, counts, None)),
            Ok(Some(n)) => {
                let t_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                if let Some(e) = classify(&buf[..n.min(buf.len())], t_ms, names, &mut counts) {
                    on_event(&e);
                    events.push(e);
                }
            }
            Err(TransportError::Disconnected { .. }) => {
                let note = format!(
                    "device disconnected after {:.1} s; this is normal when the mouse sleeps or switches channel",
                    start.elapsed().as_secs_f64()
                );
                return Ok((events, counts, Some(note)));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Reads feature indices from `hid-lab lab features --json` output and
/// checks it belongs to the same kind of device.
pub fn load_names(text: &str, device: &HidrawDevice) -> Result<FeatureNames, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    if v["schema"] != "hid-lab.features/1" {
        return Err("not the output of `hid-lab lab features --json`".into());
    }
    let vid = format!("0x{:04x}", device.uevent.vendor_id);
    let pid = format!("0x{:04x}", device.uevent.product_id);
    if v["device"]["vendor_id"] != vid.as_str() || v["device"]["product_id"] != pid.as_str() {
        return Err(format!(
            "the file describes {}:{}, but the selected device is {vid}:{pid}",
            v["device"]["vendor_id"], v["device"]["product_id"]
        ));
    }
    let mut names = FeatureNames::new();
    for f in v["features"].as_array().ok_or("no `features` array")? {
        let index = f["index"]
            .as_u64()
            .and_then(|i| u8::try_from(i).ok())
            .ok_or("feature without a valid `index`")?;
        let id = f["id"]
            .as_str()
            .and_then(|s| s.strip_prefix("0x"))
            .and_then(|s| u16::from_str_radix(s, 16).ok())
            .ok_or("feature without a valid `id`")?;
        names.insert(index, FeatureId(id));
    }
    Ok(names)
}

#[derive(Serialize)]
struct WatchJson<'a> {
    schema: &'static str,
    tool_version: &'static str,
    passive: bool,
    device: DeviceJson,
    duration_s: u64,
    feature_names_from: Option<&'a str>,
    counts: Counts,
    events: Vec<Event>,
    end_note: Option<String>,
}

fn print_event(e: &Event) {
    if e.kind != EventKind::Notification {
        return;
    }
    let idx = e.feature_index.unwrap_or(0);
    let feature = match (&e.feature_id, e.reference_name) {
        (Some(id), Some(n)) => format!("{id} {n}"),
        (Some(id), None) => format!("{id} unknown"),
        _ => "(index only)".to_owned(),
    };
    println!(
        "{:>9.3} s  idx 0x{idx:02x}  {feature:<26}  fn {}  params {}",
        e.t_ms as f64 / 1000.0,
        e.function.unwrap_or(0),
        e.params_hex.as_deref().unwrap_or("")
    );
}

pub fn run(args: &WatchArgs) -> Result<(), String> {
    let discovery = discover().map_err(|e| e.to_string())?;
    let (number, device) = select(&discovery, args.device.as_deref())?;

    let names = match &args.features_from {
        Some(path) => {
            let text = std::fs::read_to_string(Path::new(path))
                .map_err(|e| format!("cannot read {path}: {e}"))?;
            load_names(&text, device).map_err(|e| {
                format!(
                    "cannot use {path}: {e}\nTry: hid-lab lab features --json > features.json, then pass that file."
                )
            })?
        }
        None => FeatureNames::new(),
    };

    let mut channel = Hidraw::open(&device.node).map_err(|e| e.to_string())?;
    if !args.json {
        println!(
            "Listening on {} ({}) for {} s. Nothing is sent to the device. Press Ctrl+C to stop.",
            device.node.display(),
            device.uevent.name.as_deref().unwrap_or("?"),
            args.duration.as_secs()
        );
        if names.is_empty() {
            println!("Feature names: not loaded (pass --features-from to name indices).");
        }
        println!();
    }

    let json = args.json;
    let (events, counts, end_note) = listen(&mut channel, args.duration, &names, |e| {
        if !json {
            print_event(e);
        }
    })
    .map_err(|e| e.to_string())?;

    if json {
        let out = WatchJson {
            schema: "hid-lab.watch/1",
            tool_version: env!("CARGO_PKG_VERSION"),
            passive: true,
            device: DeviceJson::new(number, device, false),
            duration_s: args.duration.as_secs(),
            feature_names_from: args.features_from.as_deref(),
            counts,
            events,
            end_note,
        };
        let text = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
        println!("{text}");
        return Ok(());
    }

    println!();
    if let Some(note) = end_note {
        println!("{note}");
    }
    let inputs: u64 = counts.input_reports.values().sum();
    println!(
        "Notifications: {}. Other HID++ traffic (other software): {}. Input reports ignored: {inputs}. Malformed: {}.",
        counts.notifications, counts.other_hidpp, counts.malformed
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct NeverSend(VecDeque<Vec<u8>>);

    impl RawChannel for NeverSend {
        fn send(&mut self, _: &[u8]) -> Result<(), TransportError> {
            panic!("watch must never send");
        }
        fn recv(&mut self, buf: &mut [u8], _: Duration) -> Result<Option<usize>, TransportError> {
            match self.0.pop_front() {
                Some(r) if r.is_empty() => Err(TransportError::Disconnected {
                    node: "/dev/hidraw-test".into(),
                }),
                Some(r) => {
                    buf[..r.len()].copy_from_slice(&r);
                    Ok(Some(r.len()))
                }
                None => Ok(None),
            }
        }
    }

    fn long(fi: u8, b3: u8, params: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; 20];
        r[..4].copy_from_slice(&[0x11, 0xFF, fi, b3]);
        r[4..4 + params.len()].copy_from_slice(params);
        r
    }

    #[test]
    fn classifies_and_counts_without_sending() {
        // SYNTHETIC traffic.
        let mut ch = NeverSend(VecDeque::from([
            vec![0x02, 0, 1, 0, 0, 0, 0, 0],
            vec![0x02, 0, 2, 0, 0, 0, 0, 0],
            long(0x08, 0x00, &[0x23, 0x04]), // notification, fn 0
            long(0x0F, 0x01, &[0x01]),       // other software, sw_id 1
            long(0xFF, 0x08, &[0x0B, 0x05]), // HID++ 2.0 error
            vec![0x11, 0xFF],                // malformed
            vec![],                          // disconnect
        ]));
        let names = FeatureNames::from([(0x08, FeatureId(0x1004))]);
        let mut live = 0;
        let (events, counts, note) =
            listen(&mut ch, Duration::from_secs(5), &names, |_| live += 1).unwrap();
        assert_eq!(counts.input_reports.get("0x02"), Some(&2));
        assert_eq!(counts.notifications, 1);
        assert_eq!(counts.other_hidpp, 2);
        assert_eq!(counts.malformed, 1);
        assert_eq!(events.len(), 4);
        assert_eq!(live, 4);
        let n = &events[0];
        assert_eq!(n.kind, EventKind::Notification);
        assert_eq!(n.feature_id.as_deref(), Some("0x1004"));
        assert_eq!(n.reference_name, Some("UnifiedBattery"));
        assert_eq!(n.params_hex.as_deref(), Some("23 04"));
        assert!(note.unwrap().contains("disconnected"));
    }

    #[test]
    fn unit_id_of_other_programs_is_masked_or_dropped() {
        // SYNTHETIC getDeviceInfo answer to another program (sw_id 0x0B).
        let answer = long(
            0x02,
            0x0B,
            &[0x03, 0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x02, 0xB0, 0x34],
        );

        let names = FeatureNames::from([(0x02, FeatureId(0x0003))]);
        let mut c = Counts::default();
        let e = classify(&answer, 0, &names, &mut c).unwrap();
        assert_eq!(e.kind, EventKind::OtherHidpp);
        assert_eq!(e.params_hex.as_deref(), Some("03 ** ** ** ** 00 02 b0 34"));
        let raw = e.raw_hex.unwrap();
        assert!(raw.starts_with("11 ff 02 0b 03 ** ** ** ** 00 02 b0 34"));
        assert!(!raw.contains("de ad"));

        let e = classify(&answer, 0, &FeatureNames::new(), &mut c).unwrap();
        assert_eq!(e.params_hex, None);
        assert_eq!(e.raw_hex, None);
        assert_eq!(c.other_hidpp, 2);

        // Notifications keep their bytes even without a map.
        let n = classify(&long(0x08, 0x00, &[0x23]), 0, &FeatureNames::new(), &mut c).unwrap();
        assert_eq!(n.params_hex.as_deref(), Some("23"));
        assert!(n.raw_hex.is_some());
    }

    #[test]
    fn ends_on_timeout() {
        let mut ch = NeverSend(VecDeque::new());
        let (events, counts, note) = listen(
            &mut ch,
            Duration::from_millis(10),
            &FeatureNames::new(),
            |_| {},
        )
        .unwrap();
        assert!(events.is_empty());
        assert_eq!(counts, Counts::default());
        assert!(note.is_none());
    }

    fn device(pid: u16) -> HidrawDevice {
        use hidlab_transport::{Access, AccessCheck, Bus, Uevent};
        HidrawDevice {
            hidraw_name: "hidraw9".into(),
            node: "/dev/hidraw9".into(),
            hid_device: None,
            uevent: Uevent {
                bus: Bus::Bluetooth,
                vendor_id: 0x046D,
                product_id: pid,
                name: None,
                uniq: None,
            },
            driver: None,
            descriptor: None,
            descriptor_error: None,
            access: AccessCheck {
                status: Access::Ok,
                detail: None,
            },
        }
    }

    #[test]
    fn loads_names_and_checks_device() {
        // SYNTHETIC file shaped like `lab features --json`.
        let text = r#"{"schema":"hid-lab.features/1",
            "device":{"vendor_id":"0x046d","product_id":"0xb034"},
            "features":[{"index":0,"id":"0x0000"},{"index":8,"id":"0x1004"}]}"#;
        let names = load_names(text, &device(0xB034)).unwrap();
        assert_eq!(names.get(&8), Some(&FeatureId(0x1004)));
        assert!(
            load_names(text, &device(0xB035))
                .unwrap_err()
                .contains("selected device")
        );
        assert!(load_names("{}", &device(0xB034)).is_err());
        assert!(load_names("nope", &device(0xB034)).is_err());
    }
}
