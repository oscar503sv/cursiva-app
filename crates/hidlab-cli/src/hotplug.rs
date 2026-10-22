//! `hid-lab lab hotplug`: passive udev listener for hidraw devices.

use std::fs;
use std::time::Instant;

use hidlab_transport::{HotplugAction, HotplugEvent, HotplugMonitor, parse_uevent};
use serde::Serialize;

use crate::args::HotplugArgs;
use crate::output::access_label;

#[derive(Serialize)]
struct TimedEvent {
    t_ms: u64,
    #[serde(flatten)]
    event: HotplugEvent,
    /// `HID_NAME`, read from sysfs right after an `add`.
    name: Option<String>,
}

#[derive(Serialize)]
struct HotplugJson {
    schema: &'static str,
    tool_version: &'static str,
    duration_s: u64,
    events: Vec<TimedEvent>,
}

fn name_of(hidraw: &str) -> Option<String> {
    let text = fs::read_to_string(format!("/sys/class/hidraw/{hidraw}/device/uevent")).ok()?;
    parse_uevent(&text).ok()?.name
}

fn print(e: &TimedEvent) {
    let ev = &e.event;
    let action = match ev.action {
        HotplugAction::Add => "add",
        HotplugAction::Remove => "remove",
        HotplugAction::Change => "change",
        HotplugAction::Other => "other",
    };
    let ids = ev.ids.map_or("-".to_owned(), |i| {
        format!("{} {:04X}:{:04X}", i.bus.label(), i.vendor_id, i.product_id)
    });
    let access = ev.access.as_ref().map_or(String::new(), |a| {
        format!("  access {}", access_label(a.status))
    });
    println!(
        "{:>9.3} s  {action:<6}  {:<8}  {ids:<20}{}{access}{}",
        e.t_ms as f64 / 1000.0,
        ev.hidraw_name,
        e.name
            .as_deref()
            .map_or(String::new(), |n| format!("  {n}")),
        if ev.is_logitech() { "  [Logitech]" } else { "" },
    );
}

pub fn run(args: &HotplugArgs) -> Result<(), String> {
    let mut monitor = HotplugMonitor::new().map_err(|e| e.to_string())?;
    if !args.json {
        println!(
            "Listening for hidraw devices for {} s. Nothing is sent to any device. Press Ctrl+C to stop.",
            args.duration.as_secs()
        );
        println!();
    }
    let start = Instant::now();
    let deadline = start + args.duration;
    let mut events = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let Some(event) = monitor.next_event(remaining).map_err(|e| e.to_string())? else {
            break;
        };
        let name = (event.action == HotplugAction::Add)
            .then(|| name_of(&event.hidraw_name))
            .flatten();
        let timed = TimedEvent {
            t_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
            event,
            name,
        };
        if !args.json {
            print(&timed);
        }
        events.push(timed);
    }

    if args.json {
        let out = HotplugJson {
            schema: "hid-lab.hotplug/1",
            tool_version: env!("CARGO_PKG_VERSION"),
            duration_s: args.duration.as_secs(),
            events,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        let adds = events
            .iter()
            .filter(|e| e.event.action == HotplugAction::Add)
            .count();
        let removes = events
            .iter()
            .filter(|e| e.event.action == HotplugAction::Remove)
            .count();
        println!();
        println!("Events: {} ({adds} add, {removes} remove).", events.len());
    }
    Ok(())
}
