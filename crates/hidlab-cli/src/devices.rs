//! `hid-lab lab devices`.

use hidlab_transport::{Access, Discovery, UDEV_INSTALL_HINT, discover};
use serde::Serialize;

use crate::args::DevicesArgs;
use crate::output::{DeviceJson, access_label, hidpp_label, redact, table};

#[derive(Serialize)]
struct DevicesJson {
    schema: &'static str,
    devices: Vec<DeviceJson>,
    skipped: Vec<SkippedJson>,
}

#[derive(Serialize)]
struct SkippedJson {
    hidraw: String,
    reason: String,
}

pub fn run(args: &DevicesArgs) -> Result<(), String> {
    let discovery = discover().map_err(|e| e.to_string())?;
    if args.json {
        print_json(&discovery, args.show_unique)
    } else {
        print_text(&discovery, args.show_unique);
        Ok(())
    }
}

fn print_json(d: &Discovery, show_unique: bool) -> Result<(), String> {
    let out = DevicesJson {
        schema: "hid-lab.devices/1",
        devices: d
            .devices
            .iter()
            .enumerate()
            .map(|(i, dev)| DeviceJson::new(i + 1, dev, show_unique))
            .collect(),
        skipped: d
            .skipped
            .iter()
            .map(|s| SkippedJson {
                hidraw: s.hidraw_name.clone(),
                reason: s.reason.clone(),
            })
            .collect(),
    };
    let text = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

fn print_text(d: &Discovery, show_unique: bool) {
    if d.devices.is_empty() {
        println!("No hidraw devices found.");
        println!("Try: check that the device is connected (`bluetoothctl devices Connected`).");
    } else {
        let mut header = vec![
            "#", "node", "bus", "VID:PID", "driver", "HID++", "access", "name",
        ];
        if show_unique {
            header.push("unique");
        }
        let rows: Vec<Vec<String>> = d
            .devices
            .iter()
            .enumerate()
            .map(|(i, dev)| {
                let mut row = vec![
                    (i + 1).to_string(),
                    dev.node.display().to_string(),
                    dev.uevent.bus.label(),
                    format!("{:04X}:{:04X}", dev.uevent.vendor_id, dev.uevent.product_id),
                    dev.driver.clone().unwrap_or_else(|| "-".into()),
                    hidpp_label(dev).to_owned(),
                    access_label(dev.access.status).to_owned(),
                    dev.uevent.name.clone().unwrap_or_else(|| "-".into()),
                ];
                if show_unique {
                    row.push(
                        dev.uevent
                            .uniq
                            .as_deref()
                            .map_or_else(|| "-".into(), |u| redact(u, true)),
                    );
                }
                row
            })
            .collect();
        print!("{}", table(&header, &rows));
    }

    for s in &d.skipped {
        eprintln!("note: skipped {}: {}", s.hidraw_name, s.reason);
    }

    let denied: Vec<_> = d
        .devices
        .iter()
        .filter(|dev| dev.is_logitech() && dev.access.status == Access::PermissionDenied)
        .collect();
    if !denied.is_empty() {
        let nodes: Vec<_> = denied
            .iter()
            .map(|d| d.node.display().to_string())
            .collect();
        println!();
        println!(
            "Cannot open {} (permission denied).\n\
             The hid-lab udev rule is probably not installed or not yet applied.\n\
             Try, from the hid-lab source directory:\n{UDEV_INSTALL_HINT}",
            nodes.join(", ")
        );
    }
}
