//! User commands: `status`, `set`, `reapply`. They talk to hid-labd only.

use std::time::{SystemTime, UNIX_EPOCH};

use hidlab_daemon::api::{ButtonEvent, DesktopReply, ReapplyReply, ResetReply, SetReply, Status};
use hidlab_daemon::apply::{ApplyItem, ApplyReport, ItemResult};
use hidlab_features::settings::SettingKind;
use serde::Serialize;

use crate::args::{HotplugArgs, ResetArgs, SetArgs, SetTarget};
use crate::client::{call, call_value, subscribe};

fn print_json<T: Serialize>(v: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(v).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn ago(unix: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match now.saturating_sub(unix) {
        s if s < 120 => format!("{s} s ago"),
        s if s < 7200 => format!("{} min ago", s / 60),
        s => format!("{} h ago", s / 3600),
    }
}

fn result_label(r: ItemResult) -> &'static str {
    match r {
        ItemResult::Unchanged => "already set",
        ItemResult::Written => "written and verified",
        ItemResult::Mismatch => "MISMATCH after writing",
        ItemResult::Rejected => "REJECTED, not sent",
        ItemResult::Failed => "FAILED",
    }
}

fn print_item(i: &ApplyItem) {
    println!(
        "  {:<22} {:<18} {}",
        i.setting.name(),
        i.target.display(),
        result_label(i.result)
    );
    for c in i.checks.iter().filter(|c| !c.ok) {
        println!(
            "      {}: expected {}, read {}",
            c.field, c.expected, c.actual
        );
    }
    if let Some(e) = &i.error {
        for line in e.lines() {
            println!("      {line}");
        }
    }
}

fn print_report(r: &ApplyReport) {
    for i in &r.items {
        print_item(i);
    }
    if r.items.is_empty() {
        println!("  (nothing configured)");
    }
    println!(
        "  {} in {} ms",
        if r.ok { "verified" } else { "with problems" },
        r.elapsed_ms
    );
}

fn report_result(r: &ApplyReport) -> Result<(), String> {
    if r.ok {
        Ok(())
    } else {
        Err("not every setting was applied; see above".into())
    }
}

pub fn status(json: bool) -> Result<(), String> {
    let s: Status = call("GetStatus", &())?;
    if json {
        return print_json(&s);
    }
    println!("Daemon:  hid-labd {}", s.daemon_version);
    println!("Config:  {}", s.config_file);
    match &s.device {
        Some(d) => println!(
            "Mouse:   {} at {} ({} {:04X}:{:04X}, access {})",
            d.name, d.node, d.bus, d.vendor_id, d.product_id, d.access
        ),
        None => println!("Mouse:   not connected"),
    }
    if let Some(b) = &s.battery {
        println!(
            "Battery: {}{}",
            b.percent.map_or_else(
                || b.level.clone().unwrap_or_else(|| "?".into()),
                |p| format!("{p}%")
            ),
            b.status
                .as_deref()
                .map(|st| format!(", {}", st.to_lowercase()))
                .unwrap_or_default()
        );
    }
    println!();
    println!("  {:<22} {:<18} {:<18}", "setting", "wanted", "current");
    let wanted = |k: SettingKind| s.desired.iter().find(|v| v.kind() == k).copied();
    for k in SettingKind::ALL {
        let current = s.current.iter().find(|c| c.setting == k);
        let cur = match current {
            Some(c) => match (&c.value, &c.error) {
                (Some(v), _) => v.display(),
                (None, Some(_)) => "error".into(),
                (None, None) => "?".into(),
            },
            None => "-".into(),
        };
        let want = wanted(k).map(|v| v.display());
        let mark = match (&want, current.and_then(|c| c.value)) {
            (Some(_), Some(v)) if Some(v) == wanted(k) => "ok",
            (Some(_), Some(_)) => "DIFFERS",
            _ => "",
        };
        println!(
            "  {:<22} {:<18} {:<18} {mark}",
            k.name(),
            want.as_deref().unwrap_or("-"),
            cur
        );
    }
    let d = &s.desktop;
    let onoff = |b: Option<bool>| b.map_or("?", |b| if b { "on" } else { "off" });
    println!(
        "\nGNOME:   natural-scroll {}, acceleration {}",
        onoff(d.natural_scroll),
        d.acceleration.as_deref().unwrap_or("?")
    );
    if let Some(e) = &d.error {
        println!("         {e}");
    }
    let off = wanted(SettingKind::SmartshiftThreshold)
        == Some(
            hidlab_features::settings::SettingValue::SmartshiftThreshold(
                hidlab_features::settings::THRESHOLD_OFF,
            ),
        );
    if let (true, Some(t)) = (off, s.saved_threshold) {
        println!(
            "\nSmartShift switching is off; `hid-lab set smartshift-threshold on` restores {t}."
        );
    }
    if let Some(e) = s.current.iter().find_map(|c| c.error.as_deref()) {
        println!("\nCould not read the mouse: {e}");
    }
    match &s.last_apply {
        Some(r) => {
            println!(
                "\nLast apply: {:?}, {}: {} setting(s) {} {} ms after the event",
                r.trigger,
                ago(r.at_unix),
                r.report.items.len(),
                if r.report.ok {
                    "verified"
                } else {
                    "with problems"
                },
                r.since_event_ms
            );
            if let Some(c) = &r.second_check {
                let what = match (&c.reapplied, c.ok) {
                    (None, _) => "nothing overwritten".to_owned(),
                    (Some(_), ok) => format!(
                        "{} overwritten ({}), re-applied: {}",
                        c.overwritten.len(),
                        c.overwritten
                            .iter()
                            .map(|o| o.setting.name())
                            .collect::<Vec<_>>()
                            .join(", "),
                        if ok { "verified" } else { "FAILED" }
                    ),
                };
                println!(
                    "            second check {} ms after the event: {what}",
                    c.since_event_ms
                );
            }
            println!(
                "            {} ms in total; the target is 2000 ms",
                r.total_ms()
            );
        }
        None => println!("\nLast apply: none since the daemon started"),
    }
    if let Some(e) = &s.config_error {
        println!("\nWARNING: config file error; the last valid configuration is in use:\n{e}");
    }
    if let Some(e) = &s.state_error {
        println!("\nWARNING: {e}");
    }
    for c in &s.conflicts {
        println!("\nWARNING: {}", c.advice());
    }
    Ok(())
}

pub fn set(args: &SetArgs) -> Result<(), String> {
    let kind = match args.setting {
        SetTarget::Device(k) => k,
        SetTarget::Desktop(d) => {
            let reply: DesktopReply = call("SetDesktop", &(d.name(), args.raw_value.as_str()))?;
            if args.json {
                print_json(&reply)?;
            } else {
                println!("GNOME {} = {}", d.name(), reply.value);
            }
            return Ok(());
        }
    };
    let reply: SetReply = call("Set", &(kind.name(), args.raw_value.as_str()))?;
    if args.json {
        print_json(&reply)?;
    } else {
        if let Some(r) = &reply.applied {
            print_report(r);
        }
        if let Some(n) = &reply.note {
            println!("{n}");
        }
        if let Some(p) = &reply.saved_to {
            println!("Saved to {p}");
        }
    }
    match &reply.applied {
        Some(r) => report_result(r),
        None => Ok(()),
    }
}

pub fn reset(args: &ResetArgs) -> Result<(), String> {
    let name = args.setting.map_or("all", |k| k.name());
    let reply: ResetReply = call("Reset", &(name,))?;
    if args.json {
        print_json(&reply)?;
    } else {
        if let Some(r) = &reply.applied {
            print_report(r);
        }
        if let Some(n) = &reply.note {
            println!("{n}");
        }
        for p in &reply.saved_to {
            println!("Updated {p}");
        }
    }
    match &reply.applied {
        Some(r) => report_result(r),
        None => Ok(()),
    }
}

/// `hid-lab buttons`: renews the daemon's lease every 20 s while listening.
pub fn buttons(args: &HotplugArgs) -> Result<(), String> {
    use std::io::Write as _;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    const RENEW: Duration = Duration::from_secs(20);
    let rx = subscribe("ButtonPressed")?;
    let end = Instant::now() + args.duration;
    let mut renew_at = Instant::now();
    if !args.json {
        println!(
            "Press mouse buttons; listening for {} s.",
            args.duration.as_secs()
        );
    }
    let mut out = std::io::stdout().lock();
    loop {
        let now = Instant::now();
        if now >= end {
            break;
        }
        if now >= renew_at {
            let _: u32 = call_value("WatchButtons", &(30u32,))?;
            renew_at = now + RENEW;
        }
        let wait = end.min(renew_at).saturating_duration_since(now);
        let body = match rx.recv_timeout(wait) {
            Ok(b) => b,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err("the D-Bus connection closed".into());
            }
        };
        let line = if args.json {
            body
        } else {
            let ev: ButtonEvent = serde_json::from_str(&body).map_err(|e| {
                format!("hid-labd sent a button event this hid-lab cannot read: {e}")
            })?;
            format!(
                "{:<8} {:<5} {}",
                ev.control.as_deref().unwrap_or("unknown"),
                if ev.pressed { "down" } else { "up" },
                ev.number
                    .map_or_else(|| "(ratchet event)".into(), |n| format!("button {n}"))
            )
        };
        // A closed pipe (for example `| head`) ends quietly.
        if writeln!(out, "{line}").is_err() {
            break;
        }
    }
    drop(out);
    // Best effort: stop the lease; it also ends by itself.
    let _: Result<u32, String> = call_value("WatchButtons", &(0u32,));
    Ok(())
}

pub fn reapply(json: bool) -> Result<(), String> {
    let reply: ReapplyReply = call("Reapply", &())?;
    if json {
        print_json(&reply)?;
    } else {
        if let Some(e) = &reply.config_error {
            println!(
                "WARNING: config file error; the last valid configuration was applied:\n{e}\n"
            );
        }
        print_report(&reply.report);
    }
    report_result(&reply.report)
}
