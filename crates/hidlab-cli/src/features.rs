//! `hid-lab lab features`: read-only HID++ 2.0 feature enumeration.

use hidlab_hidpp::{
    Enumeration, ReportKind, Session, SessionConfig, TranscriptEntry, enumerate, flag_labels,
    reference_name,
};
use hidlab_transport::{Hidraw, HidrawDevice, discover};
use serde::Serialize;

use crate::args::FeaturesArgs;
use crate::output::{DeviceJson, table};
use crate::target::{select, session_config};

#[derive(Serialize)]
struct FeaturesJson {
    schema: &'static str,
    tool_version: &'static str,
    read_only: bool,
    device: DeviceJson,
    session: SessionJson,
    protocol: Option<ProtocolJson>,
    feature_set_index: Option<u8>,
    count: Option<u8>,
    features: Vec<FeatureJson>,
    failures: Vec<FailureJson>,
    ignored_reports: u64,
    error: Option<String>,
    transcript: Vec<TranscriptEntry>,
}

#[derive(Serialize)]
struct SessionJson {
    report: ReportKind,
    report_id: String,
    device_index: String,
    sw_id: String,
    timeout_ms: u128,
}

#[derive(Serialize)]
struct ProtocolJson {
    major: u8,
    minor: u8,
}

#[derive(Serialize)]
struct FeatureJson {
    index: u8,
    id: String,
    reference_name: Option<&'static str>,
    reference_name_verified: bool,
    flags: u8,
    flag_labels: Vec<String>,
    version: u8,
}

#[derive(Serialize)]
struct FailureJson {
    index: u8,
    error: String,
}

pub fn run(args: &FeaturesArgs) -> Result<(), String> {
    let discovery = discover().map_err(|e| e.to_string())?;
    let (number, device) = select(&discovery, args.device.as_deref())?;

    let config = session_config(device, args.index, args.timeout);
    let report = config.report;

    let channel = Hidraw::open(&device.node).map_err(|e| e.to_string())?;
    let mut session = Session::new(channel, config);
    let result = enumerate(&mut session);
    let ignored = session.ignored();
    let (_, transcript) = session.into_parts();

    if args.json {
        let (protocol, fs_index, count, features, failures, error) = match &result {
            Ok(e) => (
                Some(ProtocolJson {
                    major: e.protocol.major,
                    minor: e.protocol.minor,
                }),
                Some(e.feature_set.index),
                Some(e.count),
                e.features.iter().map(feature_json).collect(),
                e.failures
                    .iter()
                    .map(|f| FailureJson {
                        index: f.index,
                        error: f.error.clone(),
                    })
                    .collect(),
                None,
            ),
            Err(e) => (None, None, None, vec![], vec![], Some(e.to_string())),
        };
        let out = FeaturesJson {
            schema: "hid-lab.features/1",
            tool_version: env!("CARGO_PKG_VERSION"),
            read_only: true,
            device: DeviceJson::new(number, device, args.show_unique),
            session: SessionJson {
                report,
                report_id: format!("0x{:02x}", report.report_id()),
                device_index: format!("0x{:02x}", config.device_index),
                sw_id: format!("0x{:02x}", config.sw_id),
                timeout_ms: config.timeout.as_millis(),
            },
            protocol,
            feature_set_index: fs_index,
            count,
            features,
            failures,
            ignored_reports: ignored,
            error,
            transcript: transcript.entries,
        };
        let text = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
        println!("{text}");
        return match result {
            Ok(_) => Ok(()),
            Err(_) => Err("enumeration failed; see the `error` field above".into()),
        };
    }

    print_header(number, device, &config);
    let e = result.map_err(|e| e.to_string())?;
    print_text(&e, ignored);
    Ok(())
}

fn feature_json(f: &hidlab_hidpp::FeatureInfo) -> FeatureJson {
    FeatureJson {
        index: f.index,
        id: f.id.to_string(),
        reference_name: reference_name(f.id),
        reference_name_verified: false,
        flags: f.flags,
        flag_labels: flag_labels(f.flags),
        version: f.version,
    }
}

fn print_header(number: usize, d: &HidrawDevice, c: &SessionConfig) {
    println!(
        "Device:   #{number} {}  {}",
        d.node.display(),
        d.uevent.name.as_deref().unwrap_or("(no name)")
    );
    println!(
        "          {} {:04X}:{:04X}, driver {}",
        d.uevent.bus.label(),
        d.uevent.vendor_id,
        d.uevent.product_id,
        d.driver.as_deref().unwrap_or("-")
    );
    println!(
        "Requests: {:?} report 0x{:02x}, device index 0x{:02x}, sw_id 0x{:02x}, timeout {} ms",
        c.report,
        c.report.report_id(),
        c.device_index,
        c.sw_id,
        c.timeout.as_millis()
    );
}

fn print_text(e: &Enumeration, ignored: u64) {
    println!(
        "HID++ protocol version: {}.{}",
        e.protocol.major, e.protocol.minor
    );
    println!(
        "IFeatureSet at index 0x{:02x}; {} features plus root",
        e.feature_set.index, e.count
    );
    println!();

    let rows: Vec<Vec<String>> = e
        .features
        .iter()
        .map(|f| {
            let labels = flag_labels(f.flags);
            let flags = if labels.is_empty() {
                format!("0x{:02x}", f.flags)
            } else {
                format!("0x{:02x} {}", f.flags, labels.join(","))
            };
            vec![
                format!("0x{:02x}", f.index),
                f.id.to_string(),
                reference_name(f.id).unwrap_or("unknown").to_owned(),
                flags,
                f.version.to_string(),
            ]
        })
        .collect();
    print!(
        "{}",
        table(
            &[
                "index",
                "feature ID",
                "reference name*",
                "type flags",
                "version"
            ],
            &rows
        )
    );

    for f in &e.failures {
        println!("index 0x{:02x}: {}", f.index, f.error);
    }
    println!();
    println!("* Reference names come from public sources and are not verified on this device.");
    println!("Read-only queries only: no device setting was changed.");
    println!("Ignored {ignored} unrelated report(s) while waiting for answers.");
}
