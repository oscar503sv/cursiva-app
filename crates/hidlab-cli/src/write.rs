//! `hid-lab lab write`: controlled write experiments (requirements §13).

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hidlab_features::lab_write::{
    self, SettingKind, SettingValue, Snapshot, WriteOutcome, WritePlan,
};
use hidlab_features::{FeatureMap, Reader, WriteIntent, write_allowed};
use hidlab_hidpp::{Disposition, Session, TranscriptEntry, reference_name};
use hidlab_transport::{Hidraw, HidrawDevice, discover};
use serde::{Deserialize, Serialize};

use crate::args::{WriteArgs, WriteMode};
use crate::output::DeviceJson;
use crate::target::{select, session_config};

/// The value a setting had before the first experimental write.
#[derive(Debug, Serialize, Deserialize)]
struct OriginalRecord {
    schema: String,
    setting: SettingKind,
    original: SettingValue,
    vendor_id: u16,
    product_id: u16,
    saved_at_unix: u64,
}

#[derive(Serialize)]
struct WriteJson {
    schema: &'static str,
    tool_version: &'static str,
    mode: &'static str,
    restore: bool,
    device: DeviceJson,
    setting: SettingKind,
    feature: String,
    feature_index: Option<u8>,
    write_function: Option<WriteFnJson>,
    before: Option<SettingValue>,
    target: Option<SettingValue>,
    plan: Option<WritePlan>,
    sent: bool,
    outcome: Option<WriteOutcome>,
    /// `pass`, `mismatch`, `no_change`, `dry_run` or `error`.
    result: &'static str,
    original_saved_to: Option<String>,
    original_removed: bool,
    notes: Vec<String>,
    error: Option<String>,
    ignored_reports: u64,
    transcript: Vec<TranscriptEntry>,
}

#[derive(Serialize)]
struct WriteFnJson {
    function: u8,
    name: &'static str,
    source: &'static str,
    status: &'static str,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `$XDG_STATE_HOME/hid-lab/lab-write`, default `~/.local/state/hid-lab/lab-write`.
pub fn state_dir() -> Result<PathBuf, String> {
    let base = match std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        Some(v) => PathBuf::from(v),
        None => {
            let home = std::env::var_os("HOME")
                .ok_or("HOME is not set; cannot find a place to save the original value")?;
            PathBuf::from(home).join(".local/state")
        }
    };
    Ok(base.join("hid-lab/lab-write"))
}

fn record_path(dir: &Path, kind: SettingKind) -> PathBuf {
    dir.join(format!("{}.json", kind.name()))
}

fn load_record(path: &Path) -> Result<Option<OriginalRecord>, String> {
    match fs::read_to_string(path) {
        Ok(t) => serde_json::from_str(&t)
            .map(Some)
            .map_err(|e| format!("{} is damaged: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

fn save_record(path: &Path, rec: &OriginalRecord) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(rec).map_err(|e| e.to_string())?;
    fs::write(path, text + "\n").map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn tilde(p: &Path) -> String {
    match std::env::var_os("HOME") {
        Some(h) => match p.strip_prefix(&h) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => p.display().to_string(),
        },
        None => p.display().to_string(),
    }
}

pub fn run(args: &WriteArgs) -> Result<(), String> {
    let dir = state_dir()?;
    let record_file = record_path(&dir, args.setting);
    let discovery = discover().map_err(|e| e.to_string())?;
    let (number, device) = select(&discovery, args.device.as_deref())?;
    let config = session_config(device, None, args.timeout);
    let channel = Hidraw::open(&device.node).map_err(|e| e.to_string())?;
    let mut session = Session::new(channel, config);

    let mut out = WriteJson {
        schema: "hid-lab.write/1",
        tool_version: env!("CARGO_PKG_VERSION"),
        mode: match args.mode {
            WriteMode::DryRun => "dry_run",
            WriteMode::Write => "write",
        },
        restore: args.value.is_none(),
        device: DeviceJson::new(number, device, false),
        setting: args.setting,
        feature: args.setting.feature().to_string(),
        feature_index: None,
        write_function: write_allowed(args.setting.feature(), args.setting.write_function()).map(
            |w| WriteFnJson {
                function: w.function,
                name: w.name,
                source: w.source,
                status: "LIKELY",
            },
        ),
        before: None,
        target: args.value,
        plan: None,
        sent: false,
        outcome: None,
        result: "error",
        original_saved_to: None,
        original_removed: false,
        notes: Vec::new(),
        error: None,
        ignored_reports: 0,
        transcript: Vec::new(),
    };

    if !args.json {
        println!(
            "Target:  #{number} {}  {}  ({} {:04X}:{:04X})",
            device.node.display(),
            device.uevent.name.as_deref().unwrap_or("(no name)"),
            device.uevent.bus.label(),
            device.uevent.vendor_id,
            device.uevent.product_id
        );
    }

    let result = experiment(&mut session, args, device, &record_file, &mut out);
    out.ignored_reports = session.ignored();
    out.transcript = session.into_parts().1.entries;
    if let Err(e) = &result {
        out.error = Some(e.clone());
        out.result = "error";
    }

    if args.mode == WriteMode::Write {
        append_log(&dir, &out);
    }

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        print_text(&out);
    }
    let what = if out.sent {
        "the write was sent"
    } else {
        "nothing was written"
    };
    match (result, out.result) {
        // In text mode the details were already printed above.
        (Err(e), _) if args.json => Err(e),
        (Err(_), _) => Err(format!("{what}; see the error above")),
        (Ok(()), "mismatch") => Err("the read-back does not match; see the checks above".into()),
        _ => Ok(()),
    }
}

fn experiment(
    session: &mut Session<Hidraw>,
    args: &WriteArgs,
    device: &HidrawDevice,
    record_file: &Path,
    out: &mut WriteJson,
) -> Result<(), String> {
    let kind = args.setting;
    let map = FeatureMap::resolve(session, &[kind.feature()]).map_err(|e| e.to_string())?;
    out.feature_index = map.get(kind.feature()).map(|i| i.index);
    if out.feature_index.is_none() {
        return Err(format!(
            "this device does not report feature {} ({})",
            kind.feature(),
            reference_name(kind.feature()).unwrap_or("unknown")
        ));
    }
    let mut r = Reader::new(session, map);
    let before: Snapshot = lab_write::snapshot(&mut r, kind).map_err(|e| e.to_string())?;
    let current = before.value(kind).ok_or("cannot read the current value")?;
    out.before = Some(current);

    // Target: the requested value, or the saved original for --restore.
    let record = load_record(record_file)?;
    let target = match args.value {
        Some(v) => v,
        None => {
            let rec = record.as_ref().ok_or_else(|| {
                format!(
                    "no saved original for {}; nothing to restore ({} does not exist)",
                    kind.name(),
                    tilde(record_file)
                )
            })?;
            if (rec.vendor_id, rec.product_id)
                != (device.uevent.vendor_id, device.uevent.product_id)
            {
                return Err(format!(
                    "the saved original belongs to {:04X}:{:04X}, not to this device",
                    rec.vendor_id, rec.product_id
                ));
            }
            rec.original
        }
    };
    out.target = Some(target);

    if target == current {
        out.result = "no_change";
        out.notes.push(format!(
            "{} is already {}; nothing was written",
            kind.name(),
            target.display()
        ));
        if args.value.is_none() && args.mode == WriteMode::Write {
            fs::remove_file(record_file).map_err(|e| e.to_string())?;
            out.original_removed = true;
        }
        return Ok(());
    }

    let plan = lab_write::plan(&before, target)?;
    out.plan = Some(plan.clone());
    if args.mode == WriteMode::DryRun {
        out.result = "dry_run";
        return Ok(());
    }

    // Save the original once, before the first write, so --restore always
    // returns to the value the mouse had before any experiment.
    if args.value.is_some() {
        if record.is_none() {
            save_record(
                record_file,
                &OriginalRecord {
                    schema: "hid-lab.lab-write-original/1".into(),
                    setting: kind,
                    original: current,
                    vendor_id: device.uevent.vendor_id,
                    product_id: device.uevent.product_id,
                    saved_at_unix: now_unix(),
                },
            )?;
            out.original_saved_to = Some(tilde(record_file));
        } else {
            out.notes.push(format!(
                "an original from an earlier experiment is kept in {}",
                tilde(record_file)
            ));
        }
    }

    let intent = WriteIntent::explicit_user_request();
    out.sent = true;
    let outcome = lab_write::execute(&mut r, &intent, &before, plan).map_err(|e| e.to_string())?;
    out.result = if outcome.pass { "pass" } else { "mismatch" };
    if outcome.pass && args.value.is_none() {
        fs::remove_file(record_file).map_err(|e| e.to_string())?;
        out.original_removed = true;
    }
    out.outcome = Some(outcome);
    Ok(())
}

fn append_log(dir: &Path, out: &WriteJson) {
    let path = dir.join("log.jsonl");
    let line = match serde_json::to_string(out) {
        Ok(l) => l,
        Err(_) => return,
    };
    let res = fs::create_dir_all(dir).and_then(|_| {
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(f, "{line}")
    });
    if let Err(e) = res {
        eprintln!("warning: could not append to {}: {e}", path.display());
    }
}

fn print_text(o: &WriteJson) {
    let feature = o.feature.as_str();
    let name = o
        .feature
        .strip_prefix("0x")
        .and_then(|h| u16::from_str_radix(h, 16).ok())
        .and_then(|id| reference_name(hidlab_hidpp::FeatureId(id)))
        .unwrap_or("unknown");
    match (&o.write_function, o.feature_index) {
        (Some(w), Some(i)) => println!(
            "Setting: {} ({feature} {name}, index 0x{i:02x}); write function {} {} [{}, source: {}]",
            o.setting.name(),
            w.function,
            w.name,
            w.status,
            w.source
        ),
        _ => println!("Setting: {} ({feature} {name})", o.setting.name()),
    }
    if let Some(b) = o.before {
        println!("Before:  {}", b.display());
    }
    if let Some(t) = o.target {
        println!(
            "Target:  {}{}",
            t.display(),
            if o.restore { " (saved original)" } else { "" }
        );
    }
    if let Some(p) = &o.plan {
        println!(
            "Bytes:   function {} params {}{}",
            p.function,
            p.params_hex,
            if o.sent { "" } else { "  (not sent)" }
        );
    }
    if let Some(saved) = &o.original_saved_to {
        println!("Original saved to {saved}");
    }
    if let Some(out) = &o.outcome {
        println!(
            "Answer:  {}",
            if out.response_hex.is_empty() {
                "(empty)"
            } else {
                &out.response_hex
            }
        );
        if let Some(a) = out.after.value(o.setting) {
            println!("After:   {}", a.display());
        }
        for c in &out.checks {
            println!(
                "  {} {}: expected {}, read {}",
                if c.ok { "ok  " } else { "FAIL" },
                c.field,
                c.expected,
                c.actual
            );
        }
    }
    for n in &o.notes {
        println!("Note:    {n}");
    }
    if let Some(e) = &o.error {
        println!("Error:   {e}");
    }
    println!(
        "Result:  {}",
        match o.result {
            "pass" => "PASS",
            "mismatch" => "MISMATCH",
            "no_change" => "NO CHANGE",
            "dry_run" => "DRY RUN (nothing was written)",
            _ => "ERROR",
        }
    );
    if o.sent && !o.restore {
        let dev = o.device.hidraw.as_str();
        println!(
            "Restore: hid-lab lab write {} --restore {dev} --write",
            o.setting.name()
        );
    }
    println!();
    println!(
        "Transcript ({} unrelated report(s) ignored):",
        o.ignored_reports
    );
    for e in o
        .transcript
        .iter()
        .filter(|e| e.disposition != Disposition::Ignored)
    {
        println!(
            "  {:>8.1} ms  {:?}  {}",
            e.t_us as f64 / 1000.0,
            e.dir,
            e.hex
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_roundtrip_and_missing_file() {
        let dir = std::env::temp_dir().join(format!("hidlab-write-test-{}", std::process::id()));
        let path = record_path(&dir, SettingKind::SmartshiftThreshold);
        assert!(load_record(&path).unwrap().is_none());
        let rec = OriginalRecord {
            schema: "hid-lab.lab-write-original/1".into(),
            setting: SettingKind::SmartshiftThreshold,
            original: SettingValue::SmartshiftThreshold(10),
            vendor_id: 0x046D,
            product_id: 0xB034,
            saved_at_unix: 1,
        };
        save_record(&path, &rec).unwrap();
        let back = load_record(&path).unwrap().unwrap();
        assert_eq!(back.original, SettingValue::SmartshiftThreshold(10));
        assert!(path.ends_with("smartshift-threshold.json"));
        fs::write(&path, "{").unwrap();
        assert!(load_record(&path).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
