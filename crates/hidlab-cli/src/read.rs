//! `hid-lab lab read`: read-only state of known HID++ features.

use hidlab_features::battery::Battery;
use hidlab_features::dpi::DpiEntry;
use hidlab_features::{DeviceState, Reading, firmware, read_all};
use hidlab_hidpp::{ReportKind, Session, TranscriptEntry, reference_name};
use hidlab_transport::{Hidraw, HidrawDevice, PowerSupply, discover, power_supplies_for};
use serde::Serialize;

use crate::args::FeaturesArgs;
use crate::output::{DeviceJson, REDACTED, table};
use crate::target::{select, session_config};

#[derive(Serialize)]
struct ReadJson {
    schema: &'static str,
    tool_version: &'static str,
    read_only: bool,
    device: DeviceJson,
    session: SessionJson,
    state: Option<DeviceState>,
    battery_vs_kernel: Option<BatteryComparison>,
    kernel_power_supplies: Vec<PowerSupply>,
    ignored_reports: u64,
    error: Option<String>,
    transcript: Vec<TranscriptEntry>,
}

#[derive(Serialize)]
struct SessionJson {
    report: ReportKind,
    device_index: String,
    sw_id: String,
    timeout_ms: u128,
}

/// Battery percentage from HID++ next to the kernel's power_supply capacity.
/// The kernel reads the same feature, so this checks hid-lab's decoding
/// against the kernel's, not against an independent sensor.
#[derive(Debug, Serialize)]
struct BatteryComparison {
    device_state_of_charge: u8,
    kernel_supply: String,
    kernel_capacity: Option<u8>,
    kernel_status: Option<String>,
    difference: Option<i16>,
    matches: bool,
}

fn compare_battery(b: &Battery, supplies: &[PowerSupply]) -> Option<BatteryComparison> {
    let s = supplies.first()?;
    let soc = b.status.state_of_charge;
    let difference = s.capacity.map(|c| i16::from(soc) - i16::from(c));
    Some(BatteryComparison {
        device_state_of_charge: soc,
        kernel_supply: s.name.clone(),
        kernel_capacity: s.capacity,
        kernel_status: s.status.clone(),
        difference,
        matches: difference == Some(0),
    })
}

pub fn run(args: &FeaturesArgs) -> Result<(), String> {
    let discovery = discover().map_err(|e| e.to_string())?;
    let (number, device) = select(&discovery, args.device.as_deref())?;
    let config = session_config(device, args.index, args.timeout);

    let channel = Hidraw::open(&device.node).map_err(|e| e.to_string())?;
    let mut session = Session::new(channel, config);
    let result = read_all(&mut session);
    let ignored = session.ignored();
    let (_, mut transcript) = session.into_parts();

    let supplies = match &device.hid_device {
        Some(hd) => power_supplies_for(hd).unwrap_or_default(),
        None => vec![],
    };

    let mut state = result.map_err(|e| e.to_string());
    if !args.show_unique {
        firmware::redact_unit_id_in_transcript(&mut transcript, config.sw_id);
        if let Ok(Reading::Ok { value }) = state.as_mut().map(|s| &mut s.firmware) {
            value.info.unit_id = REDACTED.to_owned();
        }
    }
    let comparison = state
        .as_ref()
        .ok()
        .and_then(|s| s.battery.value())
        .and_then(|b| compare_battery(b, &supplies));

    if args.json {
        let failed = state.is_err();
        let out = ReadJson {
            schema: "hid-lab.read/1",
            tool_version: env!("CARGO_PKG_VERSION"),
            read_only: true,
            device: DeviceJson::new(number, device, args.show_unique),
            session: SessionJson {
                report: config.report,
                device_index: format!("0x{:02x}", config.device_index),
                sw_id: format!("0x{:02x}", config.sw_id),
                timeout_ms: config.timeout.as_millis(),
            },
            error: state.as_ref().err().cloned(),
            state: state.ok(),
            battery_vs_kernel: comparison,
            kernel_power_supplies: supplies,
            ignored_reports: ignored,
            transcript: transcript.entries,
        };
        let text = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
        println!("{text}");
        return if failed {
            Err("reading failed; see the `error` field above".into())
        } else {
            Ok(())
        };
    }

    print_device(number, device);
    let state = state?;
    print_state(&state, comparison.as_ref(), args.show_unique);
    println!();
    println!(
        "Read-only: only allowlisted read functions were sent. No device setting was changed."
    );
    println!("Ignored {ignored} unrelated report(s) while waiting for answers.");
    Ok(())
}

fn print_device(number: usize, d: &HidrawDevice) {
    println!(
        "Device: #{number} {}  {}  ({} {:04X}:{:04X})",
        d.node.display(),
        d.uevent.name.as_deref().unwrap_or("(no name)"),
        d.uevent.bus.label(),
        d.uevent.vendor_id,
        d.uevent.product_id
    );
}

fn heading<'a, T>(title: &str, id: u16, r: &'a Reading<T>) -> Option<&'a T> {
    println!();
    let name = reference_name(hidlab_hidpp::FeatureId(id)).unwrap_or("unknown");
    print!("{title} (0x{id:04X} {name}): ");
    match r {
        Reading::NotSupported => {
            println!("not reported by this device");
            None
        }
        Reading::Failed { error } => {
            println!("failed: {error}");
            None
        }
        Reading::Ok { value } => {
            println!();
            Some(value)
        }
    }
}

fn notes(n: &[String]) {
    for note in n {
        println!("  note: {note}");
    }
}

fn print_state(s: &DeviceState, cmp: Option<&BatteryComparison>, show_unique: bool) {
    if let Some(n) = heading("Name", 0x0005, &s.device_name) {
        println!("  name: {:?}", n.name);
        match &n.device_type {
            Some(t) => println!(
                "  type: {} ({})",
                t.code,
                t.reference_label.unwrap_or("not in reference table")
            ),
            None => println!("  type: not read"),
        }
        notes(&n.notes);
    }

    if let Some(f) = heading("Firmware", 0x0003, &s.firmware) {
        let unit = if show_unique {
            f.info.unit_id.clone()
        } else {
            REDACTED.to_owned()
        };
        println!(
            "  entities: {}, unit ID: {unit}, transport bits: 0x{:02x}",
            f.info.entity_count, f.info.transport_bits
        );
        let models: Vec<String> = f
            .info
            .model_ids
            .iter()
            .map(|m| format!("{}={}", m.transport, m.id))
            .collect();
        println!("  model IDs: {}", models.join(", "));
        for e in &f.entities {
            match e.kind_code {
                0 | 1 => println!(
                    "  [{}] {:<10} {} {} build 0x{:04X}{}",
                    e.entity,
                    e.kind,
                    e.name.as_deref().unwrap_or("?"),
                    e.version.as_deref().unwrap_or("?"),
                    e.build.unwrap_or(0),
                    e.extras_hex
                        .as_deref()
                        .map(|x| format!(", extras {x}"))
                        .unwrap_or_default()
                ),
                2 => println!(
                    "  [{}] hardware   version {}",
                    e.entity,
                    e.hardware_version.unwrap_or(0)
                ),
                _ => println!("  [{}] {:<10} raw {}", e.entity, e.kind, e.raw_hex),
            }
        }
        notes(&f.notes);
    }

    if let Some(b) = heading("Battery", 0x1004, &s.battery) {
        let c = &b.status.charging;
        println!(
            "  charge: {} %, levels [{}]",
            b.status.state_of_charge,
            b.status.levels.join(",")
        );
        println!(
            "  charging status: {} (kernel: {}, Solaar: {}; references {})",
            c.code,
            c.kernel.unwrap_or("?"),
            c.solaar.unwrap_or("?"),
            c.references
        );
        println!(
            "  external power byte: 0x{:02x} (not interpreted)",
            b.status.external_power
        );
        if let Some(cap) = &b.capabilities {
            println!(
                "  capabilities: levels [{}], flags [{}]",
                cap.supported_levels.join(","),
                cap.flags.join(",")
            );
        }
        match cmp {
            Some(c) => println!(
                "  kernel {}: capacity {}, status {} -> {}",
                c.kernel_supply,
                c.kernel_capacity.map_or("?".into(), |v| format!("{v} %")),
                c.kernel_status.as_deref().unwrap_or("?"),
                if c.matches { "matches" } else { "DIFFERS" }
            ),
            None => println!("  kernel power_supply: none found for this device"),
        }
        notes(&b.notes);
    }

    if let Some(h) = heading("Host channel", 0x1814, &s.change_host) {
        println!(
            "  channels: {}, current: {} (0-based {})",
            h.host_count, h.current_channel, h.current_host
        );
    }

    if let Some(rc) = heading("Reprogrammable controls", 0x1B04, &s.reprog_controls) {
        println!("  count: {}", rc.count);
        let rows: Vec<Vec<String>> = rc
            .controls
            .iter()
            .map(|c| {
                let rep = c.reporting.as_ref();
                vec![
                    c.index.to_string(),
                    c.cid.clone(),
                    c.tid.clone(),
                    format!("0x{:04x}", c.flags),
                    c.flag_names.join(","),
                    format!("{}/{}/0x{:02x}", c.pos, c.group, c.group_mask),
                    rep.map_or("-".into(), |r| {
                        if r.flag_names.is_empty() {
                            "none".into()
                        } else {
                            r.flag_names.join(",")
                        }
                    }),
                    rep.map_or("-".into(), |r| format!("0x{:04X}", r.mapped_to)),
                ]
            })
            .collect();
        let t = table(
            &[
                "#",
                "CID",
                "TID",
                "flags",
                "capabilities",
                "pos/grp/gmask",
                "reporting",
                "mapped to",
            ],
            &rows,
        );
        for line in t.lines() {
            println!("  {line}");
        }
        notes(&rc.notes);
    }

    if let Some(d) = heading("DPI", 0x2201, &s.dpi) {
        let entries: Vec<String> = d
            .list
            .entries
            .iter()
            .map(|e| match *e {
                DpiEntry::Value { dpi } => dpi.to_string(),
                DpiEntry::Range { from, to, step } => format!("{from}..{to} step {step}"),
            })
            .collect();
        println!(
            "  current: {} (raw {}, default {}), in list: {}",
            d.current.effective_dpi, d.current.dpi, d.current.default_dpi, d.current_in_list
        );
        println!(
            "  list: {}{}",
            entries.join(", "),
            if d.list.terminated {
                ""
            } else {
                " (not terminated in first page)"
            }
        );
    }

    if let Some(ss) = heading("SmartShift", 0x2110, &s.smartshift) {
        println!(
            "  mode: {} ({}), threshold: {}, byte 2: 0x{:02x} (unknown)",
            ss.mode_code,
            ss.mode.unwrap_or("undocumented"),
            ss.threshold,
            ss.byte2
        );
    }

    if let Some(w) = heading("Hi-res wheel", 0x2121, &s.hires_wheel) {
        println!(
            "  multiplier: {}, has invert: {}, has ratchet switch: {}",
            w.capability.multiplier, w.capability.has_invert, w.capability.has_ratchet_switch
        );
        println!(
            "  mode 0x{:02x}: diverted {}, high resolution {}, inverted {}",
            w.mode.raw, w.mode.diverted, w.mode.high_resolution, w.mode.inverted
        );
        match &w.ratchet_switch {
            Some(r) => println!("  ratchet switch: 0x{:02x} (ratchet {})", r.raw, r.ratchet),
            None => println!("  ratchet switch: not read"),
        }
        notes(&w.notes);
    }

    if let Some(t) = heading("Thumb wheel", 0x2150, &s.thumbwheel) {
        println!(
            "  diverted: {}, inverted: {}, raw {}",
            t.diverted, t.inverted, t.raw_hex
        );
    }
}
